//! Single RPC entry point: `rpc(method, params)`. Each method maps to a Core
//! call. Blocking work runs on the blocking pool so the UI thread never waits.

use std::path::PathBuf;
use std::sync::Arc;

use luau_core::app::{Core, files};
use luau_core::history::HistoryFilter;
use luau_core::model::{BoardKind, Parent};
use luau_core::search::SearchOptions;
use luau_core::store::Op;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

impl From<luau_core::Error> for RpcError {
    fn from(e: luau_core::Error) -> Self {
        RpcError {
            code: e.code().into(),
            message: e.to_string(),
        }
    }
}

fn bad(msg: impl Into<String>) -> RpcError {
    RpcError {
        code: "invalid".into(),
        message: msg.into(),
    }
}

pub type R = Result<Value, RpcError>;

pub fn arg<T: DeserializeOwned>(p: &Value, key: &str) -> Result<T, RpcError> {
    let v = p.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| bad(format!("param `{key}`: {e}")))
}

pub fn opt<T: DeserializeOwned>(p: &Value, key: &str) -> Result<Option<T>, RpcError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| bad(format!("param `{key}`: {e}"))),
    }
}

fn ok<T: Serialize>(v: T) -> R {
    serde_json::to_value(v).map_err(|e| bad(e.to_string()))
}

#[tauri::command]
pub async fn rpc(app: AppHandle, window: WebviewWindow, method: String, params: Value) -> R {
    let core = app.state::<AppState>().core.clone();
    let label = window.label().to_string();
    // Async-native methods first (network / long-running), per feature module.
    if let Some(res) =
        crate::integrations_rpc::dispatch_async(&app, &core, &label, &method, &params).await
    {
        return res;
    }
    if let Some(res) = crate::ai_rpc::dispatch_async(&app, &core, &label, &method, &params).await {
        return res;
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || dispatch(&app2, &core, &label, &method, &params))
        .await
        .map_err(|e| bad(format!("task failed: {e}")))?
}

fn dispatch(app: &AppHandle, core: &Arc<Core>, window: &str, method: &str, p: &Value) -> R {
    match method {
        // --- app ---------------------------------------------------------------
        "app.info" => ok(json!({
            "name": "Luau",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "dataDir": core.paths.data,
            "configDir": core.paths.config,
            "logsDir": core.paths.logs,
            "window": window,
            "home": dirs::home_dir(),
        })),
        "settings.get" => ok(core.settings()),
        "settings.set" => {
            core.set_settings(arg(p, "value")?)?;
            ok(true)
        }
        "keybindings.get" => ok(core.read_json_file(&core.paths.keybindings())),
        "keybindings.set" => {
            core.write_json_file(&core.paths.keybindings(), &arg::<Value>(p, "value")?)?;
            ok(true)
        }
        "uiState.get" => ok(core.read_json_file(&core.paths.ui_state())),
        "uiState.set" => {
            core.write_json_file(&core.paths.ui_state(), &arg::<Value>(p, "value")?)?;
            ok(true)
        }
        "fonts.list" => ok(crate::fonts::list()),
        "window.new" => {
            let label = crate::windows::next_label();
            let query: String = opt(p, "query")?.unwrap_or_default();
            // Only `?key=value&…` with URL-safe characters (percent-encoded JSON).
            if !(query.is_empty()
                || (query.starts_with('?')
                    && query.len() <= 4096
                    && query[1..]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "=&%_.-~".contains(c))))
            {
                return Err(bad("invalid window query"));
            }
            crate::windows::create(app, &label, &query).map_err(|e| bad(e.to_string()))?;
            ok(label)
        }
        "window.focus" => {
            let label: String = arg(p, "label")?;
            if let Some(w) = app.get_webview_window(&label) {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            ok(true)
        }
        "window.release" => {
            core.release_window(window);
            ok(true)
        }
        "file.open" => {
            // Open a board file with the default app; the path is resolved
            // inside the board by the core (no traversal, no links out).
            let board: String = arg(p, "board")?;
            let rel: String = arg(p, "rel")?;
            let path = core.board_file(&board, &rel)?;
            if !path.is_file() {
                return Err(bad("not found"));
            }
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_path(path.to_string_lossy(), None::<&str>)
                .map_err(|e| bad(e.to_string()))?;
            ok(true)
        }
        "config.open" => {
            let file: String = arg(p, "file")?;
            let path = match file.as_str() {
                "settings" => core.paths.settings(),
                "keybindings" => core.paths.keybindings(),
                _ => return Err(bad("unknown config file")),
            };
            if !path.exists() {
                luau_core::fsutil::atomic_write(&path, b"{}\n")?;
            }
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_path(path.to_string_lossy(), None::<&str>)
                .map_err(|e| bad(e.to_string()))?;
            ok(true)
        }
        "path.exists" => {
            let path: PathBuf = arg(p, "path")?;
            ok(
                json!({ "exists": path.exists(), "isDir": path.is_dir(), "isBoard": luau_core::store::is_board(&path) }),
            )
        }
        "logs.export" => {
            let dest: PathBuf = arg(p, "dest")?;
            crate::grants::require(core, &dest, false)?;
            crate::exports::zip_dir(&core.paths.logs, &dest).map_err(|e| bad(e.to_string()))?;
            ok(true)
        }

        // --- registry & discovery ------------------------------------------------
        "registry.get" => ok(core.registry()),
        "registry.update" => {
            let id: Option<String> = opt(p, "id")?;
            let pinned: Option<bool> = opt(p, "pinned")?;
            let hidden: Option<bool> = opt(p, "hidden")?;
            let section: Option<String> = opt(p, "section")?;
            let order: Option<Vec<String>> = opt(p, "order")?;
            let mirror_order: Option<Vec<String>> = opt(p, "mirrorOrder")?;
            let remove: bool = opt(p, "remove")?.unwrap_or(false);
            core.update_registry(|r| {
                if let Some(id) = &id {
                    if remove {
                        r.boards.retain(|b| &b.id != id);
                    } else if let Some(e) = r.get_mut(id) {
                        if let Some(v) = pinned {
                            e.pinned = v;
                        }
                        if let Some(v) = hidden {
                            e.hidden = v;
                        }
                        if let Some(s) = &section {
                            e.section = if s.is_empty() { None } else { Some(s.clone()) };
                        }
                    }
                }
                if let Some(o) = order {
                    r.order = o;
                }
                if let Some(o) = mirror_order {
                    r.mirror_order = o;
                }
            });
            ok(core.registry())
        }
        "discovery.rescan" => {
            core.rescan();
            ok(true)
        }
        "discovery.defaults" => ok(json!({
            "roots": luau_core::discovery::default_roots(),
            "excludes": luau_core::discovery::DEFAULT_EXCLUDES,
        })),
        "board.reassignId" => {
            let path: PathBuf = arg(p, "path")?;
            // Duplicated boards come from discovery (not a dialog): only an
            // existing board folder is accepted, and only its id changes.
            if !luau_core::store::is_board(&path) {
                crate::grants::require(core, &path, true)?;
            }
            ok(core.reassign_board_id(&path)?)
        }

        // --- boards ----------------------------------------------------------------
        "board.create" => {
            let path: PathBuf = arg(p, "path")?;
            crate::grants::require(core, &path, false)?;
            let name: String = opt(p, "name")?.unwrap_or_default();
            let kind: BoardKind = opt(p, "kind")?.unwrap_or_default();
            let lanes: Vec<String> = opt(p, "lanes")?.unwrap_or_default();
            let git: bool = opt(p, "git")?.unwrap_or(true);
            ok(core.create_board(&path, &name, kind, &lanes, git)?)
        }
        "board.open" => {
            if let Some(id) = opt::<String>(p, "id")? {
                ok(core.open_board_by_id(&id)?)
            } else {
                let path: PathBuf = arg(p, "path")?;
                crate::grants::require(core, &path, true)?;
                ok(core.open_board(&path)?)
            }
        }
        "board.close" => {
            core.close_board(&arg::<String>(p, "id")?);
            ok(true)
        }
        "board.snapshot" => ok(core.snapshot(&arg::<String>(p, "id")?)?),
        "board.claim" => ok(core.claim(&arg::<String>(p, "id")?, window)),
        "board.release" => {
            core.release(&arg::<String>(p, "id")?, window);
            ok(true)
        }
        "board.apply" => {
            let board: String = arg(p, "board")?;
            let op: Op = arg(p, "op")?;
            let label: String = opt(p, "label")?.unwrap_or_else(|| op.kind().to_string());
            let coalesce: Option<String> = opt(p, "coalesce")?;
            ok(core.apply(&board, op, &label, coalesce)?)
        }
        "board.undo" => ok(core.undo(&arg::<String>(p, "board")?)?),
        "board.redo" => ok(core.redo(&arg::<String>(p, "board")?)?),
        "board.seal" => {
            let board: String = arg(p, "board")?;
            let card: Option<String> = opt(p, "card")?;
            core.seal(&board, card.as_deref());
            ok(true)
        }
        "board.moveAcross" => {
            let from: String = arg(p, "from")?;
            let to: String = arg(p, "to")?;
            let ids: Vec<String> = arg(p, "ids")?;
            let parent: Parent = arg(p, "parent")?;
            let before: Option<String> = opt(p, "before")?;
            core.move_across(&from, &ids, &to, parent, before)?;
            ok(true)
        }
        "board.newCardId" => ok(core.new_card_id()),
        "board.newLaneId" => ok(core.new_lane_id(&arg::<String>(p, "board")?)),
        "card.read" => ok(core.read_card(&arg::<String>(p, "board")?, &arg::<String>(p, "id")?)?),
        "card.write" => {
            let board: String = arg(p, "board")?;
            let id: String = arg(p, "id")?;
            let content: String = arg(p, "content")?;
            let session: Option<String> = opt(p, "session")?;
            let base: Option<String> = opt(p, "base")?;
            ok(core.write_card_checked(&board, &id, &content, session, base.as_deref())?)
        }
        "card.path" => {
            let board: String = arg(p, "board")?;
            let id: String = arg(p, "id")?;
            let b = core.board(&board)?;
            let s = b.lock();
            ok(json!({ "file": s.state.node_file(&id), "dir": s.state.attachment_dir(&id) }))
        }
        "attachment.add" => {
            use base64::Engine;
            let board: String = arg(p, "board")?;
            let card: String = arg(p, "card")?;
            let name: String = arg(p, "name")?;
            if let Some(path) = opt::<PathBuf>(p, "path")? {
                crate::grants::require(core, &path, false)?;
                ok(core.add_attachment(&board, &card, files::Source::Path(&path), &name)?)
            } else {
                let b64: String = arg(p, "base64")?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|e| bad(e.to_string()))?;
                ok(core.add_attachment(&board, &card, files::Source::Bytes(&bytes), &name)?)
            }
        }
        "attachment.cleanup" => {
            let board: String = arg(p, "board")?;
            let now: bool = opt(p, "now")?.unwrap_or(true);
            ok(core.cleanup_unlinked(&board, now)?)
        }
        "trash.list" => ok(core.trash_list(&arg::<String>(p, "board")?)?),
        "trash.purge" => {
            ok(core.trash_purge(&arg::<String>(p, "board")?, opt(p, "all")?.unwrap_or(false))?)
        }

        // --- history & search ------------------------------------------------------
        "history.query" => {
            let board: String = arg(p, "board")?;
            let filter: HistoryFilter = opt(p, "filter")?.unwrap_or_default();
            ok(core.history(&board, &filter)?)
        }
        "history.blob" => {
            ok(core.history_blob(&arg::<String>(p, "board")?, &arg::<String>(p, "hash")?)?)
        }
        "search.query" => {
            let q: String = arg(p, "q")?;
            let opts: SearchOptions = opt(p, "opts")?.unwrap_or_default();
            ok(core.search(&q, &opts)?)
        }
        "search.tags" => ok(core
            .search
            .tags(&opt::<Vec<String>>(p, "boards")?.unwrap_or_default())),
        "search.people" => ok(core.search.people()),
        "search.titles" => ok(core
            .search
            .titles(&arg::<Vec<String>>(p, "ids")?)
            .into_iter()
            .map(|(id, board, title)| json!({ "id": id, "board": board, "title": title }))
            .collect::<Vec<_>>()),
        "search.backlinks" => ok(core
            .search
            .backlinks(&arg::<String>(p, "id")?)
            .into_iter()
            .map(|(board, id, title)| json!({ "id": id, "board": board, "title": title }))
            .collect::<Vec<_>>()),
        "search.locate" => ok(core
            .search
            .locate(&arg::<String>(p, "id")?)
            .map(|(b, t)| json!({ "board": b, "title": t }))),

        // --- import / export ---------------------------------------------------------
        "history.queryAll" => {
            let filter: HistoryFilter = opt(p, "filter")?.unwrap_or_default();
            let boards: Vec<String> = opt(p, "boards")?.unwrap_or_default();
            ok(core.history_all(&boards, &filter))
        }
        m => crate::dialogs::dispatch_sync(app, window, m, p)
            .or_else(|| crate::exports::dispatch(app, core, window, m, p))
            .or_else(|| crate::integrations_rpc::dispatch_sync(app, core, window, m, p))
            .or_else(|| crate::ai_rpc::dispatch_sync(app, core, window, m, p))
            .or_else(|| crate::history_rpc::dispatch_sync(app, core, window, m, p))
            .unwrap_or_else(|| Err(bad(format!("unknown method {m}")))),
    }
}
