//! `<board>/.lull/remote.json`: card ↔ remote-issue links (linked copies and
//! mirror cards) plus mirror sync state. The Markdown files stay readable
//! without the app: a linked card ends with `Jira: [KEY](url)`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::types::*;
use crate::error::{Error, Result};
use crate::fsutil::atomic_write;
use crate::store::marker_dir;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LinkInfo {
    pub status: Option<String>,
    pub status_category: Option<StatusCategory>,
    pub assignee: Option<RemoteUser>,
    pub priority: Option<String>,
    pub labels: Vec<String>,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub sprint: Option<String>,
    pub updated: Option<String>,
}

impl LinkInfo {
    pub fn from_issue(i: &RemoteIssue) -> Self {
        LinkInfo {
            status: Some(i.status.clone()).filter(|s| !s.is_empty()),
            status_category: Some(i.status_category),
            assignee: i.assignee.clone().map(|mut u| {
                u.email = None;
                u
            }),
            priority: i.priority.clone(),
            labels: i.labels.clone(),
            issue_type: i.issue_type.clone(),
            sprint: i.sprint.clone(),
            updated: i.updated.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub account: String,
    pub provider: ProviderKind,
    pub key: String,
    #[serde(default)]
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub info: LinkInfo,
    #[serde(default)]
    pub unavailable: bool,
    /// Hash of the remote title+description at the last link/pull (detects remote edits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced: Option<String>,
    /// Local attachment file → remote attachment name (restores refs on push).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
}

/// What the UI receives (`RemoteInfo` in `backend/types.ts`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfoDto {
    pub provider: &'static str,
    pub account: String,
    pub key: String,
    pub url: String,
    #[serde(flatten)]
    pub info: LinkInfo,
    pub unavailable: bool,
    pub mirror: bool,
}

impl Link {
    pub fn dto(&self, mirror: bool) -> RemoteInfoDto {
        RemoteInfoDto {
            provider: self.provider.service(),
            account: self.account.clone(),
            key: self.key.clone(),
            url: self.url.clone(),
            info: self.info.clone(),
            unavailable: self.unavailable,
            mirror,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum MirrorSource {
    /// Jira agile board (columns from its configuration).
    Board { id: String },
    /// A JQL query (lanes = statuses).
    Query { query: String },
    /// A Trello board (lanes = lists).
    Trello { id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorState {
    pub account: String,
    pub provider: ProviderKind,
    pub source: MirrorSource,
    #[serde(default)]
    pub watch: bool,
    #[serde(default)]
    pub last_sync: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Column / status name → lane id.
    #[serde(default)]
    pub columns: BTreeMap<String, String>,
    /// Issue key → card id.
    #[serde(default)]
    pub issues: BTreeMap<String, String>,
    /// Issue key → hash of the remote content last written (skips rewrites).
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteFile {
    pub schema: u32,
    pub links: BTreeMap<String, Link>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirror: Option<MirrorState>,
}

pub fn path(root: &Path) -> PathBuf {
    marker_dir(root).join("remote.json")
}

pub fn load(root: &Path) -> RemoteFile {
    std::fs::read_to_string(path(root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(root: &Path, f: &RemoteFile) -> Result<()> {
    let mut f = f.clone();
    f.schema = 1;
    let t = serde_json::to_string_pretty(&f).map_err(|e| Error::Other(e.to_string()))?;
    atomic_write(&path(root), t.as_bytes())
}

/// Private notes for mirror cards (never sent anywhere).
pub fn notes_path(root: &Path, card: &str) -> Result<PathBuf> {
    if !crate::ids::is_id(card, crate::ids::IdKind::Card) {
        return Err(Error::invalid("invalid card id"));
    }
    Ok(marker_dir(root).join("notes").join(format!("{card}.md")))
}

/// The provenance line appended to linked cards.
pub fn footer_line(provider: ProviderKind, key: &str, url: &str) -> String {
    format!("{}: [{key}]({url})", provider.label())
}

/// Card file content for a remote issue: title, description, provenance line.
pub fn compose(provider: ProviderKind, summary: &str, md: &str, key: &str, url: &str) -> String {
    let title = summary.replace(['\n', '\r'], " ");
    let title = if title.trim().is_empty() {
        key.to_string()
    } else {
        title.trim().to_string()
    };
    let body = md.trim();
    let line = footer_line(provider, key, url);
    if body.is_empty() {
        format!("# {title}\n\n{line}\n")
    } else {
        format!("# {title}\n\n{body}\n\n{line}\n")
    }
}

/// Split card content into (title, description) — the inverse of [`compose`]:
/// drops the `# ` title line and the provenance line for `key`.
pub fn split(content: &str, key: &str) -> (String, String) {
    let mut lines: Vec<&str> = content.lines().collect();
    let mut title = String::new();
    if let Some(i) = lines.iter().position(|l| !l.trim().is_empty())
        && let Some(t) = lines[i].strip_prefix("# ")
    {
        title = t.trim().to_string();
        lines.remove(i);
    }
    let marker = format!(": [{key}](");
    lines.retain(|l| {
        !(l.contains(&marker)
            && l.trim_end().ends_with(')')
            && l.len() < 400
            && !l.starts_with(' '))
    });
    let body = lines.join("\n").trim().to_string();
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_split_roundtrip() {
        let c = compose(
            ProviderKind::JiraCloud,
            "Fix login",
            "Steps **here**\n\n- a",
            "LULL-7",
            "https://a/browse/LULL-7",
        );
        assert_eq!(
            c,
            "# Fix login\n\nSteps **here**\n\n- a\n\nJira: [LULL-7](https://a/browse/LULL-7)\n"
        );
        assert_eq!(
            split(&c, "LULL-7"),
            ("Fix login".into(), "Steps **here**\n\n- a".into())
        );
        let empty = compose(ProviderKind::Trello, " ", "", "AbC", "https://t/c/AbC");
        assert_eq!(empty, "# AbC\n\nTrello: [AbC](https://t/c/AbC)\n");
        assert_eq!(split(&empty, "AbC"), ("AbC".into(), String::new()));
    }

    #[test]
    fn split_keeps_other_links() {
        let c = "# T\n\nSee Jira: [OTHER-1](https://x) too\n\nJira: [K-1](https://a)\n";
        assert_eq!(split(c, "K-1").1, "See Jira: [OTHER-1](https://x) too");
    }

    #[test]
    fn file_roundtrip_and_notes_path() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(marker_dir(d.path())).unwrap();
        let mut f = RemoteFile::default();
        f.links.insert(
            "cabc123".into(),
            Link {
                account: "a".into(),
                provider: ProviderKind::JiraCloud,
                key: "K-1".into(),
                id: "1".into(),
                url: "u".into(),
                info: LinkInfo::default(),
                unavailable: false,
                synced: None,
                images: BTreeMap::new(),
            },
        );
        save(d.path(), &f).unwrap();
        let g = load(d.path());
        assert_eq!(g.links, f.links);
        assert_eq!(g.schema, 1);
        assert!(notes_path(d.path(), "../../x").is_err());
    }

    #[test]
    fn dto_has_no_email() {
        let i = RemoteIssue {
            assignee: Some(RemoteUser {
                id: "x".into(),
                name: "Ana".into(),
                email: Some("ana@x.io".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let info = LinkInfo::from_issue(&i);
        assert!(info.assignee.unwrap().email.is_none());
    }
}
