//! RPC methods for local AI, activity summaries, schedules, code runner and
//! link previews. Business logic lives in `luau_core::ai`; this is glue only.

use std::sync::Arc;

use luau_core::ai::schedule::Schedule;
use luau_core::ai::service::{CardSummaryRequest, FactsQuery, Notifier, SummarizeRequest};
use luau_core::app::Core;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::AppHandle;

use crate::rpc::{R, RpcError, arg, opt};

fn ok<T: Serialize>(v: T) -> R {
    serde_json::to_value(v).map_err(|e| RpcError {
        code: "invalid".into(),
        message: e.to_string(),
    })
}

fn de<T: serde::de::DeserializeOwned>(p: &Value) -> Result<T, RpcError> {
    serde_json::from_value(p.clone()).map_err(|e| RpcError {
        code: "invalid".into(),
        message: format!("params: {e}"),
    })
}

fn notifier(app: &AppHandle) -> Notifier {
    let app = app.clone();
    Arc::new(move |title: &str, body: &str| {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| luau_core::Error::Other(format!("notification: {e}")))
    })
}

/// Start the background scheduler for scheduled summaries (called once at setup).
pub fn start_scheduler(app: &AppHandle, core: Arc<Core>) {
    luau_core::ai::scheduler::start(core, Some(notifier(app)));
}

pub async fn dispatch_async(
    app: &AppHandle,
    core: &Arc<Core>,
    _window: &str,
    method: &str,
    p: &Value,
) -> Option<R> {
    let res = match method {
        "ai.status" => ok(core.ai_status().await),
        "ai.models" => core.ai_models().await.map_err(Into::into).and_then(ok),
        "ai.pullModel" => {
            let name: String = match arg(p, "name") {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            core.ai_pull_model(&name)
                .await
                .map_err(Into::into)
                .and_then(|_| ok(()))
        }
        "activity.summarize" => match de::<SummarizeRequest>(p) {
            Ok(req) => core.summarize(req).await.map_err(Into::into).and_then(ok),
            Err(e) => Err(e),
        },
        "card.summarize" => match de::<CardSummaryRequest>(p) {
            Ok(req) => core
                .card_summarize(req)
                .await
                .map_err(Into::into)
                .and_then(ok),
            Err(e) => Err(e),
        },
        "schedules.runNow" => match arg::<String>(p, "id") {
            Ok(id) => {
                let n = notifier(app);
                core.run_schedule(&id, Some(&n))
                    .await
                    .map_err(Into::into)
                    .and_then(ok)
            }
            Err(e) => Err(e),
        },
        "code.run" => {
            let (lang, code) = match (arg::<String>(p, "lang"), arg::<String>(p, "code")) {
                (Ok(l), Ok(c)) => (l, c),
                (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
            };
            let board: Option<String> = opt(p, "board").ok().flatten();
            core.code_run(&lang, &code, board.as_deref())
                .await
                .map_err(Into::into)
                .and_then(ok)
        }
        "web.preview" => match arg::<String>(p, "url") {
            Ok(url) => core
                .web_preview(&url)
                .await
                .map_err(Into::into)
                .and_then(ok),
            Err(e) => Err(e),
        },
        _ => return None,
    };
    Some(res)
}

pub fn dispatch_sync(
    _app: &AppHandle,
    core: &Arc<Core>,
    _window: &str,
    method: &str,
    p: &Value,
) -> Option<R> {
    let res = (|| -> R {
        match method {
            "activity.facts" => ok(core.activity_facts(&de::<FactsQuery>(p)?)?),
            "schedules.list" => ok(core.schedules_list()),
            "schedules.save" => ok(core.schedules_save(arg::<Schedule>(p, "schedule")?)?),
            "schedules.delete" => ok(core.schedules_delete(&arg::<String>(p, "id")?)?),
            "summaries.list" => ok(core.summaries_list(opt::<usize>(p, "limit")?.unwrap_or(50))),
            "summaries.get" => ok(core.summaries_get(&arg::<String>(p, "id")?)?),
            "summaries.delete" => ok(core.summaries_delete(&arg::<String>(p, "id")?)?),
            "code.cached" => {
                ok(core.code_cached(&arg::<String>(p, "lang")?, &arg::<String>(p, "code")?))
            }
            "code.trusted" => {
                let board: Option<String> = opt(p, "board")?;
                ok(json!({ "trusted": core.code_is_trusted(board.as_deref()) }))
            }
            "code.trust" => {
                let board: Option<String> = opt(p, "board")?;
                let trusted: bool = opt(p, "trusted")?.unwrap_or(true);
                ok(core.code_set_trust(board.as_deref(), trusted)?)
            }
            "summaries.export" => {
                let path = std::path::PathBuf::from(arg::<String>(p, "path")?);
                crate::grants::require(core, &path, false)?;
                let markdown: String = arg(p, "markdown")?;
                let ext_ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt")
                });
                if !path.is_absolute() || !ext_ok || markdown.len() > 4 * 1024 * 1024 {
                    return Err(RpcError {
                        code: "invalid".into(),
                        message: "export needs an absolute .md path".into(),
                    });
                }
                ok(luau_core::fsutil::atomic_write(&path, markdown.as_bytes())?)
            }
            "ai.slackConnected" => ok(luau_core::ai::slack_connected()
                && luau_core::integrations::accounts::load(&core.paths.config)
                    .iter()
                    .any(|a| a.provider == luau_core::integrations::types::ProviderKind::Slack)),
            _ => Err(RpcError {
                code: "unknown".into(),
                message: String::new(),
            }),
        }
    })();
    match res {
        Err(e) if e.code == "unknown" && e.message.is_empty() => None,
        r => Some(r),
    }
}
