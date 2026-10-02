//! RPC methods for local AI, activity summaries, schedules, code runner and
//! link previews. Business logic lives in `luau_core::ai`; this is glue only.

use std::sync::Arc;

use luau_core::ai::schedule::Schedule;
use luau_core::ai::service::{
    CardSummaryRequest, CardsFromTextRequest, FactsQuery, Notifier, SummarizeRequest,
};
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

/// Register the bundled `apple-llm` sidecar (Apple on-device model). Tauri
/// puts `externalBin` sidecars next to the app executable (`Contents/MacOS`
/// in the bundle, `target/<profile>` in development).
pub fn register_apple_helper() {
    if !cfg!(target_os = "macos") {
        return;
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        let p = dir.join("apple-llm");
        if p.is_file() {
            luau_core::ai::apple::set_helper_path(p);
        } else {
            tracing::info!("apple-llm helper not bundled; Apple on-device AI disabled");
        }
    }
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
        "ai.test" => core.ai_test().await.map_err(Into::into).and_then(ok),
        "ai.transform" => match de::<luau_core::ai::assist::TransformRequest>(p) {
            Ok(req) => core
                .ai_transform(req)
                .await
                .map_err(Into::into)
                .and_then(ok),
            Err(e) => Err(e),
        },
        "ai.setupLocal" => {
            let req = match de::<luau_core::ai::setup::SetupRequest>(p) {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            let text: luau_core::ai::service::SetupTexts = p
                .get("texts")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();
            let n = notifier(app);
            core.ai_setup_local(req, Some(&n), &text)
                .await
                .map_err(Into::into)
                .and_then(ok)
        }
        "ai.remoteModels" => match arg::<String>(p, "provider") {
            Ok(pr) => core
                .ai_remote_models(&pr)
                .await
                .map_err(Into::into)
                .and_then(ok),
            Err(e) => Err(e),
        },
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
        "ai.cardsFromText" => match de::<CardsFromTextRequest>(p) {
            Ok(req) => core
                .ai_cards_from_text(req)
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
            // Strict params: a malformed board must not fall back to "any board".
            let (board, card) = match (opt::<String>(p, "board"), opt::<String>(p, "card")) {
                (Ok(b), Ok(c)) => (b, c),
                (Err(e), _) | (_, Err(e)) => return Some(Err(e)),
            };
            // Trust is per board, so the code must really come from that
            // board: it has to appear in the named card.
            if let Some(b) = board.as_deref() {
                let Some(card) = card.as_deref() else {
                    return Some(Err(crate::rpc::RpcError {
                        code: "invalid".into(),
                        message: "card required".into(),
                    }));
                };
                match core.read_card(b, card) {
                    Ok(text) if text.replace("\r\n", "\n").contains(code.trim_end()) => {}
                    Ok(_) => {
                        return Some(Err(crate::rpc::RpcError {
                            code: "invalid".into(),
                            message: "code_not_in_card".into(),
                        }));
                    }
                    Err(e) => return Some(Err(e.into())),
                }
            }
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
    app: &AppHandle,
    core: &Arc<Core>,
    window: &str,
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
                let board: String = arg(p, "board")?;
                let trusted: bool = opt(p, "trusted")?.unwrap_or(true);
                if trusted {
                    // Granting trust needs a native confirmation the page cannot
                    // fake or click for the user; it names the board.
                    let name = core
                        .snapshot(&board)
                        .map(|s| s.header.name)
                        .map_err(crate::rpc::RpcError::from)?;
                    let title: String =
                        opt(p, "title")?.unwrap_or_else(|| "Run code from this board?".into());
                    let message: String = opt(p, "message")?.unwrap_or_default();
                    let confirm: String =
                        opt(p, "confirm")?.unwrap_or_else(|| "Trust and run".into());
                    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
                    let mut d = app
                        .dialog()
                        .message(format!("“{name}”\n\n{message}"))
                        .title(title)
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            confirm,
                            "Cancel".into(),
                        ));
                    if let Some(w) = tauri::Manager::get_webview_window(app, window) {
                        d = d.parent(&w);
                    }
                    if !d.blocking_show() {
                        return Err(crate::rpc::RpcError {
                            code: "cancelled".into(),
                            message: String::new(),
                        });
                    }
                }
                ok(core.code_set_trust(Some(&board), trusted)?)
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
            "ai.remoteState" => ok(core.ai_remote_state()),
            "ai.setupCancel" => {
                luau_core::ai::setup::cancel();
                ok(())
            }
            "ai.setKey" => {
                let persisted =
                    core.ai_set_key(&arg::<String>(p, "provider")?, &arg::<String>(p, "key")?)?;
                ok(json!({ "persisted": persisted }))
            }
            "ai.deleteKey" => ok(core.ai_delete_key(&arg::<String>(p, "provider")?)?),
            "ai.consent" => {
                let provider: String = arg(p, "provider")?;
                let granted: bool = opt(p, "granted")?.unwrap_or(true);
                if granted {
                    // Consent comes from a native prompt the page cannot fake
                    // or click; it names the service the text will go to.
                    let service: String = opt(p, "service")?.unwrap_or_else(|| provider.clone());
                    let title: String =
                        opt(p, "title")?.unwrap_or_else(|| "Send card text to a remote AI?".into());
                    let message: String = opt(p, "message")?.unwrap_or_default();
                    let confirm: String = opt(p, "confirm")?.unwrap_or_else(|| "Allow".into());
                    let cancel: String = opt(p, "cancel")?.unwrap_or_else(|| "Cancel".into());
                    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
                    let mut d = app
                        .dialog()
                        .message(format!("{service}\n\n{message}"))
                        .title(title)
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancelCustom(confirm, cancel));
                    if let Some(w) = tauri::Manager::get_webview_window(app, window) {
                        d = d.parent(&w);
                    }
                    if !d.blocking_show() {
                        return Err(crate::rpc::RpcError {
                            code: "cancelled".into(),
                            message: String::new(),
                        });
                    }
                }
                ok(core.ai_set_consent(&provider, granted)?)
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
