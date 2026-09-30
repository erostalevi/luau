//! File watching: debounce external changes per board, ignore our own writes,
//! reload, journal and push deltas.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use serde_json::json;

use super::{Core, CoreEvent};
use crate::brand::{BOARD_FILE, MARKER_DIR};
use crate::history::{self, JournalEntry, Origin};

pub enum WatchMsg {
    Event { board: String, paths: Vec<PathBuf> },
}

const QUIET: Duration = Duration::from_millis(220);

fn relevant(root: &Path, p: &Path) -> bool {
    let rel = p.strip_prefix(root).unwrap_or(p);
    let mut comps = rel.components();
    if let Some(first) = comps.next()
        && first.as_os_str() == MARKER_DIR
    {
        // Only the manifest matters inside the marker dir.
        return comps.next().is_some_and(|c| c.as_os_str() == BOARD_FILE) && comps.next().is_none();
    }
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    !(name.starts_with('.') && name.contains(".tmp-")
        || name == ".DS_Store"
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swx"))
}

pub fn start(core: &Arc<Core>) {
    let (tx, rx) = channel::<WatchMsg>();
    *core.watch_tx.lock() = Some(tx);
    let weak = Arc::downgrade(core);
    std::thread::Builder::new()
        .name("lull-watch".into())
        .spawn(move || run(weak, rx))
        .ok();
}

fn run(core: Weak<Core>, rx: Receiver<WatchMsg>) {
    let mut pending: HashMap<String, (Vec<PathBuf>, Instant)> = HashMap::new();
    let mut last_flush = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(120)) {
            Ok(WatchMsg::Event { board, paths }) => {
                let e = pending
                    .entry(board)
                    .or_insert_with(|| (Vec::new(), Instant::now()));
                e.0.extend(paths);
                e.1 = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(core) = core.upgrade() else { return };
        let ready: Vec<String> = pending
            .iter()
            .filter(|(_, (_, t))| t.elapsed() >= QUIET)
            .map(|(b, _)| b.clone())
            .collect();
        for b in ready {
            if let Some((paths, _)) = pending.remove(&b) {
                process(&core, &b, &paths);
            }
        }
        if last_flush.elapsed() > Duration::from_secs(5) {
            core.flush_idle(60);
            last_flush = Instant::now();
        }
    }
}

/// Handle a batch of external paths for a board.
pub fn process(core: &Core, board: &str, paths: &[PathBuf]) {
    let Ok(b) = core.board(board) else { return };
    let mut s = b.lock();
    let root = s.state.root.clone();
    let external: Vec<&PathBuf> = paths
        .iter()
        .filter(|p| relevant(&root, p) && !s.recently_touched(p))
        .collect();
    if external.is_empty() {
        return;
    }
    let ch = match s.reload() {
        Ok(ch) => ch,
        Err(e) => {
            tracing::warn!("reload {board}: {e}");
            return;
        }
    };
    let ids: Vec<String> = ch.nodes.iter().chain(ch.removed.iter()).cloned().collect();
    if !ids.is_empty() || ch.lanes || ch.header {
        let titles: Vec<String> = ch
            .nodes
            .iter()
            .filter_map(|i| s.state.nodes.get(i).map(|n| n.meta.title.clone()))
            .collect();
        let _ = history::append(
            &root,
            &JournalEntry {
                ts: history::now(),
                board: board.to_string(),
                kind: "externalEdit".into(),
                origin: Origin::External,
                label: "Changed outside the app".into(),
                ids: ids.clone(),
                details: json!({ "titles": titles, "removed": ch.removed.len() }),
                before: None,
                after: None,
            },
        );
    }
    core.index_changes(&mut s, &ch);
    core.emit_delta(&s, &ch);
    core.sink.emit(CoreEvent::ExternalChange {
        board_id: board.to_string(),
        ids,
    });
}

pub fn watch_board(core: &Core, id: &str, root: &Path) {
    let Some(tx) = core.watch_tx.lock().clone() else {
        return;
    };
    let bid = id.to_string();
    let r = root.to_path_buf();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if matches!(ev.kind, notify::EventKind::Access(_)) {
                return;
            }
            let paths: Vec<PathBuf> = ev.paths.into_iter().filter(|p| relevant(&r, p)).collect();
            if !paths.is_empty() {
                let _ = tx.send(WatchMsg::Event {
                    board: bid.clone(),
                    paths,
                });
            }
        }
    });
    match watcher {
        Ok(mut w) => {
            if let Err(e) = w.watch(root, RecursiveMode::Recursive) {
                tracing::warn!("watch {}: {e}", root.display());
                return;
            }
            core.watchers.lock().insert(id.to_string(), w);
        }
        Err(e) => tracing::warn!("watcher: {e}"),
    }
}
