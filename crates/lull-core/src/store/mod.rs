//! File-backed board store.
//!
//! On-disk layout (see SPEC §4):
//! ```text
//! <root>/.lull/board.json
//! <root>/<laneId>/index.json
//! <root>/<laneId>/<cardId>.md                 plain card
//! <root>/<laneId>/<cardId>.<tok>[-name].<ext>  attachment of a plain card
//! <root>/<laneId>/<cardId>/index.md            group card
//! <root>/<laneId>/<cardId>/index.json          group order
//! ```
//! `index.json` order is the truth for ordering; the file system is the truth
//! for existence. Loading never rewrites card files.

pub mod ops;
pub mod trash;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;

use crate::brand::{BOARD_FILE, INDEX_JSON, INDEX_MD, MARKER_DIR, SCHEMA};
use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, mtime_ms, read_to_string, short_hash};
use crate::ids::{IdKind, is_id};
use crate::json_fmt;
use crate::markdown;
use crate::model::*;

pub use ops::{Applied, BoardPatch, LanePatch, Op, Placement};

static ATTACHMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(c[a-z0-9]{6})\.([a-z0-9]{4})(?:-(.+?))?\.([A-Za-z0-9]{1,12})$").unwrap());

pub fn marker_dir(root: &Path) -> PathBuf {
    root.join(MARKER_DIR)
}
pub fn manifest_path(root: &Path) -> PathBuf {
    marker_dir(root).join(BOARD_FILE)
}
pub fn is_board(root: &Path) -> bool {
    manifest_path(root).is_file()
}

/// Parse an attachment file name into (owner id, display name).
pub fn parse_attachment_name(name: &str) -> Option<(String, String)> {
    let c = ATTACHMENT_RE.captures(name)?;
    let display = match c.get(3) {
        Some(n) => format!("{}.{}", n.as_str(), &c[4]),
        None => format!("{}.{}", &c[2], &c[4]),
    };
    Some((c[1].to_string(), display))
}

/// Set of things changed by an operation; used to build UI deltas.
#[derive(Debug, Default, Clone)]
pub struct Changes {
    pub nodes: BTreeSet<String>,
    pub removed: BTreeSet<String>,
    pub lanes: bool,
    pub root: bool,
    pub header: bool,
}

impl Changes {
    pub fn touch_parent(&mut self, p: &Parent) {
        match p {
            Parent::Lane(_) => self.lanes = true,
            Parent::Card(c) => {
                self.nodes.insert(c.clone());
            }
            Parent::Root => self.root = true,
        }
    }
    pub fn merge(&mut self, o: Changes) {
        self.nodes.extend(o.nodes);
        self.removed.extend(o.removed);
        self.lanes |= o.lanes;
        self.root |= o.root;
        self.header |= o.header;
    }
}

#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub label: String,
    pub op: Op,
    pub coalesce: Option<String>,
}

pub struct BoardStore {
    pub state: BoardState,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    touched: VecDeque<(PathBuf, Instant)>,
}

const UNDO_LIMIT: usize = 300;
const TOUCH_WINDOW: Duration = Duration::from_millis(2500);

impl BoardStore {
    pub fn open(root: &Path) -> Result<Self> {
        let state = load_board(root, None)?;
        Ok(BoardStore { state, undo: vec![], redo: vec![], touched: VecDeque::new() })
    }

    /// Create a brand-new board at `root` (which may already contain files).
    pub fn create(root: &Path, id: String, name: String, kind: BoardKind) -> Result<Self> {
        if is_board(root) {
            return Err(Error::Conflict(format!("already a board: {}", root.display())));
        }
        fs::create_dir_all(marker_dir(root)).map_err(|e| Error::io(root, e))?;
        let manifest = BoardManifest::new(id, name, kind);
        write_json(&manifest_path(root), &manifest)?;
        write_marker_gitignore(root)?;
        Self::open(root)
    }

    pub fn id(&self) -> &str {
        &self.state.manifest.id
    }

    /// Re-read from disk (after external changes), reusing cached parses.
    pub fn reload(&mut self) -> Result<Changes> {
        let prev = std::mem::replace(&mut self.state.nodes, HashMap::new());
        let prev_lanes: Vec<LaneDto> = self.state.lanes.iter().map(LaneDto::from).collect();
        let prev_root = self.state.root_order.clone();
        let prev_header = self.state.header_dto();
        let mut next = match load_board(&self.state.root, Some(&prev)) {
            Ok(s) => s,
            Err(e) => {
                self.state.nodes = prev;
                return Err(e);
            }
        };
        next.version = self.state.version + 1;
        let mut ch = Changes::default();
        for (id, n) in &next.nodes {
            match prev.get(id) {
                Some(p) if p == n => {}
                _ => {
                    ch.nodes.insert(id.clone());
                }
            }
        }
        for id in prev.keys() {
            if !next.nodes.contains_key(id) {
                ch.removed.insert(id.clone());
            }
        }
        ch.lanes = next.lanes.iter().map(LaneDto::from).collect::<Vec<_>>() != prev_lanes;
        ch.root = next.root_order != prev_root;
        ch.header = next.header_dto() != prev_header;
        self.state = next;
        Ok(ch)
    }

    /// Record paths written by us so the watcher can ignore the echo.
    pub(crate) fn touch(&mut self, p: &Path) {
        let now = Instant::now();
        while self.touched.front().is_some_and(|(_, t)| now.duration_since(*t) > TOUCH_WINDOW) {
            self.touched.pop_front();
        }
        self.touched.push_back((p.to_path_buf(), now));
    }

    /// True when `p` (or a parent/child of it) was written by us recently.
    pub fn recently_touched(&self, p: &Path) -> bool {
        let now = Instant::now();
        self.touched.iter().any(|(t, at)| {
            now.duration_since(*at) <= TOUCH_WINDOW && (p.starts_with(t) || t.starts_with(p))
        })
    }

    pub fn delta(&self, ch: &Changes) -> BoardDelta {
        let s = &self.state;
        BoardDelta {
            board_id: s.manifest.id.clone(),
            version: s.version,
            header: ch.header.then(|| s.header_dto()),
            lanes: ch.lanes.then(|| s.lanes.iter().map(LaneDto::from).collect()),
            root_order: ch.root.then(|| s.root_order.clone()),
            nodes: ch.nodes.iter().filter_map(|id| s.nodes.get(id)).map(NodeDto::from).collect(),
            removed: ch.removed.iter().filter(|id| !s.nodes.contains_key(*id)).cloned().collect(),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    /// Apply a user operation and record its inverse for undo.
    pub fn apply_user(&mut self, op: Op, label: &str, coalesce: Option<String>) -> Result<Applied> {
        let applied = self.apply(op)?;
        if applied.changes_anything {
            let top_matches = coalesce.is_some()
                && self.undo.last().is_some_and(|e| e.coalesce == coalesce);
            if !top_matches {
                self.undo.push(UndoEntry { label: label.to_string(), op: applied.inverse.clone(), coalesce });
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
            }
            self.redo.clear();
        }
        Ok(applied)
    }

    /// Break coalescing so the next edit starts a new undo step.
    pub fn seal_undo(&mut self) {
        if let Some(e) = self.undo.last_mut() {
            e.coalesce = None;
        }
    }

    pub fn peek_undo(&self) -> Option<&UndoEntry> {
        self.undo.last()
    }
    pub fn peek_redo(&self) -> Option<&UndoEntry> {
        self.redo.last()
    }
    pub fn take_undo(&mut self) -> Option<UndoEntry> {
        self.undo.pop()
    }
    pub fn take_redo(&mut self) -> Option<UndoEntry> {
        self.redo.pop()
    }
    pub fn push_undo(&mut self, e: UndoEntry) {
        self.undo.push(e);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
    }
    pub fn push_redo(&mut self, e: UndoEntry) {
        self.redo.push(e);
    }
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }

    pub fn undo(&mut self) -> Result<Option<(String, Applied)>> {
        let Some(entry) = self.undo.pop() else { return Ok(None) };
        let applied = self.apply(entry.op)?;
        self.redo.push(UndoEntry { label: entry.label.clone(), op: applied.inverse.clone(), coalesce: None });
        Ok(Some((entry.label, applied)))
    }

    pub fn redo(&mut self) -> Result<Option<(String, Applied)>> {
        let Some(entry) = self.redo.pop() else { return Ok(None) };
        let applied = self.apply(entry.op)?;
        self.undo.push(UndoEntry { label: entry.label.clone(), op: applied.inverse.clone(), coalesce: None });
        Ok(Some((entry.label, applied)))
    }

    // --- persistence -----------------------------------------------------

    pub(crate) fn save_manifest(&mut self) -> Result<()> {
        let s = &mut self.state;
        s.manifest.lanes = s.lanes.iter().map(|l| l.id.clone()).collect();
        s.manifest.archived_lanes = s.lanes.iter().filter(|l| l.archived).map(|l| l.id.clone()).collect();
        s.manifest.order = if s.manifest.kind == BoardKind::Files { s.root_order.clone() } else { vec![] };
        s.manifest.archived = s.root_order.iter().filter(|id| s.nodes.get(*id).is_some_and(|n| n.archived)).cloned().collect();
        s.manifest.covers = s
            .root_order
            .iter()
            .filter_map(|id| s.nodes.get(id).and_then(|n| n.cover.clone().map(|c| (id.clone(), c))))
            .collect();
        let p = manifest_path(&s.root);
        let m = s.manifest.clone();
        write_json(&p, &m)?;
        self.touch(&p);
        Ok(())
    }

    pub(crate) fn save_container(&mut self, parent: &Parent) -> Result<()> {
        let s = &self.state;
        let (path, idx) = match parent {
            Parent::Root => return self.save_manifest(),
            Parent::Lane(k) => {
                let Some(l) = s.lane(k) else { return Ok(()) };
                let idx = ContainerIndex {
                    schema: SCHEMA,
                    id: l.id.clone(),
                    name: Some(l.name.clone()),
                    order: l.order.clone(),
                    archived: archived_of(s, &l.order),
                    covers: covers_of(s, &l.order),
                    color: l.color.clone(),
                    width: l.width,
                    wip: l.wip,
                    collapsed: l.collapsed,
                    extra: l.extra.clone(),
                };
                (s.root.join(k).join(INDEX_JSON), idx)
            }
            Parent::Card(c) => {
                let Some(n) = s.nodes.get(c) else { return Ok(()) };
                if !n.is_group {
                    return Ok(());
                }
                let Some(dir) = s.attachment_dir(c) else { return Ok(()) };
                let idx = ContainerIndex {
                    schema: SCHEMA,
                    id: c.clone(),
                    order: n.children.clone(),
                    archived: archived_of(s, &n.children),
                    covers: covers_of(s, &n.children),
                    extra: n.group_extra.clone(),
                    ..Default::default()
                };
                (dir.join(INDEX_JSON), idx)
            }
        };
        write_json(&path, &idx)?;
        self.touch(&path);
        Ok(())
    }

    /// Write a node's Markdown and refresh its metadata.
    pub(crate) fn write_node_content(&mut self, id: &str, content: &str) -> Result<()> {
        let path = self.state.node_file(id).ok_or_else(|| Error::not_found(id))?;
        let normalized = markdown::normalize(content);
        atomic_write(&path, normalized.as_bytes())?;
        self.touch(&path);
        self.refresh_node_meta(id, &normalized)?;
        Ok(())
    }

    pub(crate) fn refresh_node_meta(&mut self, id: &str, content: &str) -> Result<()> {
        let path = self.state.node_file(id).ok_or_else(|| Error::not_found(id))?;
        let meta = fs::metadata(&path).ok();
        let n = self.state.nodes.get_mut(id).ok_or_else(|| Error::not_found(id))?;
        n.meta = markdown::parse(content);
        n.hash = short_hash(content.as_bytes());
        if let Some(m) = meta {
            n.mtime = mtime_ms(&m);
            n.size = m.len();
        }
        Ok(())
    }

    pub fn read_content(&self, id: &str) -> Result<String> {
        let path = self.state.node_file(id).ok_or_else(|| Error::not_found(id))?;
        if !path.exists() {
            return Ok(String::new());
        }
        read_to_string(&path)
    }

    /// Re-scan the attachments of a node from disk.
    pub(crate) fn rescan_attachments(&mut self, id: &str) {
        let Some(dir) = self.state.attachment_dir(id) else { return };
        let list = list_attachments(&dir, id);
        if let Some(n) = self.state.nodes.get_mut(id) {
            n.attachments = list;
        }
    }
}

fn archived_of(s: &BoardState, ids: &[String]) -> Vec<String> {
    ids.iter().filter(|id| s.nodes.get(*id).is_some_and(|n| n.archived)).cloned().collect()
}

fn covers_of(s: &BoardState, ids: &[String]) -> std::collections::BTreeMap<String, Cover> {
    ids.iter().filter_map(|id| s.nodes.get(id).and_then(|n| n.cover.clone().map(|c| (id.clone(), c)))).collect()
}

pub(crate) fn write_json<T: serde::Serialize>(path: &Path, v: &T) -> Result<()> {
    let s = json_fmt::to_string(v).map_err(|e| Error::Json { path: path.into(), source: e })?;
    atomic_write(path, s.as_bytes())
}

pub(crate) fn write_marker_gitignore(root: &Path) -> Result<()> {
    let p = marker_dir(root).join(".gitignore");
    if !p.exists() {
        atomic_write(&p, b"# Lull: disposable caches\ncache/\n")?;
    }
    Ok(())
}

pub fn list_attachments(dir: &Path, id: &str) -> Vec<Attachment> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some((owner, display)) = parse_attachment_name(&name) {
            if owner == id {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                let ext = name.rsplit('.').next().unwrap_or("");
                out.push(Attachment { kind: AttachmentKind::from_ext(ext), file: name, display, size });
            }
        }
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

// ---------------------------------------------------------------------------
// Loading & recovery
// ---------------------------------------------------------------------------

struct LoadCtx<'a> {
    nodes: HashMap<String, Node>,
    warnings: Vec<String>,
    cache: Option<&'a HashMap<String, Node>>,
    root: PathBuf,
}

fn read_json_or_recover<T: serde::de::DeserializeOwned + Default>(path: &Path, root: &Path, warnings: &mut Vec<String>) -> T {
    let Ok(text) = fs::read_to_string(path) else { return T::default() };
    match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            let rec = marker_dir(root).join("cache").join("recovered");
            let _ = fs::create_dir_all(&rec);
            let name = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace(['/', '\\'], "_");
            let _ = fs::write(rec.join(format!("{}-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S"), name)), &text);
            warnings.push(format!("recovered:{}:{}", path.strip_prefix(root).unwrap_or(path).display(), e));
            T::default()
        }
    }
}

pub fn load_board(root: &Path, cache: Option<&HashMap<String, Node>>) -> Result<BoardState> {
    let mp = manifest_path(root);
    if !mp.is_file() {
        return Err(Error::NotABoard(root.to_path_buf()));
    }
    let mut warnings = Vec::new();
    let text = read_to_string(&mp)?;
    let manifest: BoardManifest = match serde_json::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            // Salvage the id so links and registry keep working.
            let id = Regex::new(r#""id"\s*:\s*"(b[a-z0-9]{6})""#)
                .unwrap()
                .captures(&text)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| crate::ids::new_id(IdKind::Board, |_| false));
            let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let rec = marker_dir(root).join("cache").join("recovered");
            let _ = fs::create_dir_all(&rec);
            let _ = fs::write(rec.join(format!("{}-board.json", chrono::Utc::now().format("%Y%m%dT%H%M%S"))), &text);
            warnings.push(format!("recovered:board.json:{e}"));
            let m = BoardManifest::new(id, name, BoardKind::Kanban);
            write_json(&mp, &m)?;
            m
        }
    };
    let read_only = (manifest.schema > SCHEMA).then(|| format!("newer_schema:{}", manifest.schema));

    let mut ctx = LoadCtx { nodes: HashMap::new(), warnings, cache, root: root.to_path_buf() };
    let mut lanes = Vec::new();
    let mut root_order = Vec::new();

    match manifest.kind {
        BoardKind::Kanban => {
            let mut present: Vec<String> = fs::read_dir(root)
                .map_err(|e| Error::io(root, e))?
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_id(n, IdKind::Lane))
                .collect();
            present.sort();
            let mut order: Vec<String> = manifest.lanes.iter().filter(|k| present.contains(k)).cloned().collect();
            order.dedup();
            for k in &present {
                if !order.contains(k) {
                    order.push(k.clone());
                }
            }
            for k in order {
                let dir = root.join(&k);
                let idx: ContainerIndex = read_json_or_recover(&dir.join(INDEX_JSON), root, &mut ctx.warnings);
                let (children, _) = scan_container(&mut ctx, &dir, Parent::Lane(k.clone()), &idx, None);
                lanes.push(Lane {
                    id: k.clone(),
                    name: idx.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Untitled".into()),
                    order: children,
                    color: idx.color.clone(),
                    width: idx.width,
                    wip: idx.wip,
                    collapsed: idx.collapsed,
                    archived: manifest.archived_lanes.contains(&k),
                    extra: idx.extra.clone(),
                });
            }
        }
        BoardKind::Files => {
            let idx = ContainerIndex {
                order: manifest.order.clone(),
                archived: manifest.archived.clone(),
                covers: manifest.covers.clone(),
                ..Default::default()
            };
            let (children, _) = scan_container(&mut ctx, root, Parent::Root, &idx, None);
            root_order = children;
        }
    }

    Ok(BoardState {
        root: root.to_path_buf(),
        manifest,
        lanes,
        root_order,
        nodes: ctx.nodes,
        read_only,
        warnings: ctx.warnings,
        version: 1,
    })
}

/// Scan a container directory. Returns ordered child ids and the attachments
/// found for `own_id` (the group itself) when scanning a group directory.
fn scan_container(
    ctx: &mut LoadCtx,
    dir: &Path,
    parent: Parent,
    idx: &ContainerIndex,
    own_id: Option<&str>,
) -> (Vec<String>, Vec<Attachment>) {
    scan_container_only(ctx, dir, parent, idx, own_id, None)
}

/// Like [`scan_container`] but restricted to a single child id when `only` is set.
fn scan_container_only(
    ctx: &mut LoadCtx,
    dir: &Path,
    parent: Parent,
    idx: &ContainerIndex,
    own_id: Option<&str>,
    only: Option<&str>,
) -> (Vec<String>, Vec<Attachment>) {
    let mut plain: HashMap<String, PathBuf> = HashMap::new();
    let mut groups: HashMap<String, PathBuf> = HashMap::new();
    let mut attachments: HashMap<String, Vec<Attachment>> = HashMap::new();
    let Ok(rd) = fs::read_dir(dir) else { return (vec![], vec![]) };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if let Some(o) = only {
            if name != o && name != format!("{o}.md") && !name.starts_with(&format!("{o}.")) {
                continue;
            }
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if is_id(&name, IdKind::Card) && !is_board(&e.path()) {
                groups.insert(name, e.path());
            }
        } else if let Some(stem) = name.strip_suffix(".md") {
            if is_id(stem, IdKind::Card) {
                plain.insert(stem.to_string(), e.path());
            }
        } else if let Some((owner, display)) = parse_attachment_name(&name) {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            let ext = name.rsplit('.').next().unwrap_or("").to_string();
            attachments.entry(owner).or_default().push(Attachment {
                kind: AttachmentKind::from_ext(&ext),
                file: name,
                display,
                size,
            });
        }
    }
    for v in attachments.values_mut() {
        v.sort_by(|a, b| a.file.cmp(&b.file));
    }
    for id in groups.keys() {
        if plain.remove(id).is_some() {
            ctx.warnings.push(format!("duplicate_file:{}", dir.join(format!("{id}.md")).display()));
        }
    }
    let mut present: Vec<String> = plain.keys().chain(groups.keys()).cloned().collect();
    present.sort();
    let mut order: Vec<String> = Vec::new();
    for id in &idx.order {
        if present.contains(id) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    for id in present {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    let mut result = Vec::new();
    for id in order {
        if ctx.nodes.contains_key(&id) {
            ctx.warnings.push(format!("duplicate_id:{id}"));
            continue;
        }
        let archived = idx.archived.contains(&id);
        let cover = idx.covers.get(&id).cloned();
        if let Some(gdir) = groups.get(&id) {
            let gidx: ContainerIndex = read_json_or_recover(&gdir.join(INDEX_JSON), &ctx.root.clone(), &mut ctx.warnings);
            let md = gdir.join(INDEX_MD);
            // Insert a placeholder first so children see their parent exists.
            let (meta, mtime, size, hash) = read_meta(ctx, &id, &md, true);
            ctx.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    parent: parent.clone(),
                    is_group: true,
                    children: vec![],
                    archived,
                    cover,
                    meta,
                    attachments: vec![],
                    mtime,
                    size,
                    hash,
                    group_extra: gidx.extra.clone(),
                },
            );
            let (children, own) = scan_container(ctx, gdir, Parent::Card(id.clone()), &gidx, Some(&id));
            if let Some(n) = ctx.nodes.get_mut(&id) {
                n.children = children;
                n.attachments = own;
            }
        } else if let Some(p) = plain.get(&id) {
            let (meta, mtime, size, hash) = read_meta(ctx, &id, p, false);
            ctx.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    parent: parent.clone(),
                    is_group: false,
                    children: vec![],
                    archived,
                    cover,
                    meta,
                    attachments: attachments.remove(&id).unwrap_or_default(),
                    mtime,
                    size,
                    hash,
                    group_extra: Default::default(),
                },
            );
        }
        result.push(id);
    }
    let own = own_id.and_then(|o| attachments.remove(o)).unwrap_or_default();
    (result, own)
}

/// Load a single node (and its subtree) found in `dir`.
pub(crate) fn load_node_tree(root: &Path, dir: &Path, parent: Parent, id: &str) -> Result<HashMap<String, Node>> {
    let mut ctx = LoadCtx { nodes: HashMap::new(), warnings: vec![], cache: None, root: root.to_path_buf() };
    let idx = ContainerIndex { order: vec![id.to_string()], ..Default::default() };
    scan_container_only(&mut ctx, dir, parent, &idx, None, Some(id));
    if !ctx.nodes.contains_key(id) {
        return Err(Error::not_found(format!("restored node {id}")));
    }
    Ok(ctx.nodes)
}

/// Load all children of a lane directory.
pub(crate) fn load_lane_children(root: &Path, k: &str, idx: &ContainerIndex) -> (Vec<String>, HashMap<String, Node>) {
    let mut ctx = LoadCtx { nodes: HashMap::new(), warnings: vec![], cache: None, root: root.to_path_buf() };
    let (order, _) = scan_container(&mut ctx, &root.join(k), Parent::Lane(k.to_string()), idx, None);
    (order, ctx.nodes)
}

fn read_meta(ctx: &LoadCtx, id: &str, path: &Path, is_group: bool) -> (markdown::ParsedCard, i64, u64, String) {
    let Ok(m) = fs::metadata(path) else {
        return (markdown::ParsedCard::default(), 0, 0, String::new());
    };
    let (mtime, size) = (mtime_ms(&m), m.len());
    if let Some(prev) = ctx.cache.and_then(|c| c.get(id)) {
        if prev.mtime == mtime && prev.size == size && prev.is_group == is_group && !prev.hash.is_empty() {
            return (prev.meta.clone(), mtime, size, prev.hash.clone());
        }
    }
    match read_to_string(path) {
        Ok(text) => (markdown::parse(&text), mtime, size, short_hash(text.as_bytes())),
        Err(_) => (markdown::ParsedCard::default(), mtime, size, String::new()),
    }
}

#[cfg(test)]
mod tests;
