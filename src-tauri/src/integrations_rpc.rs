//! RPC methods for integrations, AI, code runner and schedules.
//! Filled in by the integrations module.

use std::sync::Arc;

use lull_core::app::Core;
use serde_json::Value;
use tauri::AppHandle;

use crate::rpc::R;

pub async fn dispatch_async(
    _app: &AppHandle,
    _core: &Arc<Core>,
    _window: &str,
    _method: &str,
    _p: &Value,
) -> Option<R> {
    None
}

pub fn dispatch_sync(
    _app: &AppHandle,
    _core: &Arc<Core>,
    _window: &str,
    _method: &str,
    _p: &Value,
) -> Option<R> {
    None
}
