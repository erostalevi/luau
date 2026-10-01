//! RPC methods for integrations (Jira, Trello, Slack, mirrors, WriteGate).
//! Thin adapter over `luau_core::integrations::service`.

use std::sync::Arc;

use luau_core::app::Core;
use luau_core::integrations::{accounts, gate, provider, service as svc};
use luau_core::model::Parent;
use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use crate::rpc::{R, RpcError, arg, opt};

fn ok<T: Serialize>(v: T) -> R {
    serde_json::to_value(v).map_err(|e| RpcError {
        code: "invalid".into(),
        message: e.to_string(),
    })
}

fn obj(p: &Value) -> Value {
    p.clone()
}

pub async fn dispatch_async(
    _app: &AppHandle,
    core: &Arc<Core>,
    _window: &str,
    method: &str,
    p: &Value,
) -> Option<R> {
    if !(method.starts_with("integrations.")
        || method.starts_with("remote.")
        || method.starts_with("slack."))
    {
        return None;
    }
    Some(run(core, method, p).await)
}

async fn run(core: &Arc<Core>, method: &str, p: &Value) -> R {
    let account = || arg::<String>(p, "account");
    match method {
        // --- accounts ---------------------------------------------------------
        "integrations.accounts" => ok(accounts::load(&core.paths.config)),
        "integrations.connect" => {
            let req: svc::ConnectReq = serde_json::from_value(obj(p)).map_err(|e| RpcError {
                code: "invalid".into(),
                message: e.to_string(),
            })?;
            ok(svc::connect(core, req).await?)
        }
        "integrations.test" => ok(svc::test(core, &account()?).await?),
        "integrations.remove" => {
            svc::remove_account(core, &account()?)?;
            ok(true)
        }
        "integrations.update" => {
            let patch: svc::AccountPatch = arg(p, "patch")?;
            ok(svc::update_account(core, &account()?, patch)?)
        }
        "integrations.capabilities" => {
            let (_, pr) = svc::open(core, &account()?)?;
            ok(pr.capabilities())
        }
        "integrations.mirrors" => ok(svc::mirrors(core)),
        "integrations.tick" => {
            svc::tick(core).await;
            ok(true)
        }

        // --- remote reads -------------------------------------------------------
        "remote.search" => {
            let (a, pr) = svc::open(core, &account()?)?;
            let q: Option<String> = opt(p, "query")?;
            let q = q
                .filter(|q| !q.trim().is_empty())
                .unwrap_or_else(|| a.query());
            if q.len() > 4000 {
                return Err(RpcError {
                    code: "invalid".into(),
                    message: "query too long".into(),
                });
            }
            let next: Option<String> = opt(p, "next")?;
            ok(pr.search(&q, next.as_deref()).await?)
        }
        "remote.issue" => ok(svc::open(core, &account()?)?
            .1
            .issue(&arg::<String>(p, "key")?)
            .await?),
        "remote.transitions" => ok(svc::open(core, &account()?)?
            .1
            .transitions(&arg::<String>(p, "key")?)
            .await?),
        "remote.comments" => ok(svc::open(core, &account()?)?
            .1
            .comments(&arg::<String>(p, "key")?)
            .await?),
        "remote.users" => ok(svc::open(core, &account()?)?
            .1
            .users(&arg::<String>(p, "q")?)
            .await?),
        "remote.projects" => ok(svc::open(core, &account()?)?.1.projects().await?),
        "remote.issueTypes" => ok(svc::open(core, &account()?)?
            .1
            .issue_types(&arg::<String>(p, "project")?)
            .await?),
        "remote.boards" => ok(svc::open(core, &account()?)?.1.boards().await?),

        // --- links & copies -----------------------------------------------------------
        "remote.links" => ok(svc::links_dto(core, &arg::<String>(p, "board")?)?),
        "remote.link" => {
            let req: svc::LinkReq = serde_json::from_value(obj(p)).map_err(|e| RpcError {
                code: "invalid".into(),
                message: e.to_string(),
            })?;
            ok(svc::link_issue(core, req).await?)
        }
        "remote.copyFromMirror" => {
            let from: String = arg(p, "from")?;
            let ids: Vec<String> = arg(p, "ids")?;
            let to: String = arg(p, "to")?;
            let parent: Parent = arg(p, "parent")?;
            let before: Option<String> = opt(p, "before")?;
            ok(svc::copy_from_mirror(core, &from, &ids, &to, parent, before).await?)
        }
        "remote.refresh" => ok(svc::refresh_links(core, &arg::<String>(p, "board")?).await?),
        "remote.unlink" => {
            svc::unlink(
                core,
                &arg::<String>(p, "board")?,
                &arg::<String>(p, "card")?,
            )?;
            ok(true)
        }
        "remote.notes.get" => ok(svc::notes_get(
            core,
            &arg::<String>(p, "board")?,
            &arg::<String>(p, "card")?,
        )?),
        "remote.notes.set" => {
            svc::notes_set(
                core,
                &arg::<String>(p, "board")?,
                &arg::<String>(p, "card")?,
                &arg::<String>(p, "text")?,
            )?;
            ok(true)
        }

        // --- mirrors ----------------------------------------------------------------------
        "remote.mirror.create" => {
            let req: svc::MirrorReq = serde_json::from_value(obj(p)).map_err(|e| RpcError {
                code: "invalid".into(),
                message: e.to_string(),
            })?;
            ok(svc::create_mirror(core, req).await?)
        }
        "remote.mirror.sync" => {
            svc::sync_mirror(core, &arg::<String>(p, "board")?).await?;
            ok(true)
        }
        "remote.mirror.watch" => {
            svc::set_watch(core, &arg::<String>(p, "board")?, arg(p, "watch")?)?;
            ok(true)
        }
        "remote.mirror.remove" => {
            svc::remove_mirror(core, &arg::<String>(p, "board")?)?;
            ok(true)
        }

        // --- WriteGate ------------------------------------------------------------------------
        "remote.prepare" => {
            let req: svc::PrepareReq = arg(p, "request")?;
            ok(svc::prepare(core, req).await?)
        }
        "remote.commit" => ok(svc::commit(core, &arg::<String>(p, "token")?).await?),
        "remote.cancel" => {
            gate::cancel(&arg::<String>(p, "token")?);
            ok(true)
        }

        // --- Slack ------------------------------------------------------------------------------
        "slack.channels" => {
            let (_, s) = provider::slack(&core.paths.config, &account()?)?;
            ok(s.channels().await?)
        }
        m => Err(RpcError {
            code: "invalid".into(),
            message: format!("unknown method {m}"),
        }),
    }
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
