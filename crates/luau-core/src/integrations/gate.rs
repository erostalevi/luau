//! WriteGate (SPEC §9.6): every change crossing the app boundary — pushes to
//! a service and pulls into local cards — is first *prepared* into a change
//! list the UI shows as "This will update X and Y on Z", then *committed* with
//! the single-use token from that preview. Nothing can write without a token.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::links::MirrorState;
use super::types::*;
use crate::error::{Error, Result};

const TTL: Duration = Duration::from_secs(10 * 60);
const PREVIEW_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Local → service.
    Push,
    /// Service → local.
    Pull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRow {
    pub field: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What the confirmation dialog shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub token: String,
    pub direction: Direction,
    /// `jira`, `trello`, `slack`, or `local` for pulls into a card.
    pub service: String,
    /// Site host (`acme.atlassian.net`).
    pub site: String,
    /// The item affected (`PROJ-1 "Fix login"`, a card title, `#general`).
    pub target: String,
    /// Names of the fields that change (`Summary`, `Description`, `Status`).
    pub fields: Vec<String>,
    pub changes: Vec<ChangeRow>,
    /// Free-form preview (comment / message text).
    pub preview: Option<String>,
    /// True when nothing would change (the UI just reports "up to date").
    pub empty: bool,
}

/// The action executed on commit (everything needed, captured at preview time).
#[derive(Debug, Clone)]
pub enum Action {
    PushCard {
        account: String,
        key: String,
        summary: Option<String>,
        md: Option<String>,
    },
    PullCard {
        board: String,
        card: String,
        content: String,
        issue: Box<RemoteIssue>,
    },
    PullMirror {
        board: String,
        state: Box<MirrorState>,
        remote: Box<(RemoteBoard, Vec<RemoteIssue>)>,
    },
    Comment {
        account: String,
        key: String,
        md: String,
        ctx: Option<(String, String)>,
    },
    Transition {
        account: String,
        key: String,
        id: String,
        ctx: Option<(String, String)>,
    },
    Assign {
        account: String,
        key: String,
        user: Option<RemoteUser>,
        ctx: Option<(String, String)>,
    },
    Create {
        account: String,
        board: String,
        card: String,
        req: CreateIssue,
    },
    SlackPost {
        account: String,
        channel: String,
        text: String,
    },
}

struct Entry {
    action: Action,
    at: Instant,
}

static PENDING: LazyLock<Mutex<HashMap<String, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn short(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() <= PREVIEW_CHARS {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(PREVIEW_CHARS).collect::<String>())
    }
}

/// Field-by-field diff: only fields whose value changes are listed.
pub fn diff(fields: &[(&str, Option<&str>, Option<&str>)]) -> Vec<ChangeRow> {
    fields
        .iter()
        .filter(|(_, b, a)| b.map(str::trim) != a.map(str::trim))
        .map(|(f, b, a)| ChangeRow {
            field: f.to_string(),
            before: b.map(short),
            after: a.map(short),
        })
        .collect()
}

pub struct Draft {
    pub direction: Direction,
    pub service: String,
    pub site: String,
    pub target: String,
    pub changes: Vec<ChangeRow>,
    pub preview: Option<String>,
    pub action: Action,
}

/// Register a pending write and return what the UI must confirm.
pub fn prepare(d: Draft) -> Prepared {
    let mut raw = [0u8; 16];
    let _ = getrandom::fill(&mut raw);
    let token = format!("w{}", hex::encode(raw));
    let mut fields: Vec<String> = Vec::new();
    for c in &d.changes {
        if !fields.contains(&c.field) {
            fields.push(c.field.clone());
        }
    }
    let empty = d.changes.is_empty() && d.preview.is_none();
    let mut p = PENDING.lock();
    p.retain(|_, e| e.at.elapsed() < TTL);
    if !empty {
        p.insert(
            token.clone(),
            Entry {
                action: d.action,
                at: Instant::now(),
            },
        );
    }
    Prepared {
        token,
        direction: d.direction,
        service: d.service,
        site: d.site,
        target: d.target,
        fields,
        changes: d.changes,
        preview: d.preview.map(|p| short(&p)),
        empty,
    }
}

/// Take the action for a token (single use, expires after 10 minutes).
pub fn take(token: &str) -> Result<Action> {
    let mut p = PENDING.lock();
    let e = p
        .remove(token)
        .ok_or_else(|| Error::invalid("write_token_invalid"))?;
    if e.at.elapsed() >= TTL {
        return Err(Error::invalid("write_token_expired"));
    }
    Ok(e.action)
}

/// Drop a prepared write (dialog cancelled).
pub fn cancel(token: &str) {
    PENDING.lock().remove(token);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(changes: Vec<ChangeRow>) -> Draft {
        Draft {
            direction: Direction::Push,
            service: "jira".into(),
            site: "acme.atlassian.net".into(),
            target: "K-1".into(),
            changes,
            preview: None,
            action: Action::Comment {
                account: "a".into(),
                key: "K-1".into(),
                md: "hi".into(),
                ctx: None,
            },
        }
    }

    #[test]
    fn diff_lists_only_changed_fields() {
        let rows = diff(&[
            ("Summary", Some("Fix login"), Some("Fix login ")),
            ("Description", Some("old"), Some("new")),
            ("Status", None, Some("Done")),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].field, "Description");
        assert_eq!(rows[1].before, None);
        let long = "x".repeat(1000);
        let r = diff(&[("Description", Some(""), Some(&long))]);
        assert!(r[0].after.as_ref().unwrap().chars().count() <= PREVIEW_CHARS + 1);
    }

    #[test]
    fn tokens_are_single_use() {
        let p = prepare(draft(diff(&[("Summary", Some("a"), Some("b"))])));
        assert_eq!(p.fields, vec!["Summary"]);
        assert!(!p.empty);
        assert!(take(&p.token).is_ok());
        assert!(take(&p.token).is_err());
        assert!(take("nope").is_err());
    }

    #[test]
    fn empty_changes_register_nothing() {
        let p = prepare(draft(vec![]));
        assert!(p.empty);
        assert!(take(&p.token).is_err());
    }

    #[test]
    fn cancel_drops() {
        let p = prepare(draft(diff(&[("Status", Some("To Do"), Some("Done"))])));
        cancel(&p.token);
        assert!(take(&p.token).is_err());
    }
}
