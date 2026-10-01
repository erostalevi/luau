//! Application service: owns open boards, the registry, the search index and
//! history journaling. Framework-free; the Tauri layer is a thin adapter.

pub mod files;
pub mod registry;
mod trash_ops;
pub mod watch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::brand::MARKER_DIR;
use crate::error::{Error, Result};
use crate::fsutil::atomic_write;
use crate::history::{self, HistoryFilter, JournalEntry, Origin};
use crate::ids::{IdKind, new_id};
use crate::markdown;
use crate::model::*;
use crate::search::{self, SearchHit, SearchIndex, SearchOptions};
use crate::store::{self, BoardStore, Changes, Op, Placement, UndoEntry, trash};
pub use registry::{BoardEntry, Registry};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data: PathBuf,
    pub config: PathBuf,
    pub logs: PathBuf,
}

impl AppPaths {
    pub fn under(base: &Path) -> Self {
        AppPaths {
            data: base.join("data"),
            config: base.join("config"),
            logs: base.join("logs"),
        }
    }
    pub fn registry(&self) -> PathBuf {
        self.data.join("registry.json")
    }
    pub fn search_db(&self) -> PathBuf {
        self.data.join("search.db")
    }
    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn keybindings(&self) -> PathBuf {
        self.config.join("keybindings.json")
    }
    pub fn ui_state(&self) -> PathBuf {
        self.data.join("ui-state.json")
    }
    pub fn mirrors(&self) -> PathBuf {
        self.data.join("mirrors")
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum CoreEvent {
    #[serde(rename_all = "camelCase")]
    BoardDelta { delta: BoardDelta },
    #[serde(rename_all = "camelCase")]
    UndoState {
        board_id: String,
        can_undo: bool,
        can_redo: bool,
        undo_label: Option<String>,
        redo_label: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    RegistryChanged { registry: Registry },
    #[serde(rename_all = "camelCase")]
    ExternalChange { board_id: String, ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Toast {
        level: String,
        key: String,
        params: Value,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        task: String,
        done: usize,
        total: usize,
        label: Option<String>,
    },
    /// Generic channel for integrations, summaries, schedulers.
    #[serde(rename_all = "camelCase")]
    Custom { name: String, payload: Value },
}

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: CoreEvent);
}

pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _: CoreEvent) {}
}

struct PendingEdit {
    before: String,
    after: String,
    title: String,
    started: Instant,
    last: Instant,
}

/// Cross-board moves live outside a single store's op log.
#[derive(Debug, Clone)]
struct CrossMove {
    /// Where the items live *before* this move is applied.
    from_board: String,
    to_board: String,
    items: Vec<Placement>,
}

pub struct Core {
    pub paths: AppPaths,
    pub sink: Arc<dyn EventSink>,
    pub(crate) boards: RwLock<HashMap<String, Arc<Mutex<BoardStore>>>>,
    pub(crate) registry: Mutex<Registry>,
    pub search: SearchIndex,
    pub(crate) settings: RwLock<Value>,
    pending: Mutex<HashMap<(String, String), PendingEdit>>,
    cross: Mutex<HashMap<String, CrossMove>>,
    owners: Mutex<HashMap<String, String>>,
    pub(crate) watchers: Mutex<HashMap<String, notify::RecommendedWatcher>>,
    pub(crate) watch_tx: Mutex<Option<std::sync::mpsc::Sender<watch::WatchMsg>>>,
    scanning: std::sync::atomic::AtomicBool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub version: u64,
    pub created: Vec<String>,
    pub trashed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub label: Option<String>,
    pub done: bool,
}

fn setting_u64(v: &Value, key: &str, default: u64) -> u64 {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(default)
}
fn setting_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
}

impl Core {
    pub fn new(paths: AppPaths, sink: Arc<dyn EventSink>) -> Result<Arc<Self>> {
        for d in [&paths.data, &paths.config, &paths.logs] {
            std::fs::create_dir_all(d).map_err(|e| Error::io(d, e))?;
        }
        let registry = Registry::load(&paths.registry());
        let settings: Value = std::fs::read_to_string(paths.settings())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| json!({}));
        let search = match SearchIndex::open(&paths.search_db()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("search index unavailable, recreating: {e}");
                let _ = std::fs::remove_file(paths.search_db());
                SearchIndex::open(&paths.search_db())?
            }
        };
        let core = Arc::new(Core {
            paths,
            sink,
            boards: RwLock::new(HashMap::new()),
            registry: Mutex::new(registry),
            search,
            settings: RwLock::new(settings),
            pending: Mutex::new(HashMap::new()),
            cross: Mutex::new(HashMap::new()),
            owners: Mutex::new(HashMap::new()),
            watchers: Mutex::new(HashMap::new()),
            watch_tx: Mutex::new(None),
            scanning: std::sync::atomic::AtomicBool::new(false),
        });
        watch::start(&core);
        Ok(core)
    }

    // --- settings & small persisted files -------------------------------------

    pub fn settings(&self) -> Value {
        self.settings.read().clone()
    }

    pub fn set_settings(&self, v: Value) -> Result<()> {
        let s = crate::json_fmt::to_string(&v).unwrap_or_default();
        atomic_write(&self.paths.settings(), s.as_bytes())?;
        *self.settings.write() = v;
        Ok(())
    }

    pub fn read_json_file(&self, path: &Path) -> Value {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null)
    }

    pub fn write_json_file(&self, path: &Path, v: &Value) -> Result<()> {
        let s = crate::json_fmt::to_string(v).unwrap_or_default();
        atomic_write(path, s.as_bytes())
    }

    // --- ids ------------------------------------------------------------------

    pub fn id_taken(&self, id: &str) -> bool {
        self.boards
            .read()
            .values()
            .any(|b| b.lock().state.nodes.contains_key(id))
            || self.search.id_taken(id)
    }

    pub fn new_card_id(&self) -> String {
        new_id(IdKind::Card, |id| self.id_taken(id))
    }

    pub fn new_lane_id(&self, board: &str) -> String {
        let b = self.board(board).ok();
        new_id(IdKind::Lane, |id| {
            b.as_ref()
                .is_some_and(|b| b.lock().state.lane(id).is_some())
        })
    }

    pub fn new_board_id(&self) -> String {
        let reg = self.registry.lock();
        new_id(IdKind::Board, |id| reg.get(id).is_some())
    }

    // --- registry -------------------------------------------------------------

    pub fn registry(&self) -> Registry {
        self.registry.lock().clone()
    }

    fn save_registry(&self) {
        let reg = self.registry.lock().clone();
        if let Err(e) = reg.save(&self.paths.registry()) {
            tracing::warn!("saving registry: {e}");
        }
        self.sink.emit(CoreEvent::RegistryChanged { registry: reg });
    }

    pub fn update_registry(&self, f: impl FnOnce(&mut Registry)) {
        f(&mut self.registry.lock());
        self.save_registry();
    }

    fn register(&self, st: &BoardState) {
        let changed = self.registry.lock().upsert(
            &st.manifest.id,
            &st.root.to_string_lossy(),
            &st.manifest.name,
            st.manifest.kind,
            st.manifest.extra.contains_key("mirror"),
        );
        if changed {
            self.save_registry();
        }
    }

    // --- boards: lifecycle ------------------------------------------------------

    pub fn board(&self, id: &str) -> Result<Arc<Mutex<BoardStore>>> {
        self.boards
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| Error::not_found(format!("board {id} not open")))
    }

    pub fn open_board_ids(&self) -> Vec<String> {
        self.boards.read().keys().cloned().collect()
    }

    /// Create a board (folder may already exist). `lanes` seeds a kanban template.
    pub fn create_board(
        self: &Arc<Self>,
        path: &Path,
        name: &str,
        kind: BoardKind,
        lanes: &[String],
        git: bool,
    ) -> Result<BoardSnapshot> {
        if store::is_board(path) {
            return Err(Error::Conflict(format!(
                "already a board: {}",
                path.display()
            )));
        }
        if let Some(parent_board) = path.ancestors().skip(1).find(|p| store::is_board(p)) {
            return Err(Error::invalid(format!(
                "nested_board:{}",
                parent_board.display()
            )));
        }
        let id = self.new_board_id();
        let name = if name.trim().is_empty() {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Board".into())
        } else {
            name.trim().to_string()
        };
        let mut s = BoardStore::create(path, id.clone(), name, kind)?;
        if kind == BoardKind::Kanban {
            for l in lanes {
                let k = new_id(IdKind::Lane, |id| s.state.lane(id).is_some());
                s.apply(Op::CreateLane {
                    id: k,
                    name: l.clone(),
                    index: None,
                })?;
            }
        }
        if git {
            files::git_init(path);
        }
        drop(s);
        self.open_board(path)
    }

    /// Open by path (or re-use when already open). Returns a full snapshot.
    pub fn open_board(self: &Arc<Self>, path: &Path) -> Result<BoardSnapshot> {
        let found =
            crate::discovery::read_marker(path).ok_or_else(|| Error::NotABoard(path.into()))?;
        if let Some(b) = self.boards.read().get(&found.id) {
            let b = b.lock();
            if b.state.root == path {
                return Ok(b.state.snapshot());
            }
        }
        let mut store = BoardStore::open(path)?;
        let ttl = setting_u64(&self.settings(), "trash.ttlDays", 7) as u32;
        let _ = trash::purge(path, ttl.max(1), false);
        let snap = store.state.snapshot();
        self.register(&store.state);
        if let Some(e) = self.registry.lock().get_mut(&found.id) {
            e.last_opened = Some(chrono::Utc::now().timestamp_millis());
        }
        self.save_registry();
        let id = store.id().to_string();
        let _ = &mut store;
        self.boards
            .write()
            .insert(id.clone(), Arc::new(Mutex::new(store)));
        watch::watch_board(self, &id, path);
        let me = Arc::clone(self);
        let bid = id.clone();
        std::thread::spawn(move || {
            me.reindex_board(&bid);
            if let Ok(b) = me.board(&bid) {
                let s = me.settings();
                if setting_bool(&s, "files.autoCleanup", true) {
                    let ttl = setting_u64(&s, "files.unlinkedTtlDays", 7);
                    let _ = files::sweep_unlinked(&mut b.lock(), ttl, false);
                }
            }
        });
        Ok(snap)
    }

    pub fn open_board_by_id(self: &Arc<Self>, id: &str) -> Result<BoardSnapshot> {
        if let Ok(b) = self.board(id) {
            return Ok(b.lock().state.snapshot());
        }
        let path = self
            .registry
            .lock()
            .get(id)
            .map(|e| PathBuf::from(&e.path))
            .ok_or_else(|| Error::not_found(id))?;
        if self.registry.lock().get(id).is_some_and(|e| e.loose) && path.is_dir() {
            return self.open_loose(&path);
        }
        if !store::is_board(&path) {
            self.update_registry(|r| {
                if let Some(e) = r.get_mut(id) {
                    e.missing = true;
                }
            });
            return Err(Error::NotABoard(path));
        }
        self.open_board(&path)
    }

    pub fn close_board(&self, id: &str) {
        self.flush_edits(Some(id));
        self.boards.write().remove(id);
        self.watchers.lock().remove(id);
        self.owners.lock().remove(id);
    }

    pub fn snapshot(&self, id: &str) -> Result<BoardSnapshot> {
        Ok(self.board(id)?.lock().state.snapshot())
    }

    /// Claim a board for a window tab; returns the current owner when taken.
    pub fn claim(&self, board: &str, window: &str) -> Option<String> {
        let mut o = self.owners.lock();
        match o.get(board) {
            Some(w) if w != window => Some(w.clone()),
            _ => {
                o.insert(board.to_string(), window.to_string());
                None
            }
        }
    }

    pub fn release(&self, board: &str, window: &str) {
        let mut o = self.owners.lock();
        if o.get(board).is_some_and(|w| w == window) {
            o.remove(board);
        }
    }

    pub fn release_window(&self, window: &str) {
        self.owners.lock().retain(|_, w| w != window);
    }

    // --- boards: operations ------------------------------------------------------

    pub(crate) fn emit_delta(&self, store: &BoardStore, ch: &Changes) {
        let delta = store.delta(ch);
        if !delta.is_empty() {
            self.sink.emit(CoreEvent::BoardDelta { delta });
        }
        self.sink.emit(CoreEvent::UndoState {
            board_id: store.id().to_string(),
            can_undo: store.can_undo(),
            can_redo: store.can_redo(),
            undo_label: store.undo_label().map(str::to_string),
            redo_label: store.redo_label().map(str::to_string),
        });
    }

    /// Validate board-kind rules for user operations.
    fn check_kind_rules(st: &BoardState, op: &Op) -> Result<()> {
        let bad = |p: &Parent| {
            matches!(
                (st.manifest.kind, p),
                (BoardKind::Kanban, Parent::Root) | (BoardKind::Files, Parent::Lane(_))
            )
        };
        match op {
            Op::CreateCard { parent, .. } if bad(parent) => {
                Err(Error::invalid("parent not valid for this board type"))
            }
            Op::Move { to, .. } if bad(to) => {
                Err(Error::invalid("target not valid for this board type"))
            }
            Op::CreateLane { .. } if st.manifest.kind == BoardKind::Files => {
                Err(Error::invalid("files boards have no lanes"))
            }
            _ => Ok(()),
        }
    }

    pub fn apply(
        &self,
        board: &str,
        op: Op,
        label: &str,
        coalesce: Option<String>,
    ) -> Result<ApplyResult> {
        let b = self.board(board)?;
        let mut s = b.lock();
        Self::check_kind_rules(&s.state, &op)?;
        let before_ctx = journal_context(&s.state, &op);
        // Edits are journaled lazily (coalesced) by the pending-edit buffer.
        let edit_before = match &op {
            Op::WriteCard { id, .. } => Some((id.clone(), s.read_content(id).unwrap_or_default())),
            _ => None,
        };
        let kind = op.kind();
        let op_for_journal = op.clone();
        let applied = s.apply_user(op, label, coalesce)?;
        if applied.changes_anything {
            if let Some((id, before)) = edit_before {
                let after = s.read_content(&id).unwrap_or_default();
                let title = s
                    .state
                    .nodes
                    .get(&id)
                    .map(|n| n.meta.title.clone())
                    .unwrap_or_default();
                let root = s.state.root.clone();
                self.note_edit(&root, board, &id, before, after, title);
            } else {
                self.journal(&s.state, kind, label, &op_for_journal, &applied, before_ctx);
            }
            self.index_changes(&mut s, &applied.changes);
        }
        self.emit_delta(&s, &applied.changes);
        Ok(ApplyResult {
            version: s.state.version,
            created: applied.created,
            trashed: applied.trashed,
        })
    }

    pub fn undo(&self, board: &str) -> Result<UndoResult> {
        self.flush_edits(Some(board));
        let b = self.board(board)?;
        // Peek and pop under one lock: concurrent undos must not race.
        let external = {
            let mut s = b.lock();
            let top = matches!(
                s.peek_undo(),
                Some(UndoEntry {
                    op: Op::External { .. },
                    ..
                })
            );
            if top { s.take_undo() } else { None }
        };
        if let Some(entry) = external {
            let Op::External { token } = &entry.op else {
                return Err(Error::Other("undo stack changed".into()));
            };
            let token = token.clone();
            let label = entry.label.clone();
            let inverse = match self.run_cross(&token) {
                Ok(i) => i,
                Err(e) => {
                    if e.is_transient() {
                        b.lock().push_undo(entry);
                    }
                    return Err(e);
                }
            };
            b.lock().push_redo(UndoEntry {
                label: label.clone(),
                op: Op::External {
                    token: inverse.clone(),
                },
                coalesce: None,
            });
            self.emit_undo_state(board);
            return Ok(UndoResult {
                label: Some(label),
                done: true,
            });
        }
        let mut s = b.lock();
        match s.undo() {
            Ok(Some((label, applied))) => {
                self.journal_simple(
                    &s.state,
                    "undo",
                    &format!("Undo: {label}"),
                    &applied.changes,
                );
                self.index_changes(&mut s, &applied.changes);
                self.emit_delta(&s, &applied.changes);
                Ok(UndoResult {
                    label: Some(label),
                    done: true,
                })
            }
            Ok(None) => Ok(UndoResult {
                label: None,
                done: false,
            }),
            Err(e) => {
                self.emit_delta(&s, &Changes::default());
                Err(e)
            }
        }
    }

    pub fn redo(&self, board: &str) -> Result<UndoResult> {
        let b = self.board(board)?;
        let external = {
            let mut s = b.lock();
            let top = matches!(
                s.peek_redo(),
                Some(UndoEntry {
                    op: Op::External { .. },
                    ..
                })
            );
            if top { s.take_redo() } else { None }
        };
        if let Some(entry) = external {
            let Op::External { token } = &entry.op else {
                return Err(Error::Other("redo stack changed".into()));
            };
            let token = token.clone();
            let inverse = match self.run_cross(&token) {
                Ok(i) => i,
                Err(e) => {
                    if e.is_transient() {
                        b.lock().push_redo(entry);
                    }
                    return Err(e);
                }
            };
            b.lock().push_undo(UndoEntry {
                label: entry.label.clone(),
                op: Op::External {
                    token: inverse.clone(),
                },
                coalesce: None,
            });
            self.emit_undo_state(board);
            return Ok(UndoResult {
                label: Some(entry.label),
                done: true,
            });
        }
        let mut s = b.lock();
        match s.redo()? {
            Some((label, applied)) => {
                self.journal_simple(
                    &s.state,
                    "redo",
                    &format!("Redo: {label}"),
                    &applied.changes,
                );
                self.index_changes(&mut s, &applied.changes);
                self.emit_delta(&s, &applied.changes);
                Ok(UndoResult {
                    label: Some(label),
                    done: true,
                })
            }
            None => Ok(UndoResult {
                label: None,
                done: false,
            }),
        }
    }

    fn emit_undo_state(&self, board: &str) {
        if let Ok(b) = self.board(board) {
            let s = b.lock();
            self.emit_delta(&s, &Changes::default());
        }
    }

    /// End an editing session: next edit starts a new undo step; journal it.
    pub fn seal(&self, board: &str, card: Option<&str>) {
        if let Ok(b) = self.board(board) {
            b.lock().seal_undo();
        }
        match card {
            Some(c) => self.flush_edit(board, c),
            None => self.flush_edits(Some(board)),
        }
    }

    pub fn read_card(&self, board: &str, id: &str) -> Result<String> {
        self.board(board)?.lock().read_content(id)
    }

    pub fn write_card(
        &self,
        board: &str,
        id: &str,
        content: &str,
        session: Option<String>,
    ) -> Result<ApplyResult> {
        let key = session.map(|s| format!("edit:{id}:{s}"));
        self.apply(
            board,
            Op::WriteCard {
                id: id.to_string(),
                content: content.to_string(),
            },
            "Edit card",
            key,
        )
    }

    // --- cross-board moves ------------------------------------------------------

    /// Move `ids` from `from` into `to` board at (`parent`, before `before`).
    pub fn move_across(
        &self,
        from: &str,
        ids: &[String],
        to: &str,
        parent: Parent,
        before: Option<String>,
    ) -> Result<()> {
        if from == to {
            self.apply(
                from,
                Op::Move {
                    ids: ids.to_vec(),
                    to: parent,
                    before,
                },
                "Move",
                None,
            )?;
            return Ok(());
        }
        let target_index = {
            let b = self.board(to)?;
            let s = b.lock();
            Self::check_kind_rules(
                &s.state,
                &Op::Move {
                    ids: vec![],
                    to: parent.clone(),
                    before: None,
                },
            )?;
            let sib = s
                .state
                .children_of(&parent)
                .cloned()
                .ok_or_else(|| Error::not_found("target"))?;
            before
                .as_ref()
                .and_then(|x| sib.iter().position(|c| c == x))
                .unwrap_or(sib.len())
        };
        let items: Vec<Placement> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Placement {
                id: id.clone(),
                parent: parent.clone(),
                index: target_index + i,
            })
            .collect();
        let token = new_id(IdKind::Trash, |t| self.cross.lock().contains_key(t));
        self.cross.lock().insert(
            token.clone(),
            CrossMove {
                from_board: from.to_string(),
                to_board: to.to_string(),
                items,
            },
        );
        let inverse = self.run_cross(&token)?;
        let b = self.board(to)?;
        let mut s = b.lock();
        s.push_undo(UndoEntry {
            label: "Move to board".into(),
            op: Op::External {
                token: inverse.clone(),
            },
            coalesce: None,
        });
        s.clear_redo();
        drop(s);
        self.emit_undo_state(to);
        Ok(())
    }

    /// Execute a registered cross move; returns the token of its inverse.
    fn run_cross(&self, token: &str) -> Result<String> {
        let cm = self
            .cross
            .lock()
            .get(token)
            .cloned()
            .ok_or_else(|| Error::not_found("cross move"))?;
        let (a, b) = (self.board(&cm.from_board)?, self.board(&cm.to_board)?);
        // Lock in a stable order to avoid deadlocks.
        let (mut src, mut dst) = if cm.from_board < cm.to_board {
            let x = a.lock();
            let y = b.lock();
            (x, y)
        } else {
            let y = b.lock();
            let x = a.lock();
            (x, y)
        };
        let mut original = Vec::new();
        for it in &cm.items {
            if let Some((p, i)) = src.state.position_of(&it.id) {
                original.push(Placement {
                    id: it.id.clone(),
                    parent: p,
                    index: i,
                });
            }
        }
        let (src_ch, dst_ch) = files::transfer(&mut src, &mut dst, &cm.items)?;
        let titles: Vec<String> = cm
            .items
            .iter()
            .filter_map(|i| dst.state.nodes.get(&i.id).map(|n| n.meta.title.clone()))
            .collect();
        for (st, dir) in [(&src.state, "out"), (&dst.state, "in")] {
            let _ = history::append(
                &st.root,
                &JournalEntry {
                    ts: history::now(),
                    board: st.manifest.id.clone(),
                    kind: "moveBoard".into(),
                    origin: Origin::You,
                    label: if dir == "in" {
                        "Moved from another board".into()
                    } else {
                        "Moved to another board".into()
                    },
                    ids: cm.items.iter().map(|i| i.id.clone()).collect(),
                    details: json!({ "titles": titles, "from": cm.from_board, "to": cm.to_board }),
                    before: None,
                    after: None,
                },
            );
        }
        self.index_changes(&mut src, &src_ch);
        self.index_changes(&mut dst, &dst_ch);
        self.emit_delta(&src, &src_ch);
        self.emit_delta(&dst, &dst_ch);
        let inv = new_id(IdKind::Trash, |t| self.cross.lock().contains_key(t));
        self.cross.lock().insert(
            inv.clone(),
            CrossMove {
                from_board: cm.to_board.clone(),
                to_board: cm.from_board.clone(),
                items: original,
            },
        );
        Ok(inv)
    }

    // --- journaling --------------------------------------------------------------

    fn journal(
        &self,
        st: &BoardState,
        kind: &str,
        label: &str,
        op: &Op,
        applied: &store::Applied,
        before: Value,
    ) {
        let mut ids: Vec<String> = applied
            .changes
            .nodes
            .iter()
            .chain(applied.changes.removed.iter())
            .cloned()
            .collect();
        ids.extend(applied.created.iter().cloned());
        ids.sort();
        ids.dedup();
        let after = journal_context(st, op);
        let entry = JournalEntry {
            ts: history::now(),
            board: st.manifest.id.clone(),
            kind: kind.to_string(),
            origin: Origin::You,
            label: label.to_string(),
            ids,
            details: json!({ "before": before, "after": after }),
            before: None,
            after: None,
        };
        if let Err(e) = history::append(&st.root, &entry) {
            tracing::warn!("journal: {e}");
        }
    }

    fn journal_simple(&self, st: &BoardState, kind: &str, label: &str, ch: &Changes) {
        let _ = history::append(
            &st.root,
            &JournalEntry {
                ts: history::now(),
                board: st.manifest.id.clone(),
                kind: kind.into(),
                origin: Origin::You,
                label: label.into(),
                ids: ch.nodes.iter().chain(ch.removed.iter()).cloned().collect(),
                details: Value::Null,
                before: None,
                after: None,
            },
        );
    }

    /// Journal an arbitrary entry on a board (used by integrations).
    pub fn journal_entry(&self, board: &str, entry: JournalEntry) {
        if let Ok(b) = self.board(board) {
            let _ = history::append(&b.lock().state.root, &entry);
        }
    }

    /// Buffer an edit for coalesced journaling. Called while the board lock is
    /// held, so it must never lock the board again (`root` is passed in).
    fn note_edit(
        &self,
        root: &Path,
        board: &str,
        id: &str,
        before: String,
        after: String,
        title: String,
    ) {
        let now = Instant::now();
        let mut p = self.pending.lock();
        let key = (board.to_string(), id.to_string());
        let flush_old = p
            .get(&key)
            .is_some_and(|e| now.duration_since(e.started).as_secs() > 60);
        if flush_old {
            let old = p.remove(&key).unwrap();
            drop(p);
            Self::write_edit_at(root, board, id, old);
            p = self.pending.lock();
        }
        match p.get_mut(&key) {
            Some(e) => {
                e.after = after;
                e.title = title;
                e.last = now;
            }
            None => {
                p.insert(
                    key,
                    PendingEdit {
                        before,
                        after,
                        title,
                        started: now,
                        last: now,
                    },
                );
            }
        }
    }

    fn write_edit(&self, board: &str, id: &str, e: PendingEdit) {
        if e.before == e.after {
            return;
        }
        let Ok(b) = self.board(board) else { return };
        let root = b.lock().state.root.clone();
        Self::write_edit_at(&root, board, id, e);
    }

    /// Journal one coalesced edit. Takes no locks.
    fn write_edit_at(root: &Path, board: &str, id: &str, e: PendingEdit) {
        if e.before == e.after {
            return;
        }
        let root = root.to_path_buf();
        let before = history::put_blob(&root, &e.before).ok();
        let after = history::put_blob(&root, &e.after).ok();
        let lines_before = e.before.lines().count() as i64;
        let lines_after = e.after.lines().count() as i64;
        let _ = history::append(
            &root,
            &JournalEntry {
                ts: history::now(),
                board: board.to_string(),
                kind: "edit".into(),
                origin: Origin::You,
                label: "Edited card".into(),
                ids: vec![id.to_string()],
                details: json!({ "title": e.title, "chars": e.after.len() as i64 - e.before.len() as i64, "lines": lines_after - lines_before }),
                before,
                after,
            },
        );
    }

    pub fn flush_edit(&self, board: &str, id: &str) {
        let e = self
            .pending
            .lock()
            .remove(&(board.to_string(), id.to_string()));
        if let Some(e) = e {
            self.write_edit(board, id, e);
        }
    }

    /// Flush pending edits (all, or for one board). Also used on a timer.
    pub fn flush_edits(&self, board: Option<&str>) {
        let drained: Vec<((String, String), PendingEdit)> = {
            let mut p = self.pending.lock();
            let keys: Vec<(String, String)> = p
                .keys()
                .filter(|(b, _)| board.is_none_or(|x| x == b))
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|k| p.remove(&k).map(|v| (k, v)))
                .collect()
        };
        for ((b, id), e) in drained {
            self.write_edit(&b, &id, e);
        }
    }

    /// Flush edits idle for more than `secs`.
    pub fn flush_idle(&self, secs: u64) {
        let now = Instant::now();
        let idle: Vec<(String, String)> = self
            .pending
            .lock()
            .iter()
            .filter(|(_, e)| now.duration_since(e.last).as_secs() >= secs)
            .map(|(k, _)| k.clone())
            .collect();
        for (b, id) in idle {
            self.flush_edit(&b, &id);
        }
    }

    pub fn history(&self, board: &str, filter: &HistoryFilter) -> Result<Vec<JournalEntry>> {
        let root = self.board_root(board)?;
        Ok(history::query(&root, filter))
    }

    /// History across several boards (all known boards when `boards` is empty), newest first.
    pub fn history_all(&self, boards: &[String], filter: &HistoryFilter) -> Vec<JournalEntry> {
        let ids: Vec<String> = if boards.is_empty() {
            self.registry
                .lock()
                .boards
                .iter()
                .filter(|b| !b.missing)
                .map(|b| b.id.clone())
                .collect()
        } else {
            boards.to_vec()
        };
        let mut out: Vec<JournalEntry> = ids
            .iter()
            .filter_map(|b| self.board_root(b).ok())
            .flat_map(|root| history::query(&root, filter))
            .collect();
        out.sort_by(|a, b| b.ts.cmp(&a.ts));
        out.truncate(filter.limit.unwrap_or(500));
        out
    }

    pub fn history_blob(&self, board: &str, hash: &str) -> Result<String> {
        history::get_blob(&self.board_root(board)?, hash)
    }

    /// Root folder of a board, open or not.
    pub fn board_root(&self, board: &str) -> Result<PathBuf> {
        if let Ok(b) = self.board(board) {
            return Ok(b.lock().state.root.clone());
        }
        self.registry
            .lock()
            .get(board)
            .map(|e| PathBuf::from(&e.path))
            .ok_or_else(|| Error::not_found(board))
    }

    // --- search indexing --------------------------------------------------------

    pub(crate) fn index_changes(&self, s: &mut BoardStore, ch: &Changes) {
        let st = &s.state;
        let kind = if st.manifest.kind == BoardKind::Files {
            "doc"
        } else {
            "card"
        };
        let mut docs = Vec::new();
        let mut ids: HashSet<String> = ch.nodes.clone().into_iter().collect();
        if ch.reindex_lanes {
            // Lane renames change lane_name of every card in them.
            ids.extend(st.nodes.keys().cloned());
        }
        for id in &ids {
            let Some(n) = st.nodes.get(id) else { continue };
            let body = if n.meta.plain.is_empty() {
                s.read_content(id)
                    .map(|c| markdown::parse(&c).plain)
                    .unwrap_or_default()
            } else {
                n.meta.plain.clone()
            };
            let lane = st.lane_of(id);
            let lane_name = lane
                .as_ref()
                .and_then(|k| st.lane(k))
                .map(|l| l.name.clone());
            docs.push(search::doc_from(
                &st.manifest.id,
                id,
                &n.meta,
                n,
                lane,
                lane_name,
                kind,
                body,
                None,
            ));
        }
        let _ = self.search.upsert(&docs);
        let removed: Vec<String> = ch
            .removed
            .iter()
            .filter(|i| !st.nodes.contains_key(*i))
            .cloned()
            .collect();
        let _ = self.search.remove(&st.manifest.id, &removed);
        if ch.header {
            let _ = self
                .search
                .set_board(&st.manifest.id, &st.manifest.name, kind);
        }
        // Drop the plain text from memory once indexed.
        for id in &ids {
            if let Some(n) = s.state.nodes.get_mut(id) {
                n.meta.plain = String::new();
            }
        }
    }

    pub fn reindex_board(&self, board: &str) {
        let Ok(b) = self.board(board) else { return };
        let mut s = b.lock();
        let st = &s.state;
        let kind = if st.manifest.kind == BoardKind::Files {
            "doc"
        } else {
            "card"
        };
        let _ = self
            .search
            .set_board(&st.manifest.id, &st.manifest.name, kind);
        let existing = self.search.hashes(&st.manifest.id);
        let mut docs = Vec::new();
        for (id, n) in &st.nodes {
            if existing.get(id).is_some_and(|h| h == &n.hash) {
                continue;
            }
            let body = if n.meta.plain.is_empty() {
                s.read_content(id)
                    .map(|c| markdown::parse(&c).plain)
                    .unwrap_or_default()
            } else {
                n.meta.plain.clone()
            };
            let lane = st.lane_of(id);
            let lane_name = lane
                .as_ref()
                .and_then(|k| st.lane(k))
                .map(|l| l.name.clone());
            docs.push(search::doc_from(
                &st.manifest.id,
                id,
                &n.meta,
                n,
                lane,
                lane_name,
                kind,
                body,
                None,
            ));
        }
        let keep: HashSet<String> = st.nodes.keys().cloned().collect();
        let _ = self.search.upsert(&docs);
        let _ = self.search.retain(&st.manifest.id, &keep);
        for n in s.state.nodes.values_mut() {
            n.meta.plain = String::new();
        }
    }

    /// Index a board that is not open (discovery), without keeping it in memory.
    pub fn index_closed_board(&self, path: &Path) {
        let Ok(store) = BoardStore::open(path) else {
            return;
        };
        let st = &store.state;
        let kind = if st.manifest.kind == BoardKind::Files {
            "doc"
        } else {
            "card"
        };
        let _ = self
            .search
            .set_board(&st.manifest.id, &st.manifest.name, kind);
        let existing = self.search.hashes(&st.manifest.id);
        let docs: Vec<_> = search::docs_for_board(st, &|_| None, &|id| store.read_content(id).ok())
            .into_iter()
            .filter(|d| existing.get(&d.id) != Some(&d.hash))
            .collect();
        let _ = self.search.upsert(&docs);
        let keep: HashSet<String> = st.nodes.keys().cloned().collect();
        let _ = self.search.retain(&st.manifest.id, &keep);
    }

    pub fn search(&self, q: &str, opts: &SearchOptions) -> Result<Vec<SearchHit>> {
        self.search.search(q, opts)
    }

    // --- discovery ----------------------------------------------------------------

    pub fn discovery_options(&self) -> crate::discovery::Options {
        let s = self.settings();
        let mut o = crate::discovery::Options::default();
        if let Some(roots) = s.get("discovery.roots").and_then(|v| v.as_array()) {
            let r: Vec<PathBuf> = roots
                .iter()
                .filter_map(|x| x.as_str())
                .map(PathBuf::from)
                .collect();
            if !r.is_empty() {
                o.roots = r;
            }
        }
        if let Some(ex) = s.get("discovery.exclude").and_then(|v| v.as_array()) {
            o.excludes
                .extend(ex.iter().filter_map(|x| x.as_str()).map(str::to_string));
        }
        o
    }

    /// Scan for boards in the background. Updates registry and search index.
    pub fn rescan(self: &Arc<Self>) {
        use std::sync::atomic::Ordering;
        if self.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            let opts = me.discovery_options();
            me.sink.emit(CoreEvent::Progress {
                task: "discovery".into(),
                done: 0,
                total: 0,
                label: None,
            });
            let found = crate::discovery::scan(&opts, &|| false);
            let mut seen = HashSet::new();
            {
                let mut reg = me.registry.lock();
                for f in &found {
                    reg.upsert(&f.id, &f.path, &f.name, f.kind, false);
                    seen.insert(f.id.clone());
                }
                for e in reg.boards.iter_mut() {
                    if !e.mirror
                        && !e.loose
                        && !seen.contains(&e.id)
                        && !Path::new(&e.path).join(MARKER_DIR).exists()
                    {
                        e.missing = true;
                    }
                }
            }
            me.save_registry();
            let total = found.len();
            for (i, f) in found.iter().enumerate() {
                if me.board(&f.id).is_err() {
                    me.index_closed_board(Path::new(&f.path));
                }
                me.sink.emit(CoreEvent::Progress {
                    task: "index".into(),
                    done: i + 1,
                    total,
                    label: Some(f.name.clone()),
                });
            }
            me.scanning.store(false, Ordering::SeqCst);
        });
    }

    /// Give a duplicated board (copy) a fresh id.
    pub fn reassign_board_id(self: &Arc<Self>, path: &Path) -> Result<String> {
        let mp = store::manifest_path(path);
        let text = std::fs::read_to_string(&mp).map_err(|e| Error::io(&mp, e))?;
        let mut m: BoardManifest = serde_json::from_str(&text).map_err(|e| Error::Json {
            path: mp.clone(),
            source: e,
        })?;
        let old = m.id.clone();
        m.id = self.new_board_id();
        let s = crate::json_fmt::to_string(&m).unwrap_or_default();
        atomic_write(&mp, s.as_bytes())?;
        self.update_registry(|r| {
            if let Some(e) = r.get_mut(&old) {
                e.duplicates.retain(|d| Path::new(d) != path);
            }
            r.upsert(&m.id, &path.to_string_lossy(), &m.name, m.kind, false);
        });
        Ok(m.id)
    }

    // --- attachments & cleanup ---------------------------------------------------

    pub fn add_attachment(
        &self,
        board: &str,
        card: &str,
        src: files::Source,
        name: &str,
    ) -> Result<Attachment> {
        let b = self.board(board)?;
        let mut s = b.lock();
        let (att, ch) = files::add_attachment(&mut s, card, src, name)?;
        self.emit_delta(&s, &ch);
        Ok(att)
    }

    /// Remove unreferenced attachments now (`now`) or per the TTL setting.
    pub fn cleanup_unlinked(&self, board: &str, now: bool) -> Result<usize> {
        let ttl = setting_u64(&self.settings(), "files.unlinkedTtlDays", 7);
        let b = self.board(board)?;
        let mut s = b.lock();
        let (n, ch) = files::sweep_unlinked(&mut s, ttl, now)?;
        self.emit_delta(&s, &ch);
        Ok(n)
    }

    pub fn trash_list(&self, board: &str) -> Result<Vec<trash::TrashEntry>> {
        Ok(trash::list(&self.board_root(board)?))
    }

    pub fn trash_purge(&self, board: &str, all: bool) -> Result<usize> {
        let ttl = setting_u64(&self.settings(), "trash.ttlDays", 7) as u32;
        trash::purge(&self.board_root(board)?, ttl, all)
    }

    /// Resolve a file inside a board for the `luau://` protocol.
    pub fn board_file(&self, board: &str, rel: &str) -> Result<PathBuf> {
        if let Some(p) = self.loose_board_file(board, rel) {
            return p;
        }
        let root = self.board_root(board)?;
        files::resolve_board_file(&root, rel)
    }

    pub fn shutdown(&self) {
        self.flush_edits(None);
    }
}

// --- helpers -----------------------------------------------------------------

/// Titles and lane names of the ids an op touches (for readable history).
fn journal_context(st: &BoardState, op: &Op) -> Value {
    let describe = |id: &str| {
        let n = st.nodes.get(id);
        json!({
            "id": id,
            "title": n.map(|n| n.meta.title.clone()),
            "lane": st.lane_of(id).and_then(|k| st.lane(&k).map(|l| l.name.clone())),
            "parent": n.and_then(|n| if let Parent::Card(c) = &n.parent { st.nodes.get(c).map(|p| p.meta.title.clone()) } else { None }),
        })
    };
    let lane_name = |k: &str| st.lane(k).map(|l| l.name.clone());
    match op {
        Op::Move { ids, to, .. } => json!({
            "items": ids.iter().map(|i| describe(i)).collect::<Vec<_>>(),
            "to": match to { Parent::Lane(k) => json!({"lane": lane_name(k)}), Parent::Card(c) => json!({"card": st.nodes.get(c).map(|n| n.meta.title.clone())}), Parent::Root => json!({"root": true}) },
        }),
        Op::Place { items } => {
            json!({ "items": items.iter().map(|i| describe(&i.id)).collect::<Vec<_>>() })
        }
        Op::CreateCard { id, .. } | Op::WriteCard { id, .. } | Op::SetCover { id, .. } => {
            json!({ "items": [describe(id)] })
        }
        Op::Trash { nodes, lanes } => json!({
            "items": nodes.iter().map(|i| describe(i)).collect::<Vec<_>>(),
            "lanes": lanes.iter().map(|k| lane_name(k)).collect::<Vec<_>>(),
        }),
        Op::SetArchived { nodes, lanes } => json!({
            "items": nodes.iter().map(|(i, a)| { let mut d = describe(i); d["archived"] = json!(a); d }).collect::<Vec<_>>(),
            "lanes": lanes.iter().map(|(k, a)| json!({"lane": lane_name(k), "archived": a})).collect::<Vec<_>>(),
        }),
        Op::CreateLane { name, .. } => json!({ "lane": name }),
        Op::UpdateLane { id, patch } => json!({ "lane": lane_name(id), "patch": patch }),
        Op::MoveLane { id, index } => json!({ "lane": lane_name(id), "index": index }),
        Op::UpdateBoard { patch } => json!({ "patch": patch }),
        Op::SetKind { kind } => json!({ "kind": kind }),
        Op::Batch { ops } => json!({ "ops": ops.iter().map(|o| o.kind()).collect::<Vec<_>>() }),
        Op::Restore { .. } | Op::External { .. } => Value::Null,
    }
}

#[cfg(test)]
mod tests;
