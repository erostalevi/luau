//! RPC methods for the History panel (trash maintenance).
//! Timeline, blobs, list/purge and restore already live in `rpc.rs` / `board.apply`.

use std::sync::Arc;

use lull_core::app::Core;
use serde_json::{Value, json};
use tauri::AppHandle;

use crate::rpc::{R, arg};

pub fn dispatch_sync(
    _app: &AppHandle,
    core: &Arc<Core>,
    _window: &str,
    method: &str,
    p: &Value,
) -> Option<R> {
    Some(match method {
        // Permanently delete trash entries: `{ board, ids: string[] }` → count removed.
        "trash.delete" => delete(core, p),
        _ => return None,
    })
}

fn delete(core: &Arc<Core>, p: &Value) -> R {
    let board: String = arg(p, "board")?;
    let ids: Vec<String> = arg(p, "ids")?;
    Ok(json!(core.trash_delete(&board, &ids)?))
}
