//! Folders opened "as is" (import mode C): a plain folder of Markdown is shown
//! as a **read-only** board without creating `.lull/` or renaming anything.
//!
//! The layout comes from [`super::vault::scan`] (subfolders → lanes, deeper
//! folders → group cards, `.md` → cards). Ids are derived from the relative
//! path (stable across reloads), and [`LooseLayout`] maps them back to the
//! real files. Every mutation is refused by the store (`read_only = "loose"`);
//! to edit, the user imports a copy (mode B).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::vault::{self, VaultNode};
use crate::app::Core;
use crate::error::{Error, Result};
use crate::fsutil::{mtime_ms, read_to_string, sha256_hex, short_hash};
use crate::markdown;
use crate::model::*;
use crate::store::BoardStore;

pub const READ_ONLY: &str = "loose";

/// Stable id (`<prefix>` + 6 base36 chars) for a vault-relative path.
pub fn stable_id(prefix: char, rel: &str) -> String {
    const A: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let h = sha256_hex(rel.as_bytes());
    let mut out = String::with_capacity(7);
    out.push(prefix);
    for b in hex::decode(&h[..12]).unwrap_or_default().iter().take(6) {
        out.push(A[(*b as usize) % 36] as char);
    }
    out
}

fn node_from(
    n: &VaultNode,
    parent: Parent,
    layout: &mut LooseLayout,
    nodes: &mut HashMap<String, Node>,
) -> String {
    let id = stable_id('c', &n.rel);
    let children: Vec<String> = n
        .children
        .iter()
        .map(|c| node_from(c, Parent::Card(id.clone()), layout, nodes))
        .collect();
    let (text, mtime, size) = match &n.file {
        Some(f) => {
            let meta = fs::metadata(f).ok();
            (
                read_to_string(f).unwrap_or_default(),
                meta.as_ref().map(mtime_ms).unwrap_or(0),
                meta.map(|m| m.len()).unwrap_or(0),
            )
        }
        None => (format!("# {}\n", n.name), 0, 0),
    };
    match &n.file {
        Some(f) => {
            layout.files.insert(id.clone(), f.clone());
        }
        None => {
            layout.synthetic.insert(id.clone(), text.clone());
        }
    }
    if let Some(d) = &n.dir {
        layout.dirs.insert(id.clone(), d.clone());
    }
    let mut meta = markdown::parse(&text);
    if meta.title.trim().is_empty() {
        meta.title = n.name.clone();
    }
    nodes.insert(
        id.clone(),
        Node {
            id: id.clone(),
            parent,
            is_group: n.dir.is_some(),
            children,
            archived: false,
            cover: None,
            meta,
            attachments: vec![],
            mtime,
            size,
            hash: short_hash(text.as_bytes()),
            group_extra: Default::default(),
        },
    );
    id
}

/// Build the read-only state of a plain folder.
pub fn load(root: &Path, id: &str) -> Result<BoardState> {
    let tree = vault::scan(root)?;
    let mut manifest = BoardManifest::new(id.to_string(), tree.name.clone(), tree.kind);
    manifest.created = None;
    let mut layout = LooseLayout::default();
    let mut nodes = HashMap::new();
    let mut lanes = Vec::new();
    for l in &tree.lanes {
        let k = stable_id('k', &l.rel);
        layout.dirs.insert(k.clone(), l.dir.clone());
        let order = l
            .items
            .iter()
            .map(|n| node_from(n, Parent::Lane(k.clone()), &mut layout, &mut nodes))
            .collect();
        manifest.lanes.push(k.clone());
        lanes.push(Lane {
            id: k,
            name: l.name.clone(),
            order,
            color: None,
            width: None,
            wip: None,
            collapsed: false,
            archived: false,
            extra: Default::default(),
        });
    }
    let mut root_order: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    if tree.kind == BoardKind::Kanban && !tree.root_items.is_empty() {
        warnings.push(format!("loose_root_notes:{}", tree.root_items.len()));
    } else {
        root_order = tree
            .root_items
            .iter()
            .map(|n| node_from(n, Parent::Root, &mut layout, &mut nodes))
            .collect();
    }
    if tree.truncated {
        warnings.push("loose_truncated".into());
    }
    Ok(BoardState {
        root: root.to_path_buf(),
        manifest,
        lanes,
        root_order,
        nodes,
        read_only: Some(READ_ONLY.into()),
        warnings,
        version: 1,
        loose: Some(Box::new(layout)),
    })
}

/// Reload hook used by `BoardStore::reload` for loose boards.
pub fn reload_state(st: &BoardState, _prev: &HashMap<String, Node>) -> Result<BoardState> {
    load(&st.root, &st.manifest.id)
}

impl Core {
    /// Open a plain folder read-only (import mode C). Re-uses its registry id.
    pub fn open_loose(self: &Arc<Self>, path: &Path) -> Result<BoardSnapshot> {
        if crate::store::is_board(path) {
            return self.open_board(path);
        }
        if !path.is_dir() {
            return Err(Error::invalid(format!("not a folder: {}", path.display())));
        }
        let path_s = path.to_string_lossy().into_owned();
        let existing = self
            .registry
            .lock()
            .boards
            .iter()
            .find(|e| e.loose && Path::new(&e.path) == path)
            .map(|e| e.id.clone());
        if let Some(id) = &existing
            && let Ok(b) = self.board(id)
        {
            return Ok(b.lock().state.snapshot());
        }
        let id = existing.unwrap_or_else(|| self.new_board_id());
        let state = load(path, &id)?;
        let snap = state.snapshot();
        let (name, kind) = (state.manifest.name.clone(), state.manifest.kind);
        self.update_registry(|r| {
            r.upsert(&id, &path_s, &name, kind, false);
            if let Some(e) = r.get_mut(&id) {
                e.loose = true;
                e.last_opened = Some(chrono::Utc::now().timestamp_millis());
            }
        });
        self.boards.write().insert(
            id.clone(),
            Arc::new(parking_lot::Mutex::new(BoardStore::from_state(state))),
        );
        crate::app::watch::watch_board(self, &id, path);
        Ok(snap)
    }

    /// `lull://` resolution for loose boards: any regular, non-hidden file
    /// inside the folder. `None` when `board` is not an open loose board.
    pub(crate) fn loose_board_file(&self, board: &str, rel: &str) -> Option<Result<PathBuf>> {
        let b = self.board(board).ok()?;
        let s = b.lock();
        s.state.loose.as_ref()?;
        let root = s.state.root.clone();
        drop(s);
        Some((|| {
            if rel.split('/').any(|c| c.starts_with('.')) {
                return Err(Error::invalid("hidden path"));
            }
            let p = crate::fsutil::safe_join(&root, rel)?;
            let meta = fs::symlink_metadata(&p).map_err(|e| Error::io(&p, e))?;
            if !meta.is_file() || !super::is_within(&p, &root) {
                return Err(Error::invalid("not a file in this folder"));
            }
            Ok(p)
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_read_only_with_stable_ids_and_never_writes() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        fs::create_dir_all(r.join("Todo/Group")).unwrap();
        fs::write(r.join("Todo/a.md"), "# Alpha\n#x\n").unwrap();
        fs::write(r.join("Todo/Group/b.md"), "plain body\n").unwrap();
        let st = load(r, "b000001").unwrap();
        assert_eq!(st.read_only.as_deref(), Some(READ_ONLY));
        assert_eq!(st.lanes.len(), 1);
        assert_eq!(st.lanes[0].name, "Todo");
        let a = stable_id('c', "Todo/a.md");
        assert_eq!(st.nodes[&a].meta.title, "Alpha");
        assert_eq!(st.node_file(&a).unwrap(), r.join("Todo/a.md"));
        let g = stable_id('c', "Todo/Group");
        assert!(st.nodes[&g].is_group);
        let b = stable_id('c', "Todo/Group/b.md");
        assert_eq!(st.nodes[&b].meta.title, "b");
        assert!(crate::ids::is_id(&a, crate::ids::IdKind::Card));
        let st2 = load(r, "b000001").unwrap();
        assert_eq!(st2.lanes[0].order, st.lanes[0].order);

        let mut store = BoardStore::from_state(st);
        assert_eq!(store.read_content(&g).unwrap(), "# Group\n");
        let err = store.apply(crate::store::Op::WriteCard {
            id: a.clone(),
            content: "# changed\n".into(),
        });
        assert!(matches!(err, Err(Error::ReadOnly(_))));
        assert_eq!(
            fs::read_to_string(r.join("Todo/a.md")).unwrap(),
            "# Alpha\n#x\n"
        );
        assert!(!r.join(crate::brand::MARKER_DIR).exists());
    }
}
