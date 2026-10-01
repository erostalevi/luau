//! Normalized remote-issue types shared by every provider (SPEC §9.7).
//! Serialized camelCase for the UI (`src/lib/integrations/types.ts`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which service an account talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    /// Jira Cloud (REST v3, ADF, accountId users).
    JiraCloud,
    /// Jira Server / Data Center (REST v2, wiki markup, name/key users).
    JiraServer,
    Trello,
    Slack,
}

impl ProviderKind {
    pub fn is_jira(self) -> bool {
        matches!(self, ProviderKind::JiraCloud | ProviderKind::JiraServer)
    }
    /// Short service name used in links, journal entries and footers.
    pub fn service(self) -> &'static str {
        match self {
            ProviderKind::JiraCloud | ProviderKind::JiraServer => "jira",
            ProviderKind::Trello => "trello",
            ProviderKind::Slack => "slack",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::JiraCloud | ProviderKind::JiraServer => "Jira",
            ProviderKind::Trello => "Trello",
            ProviderKind::Slack => "Slack",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusCategory {
    #[default]
    Todo,
    InProgress,
    Done,
}

impl StatusCategory {
    /// Map Jira's `statusCategory.key` (`new`, `indeterminate`, `done`).
    pub fn from_jira(key: &str) -> Self {
        match key {
            "done" => StatusCategory::Done,
            "indeterminate" => StatusCategory::InProgress,
            _ => StatusCategory::Todo,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUser {
    /// accountId (Cloud), name/key (Server), member id (Trello).
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// Never logged; only shown in the UI when the service exposes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Server/DC `key` (differs from `name` on old instances).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueRef {
    pub key: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_category: Option<StatusCategory>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAttachment {
    pub id: String,
    pub filename: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteIssue {
    pub key: String,
    pub id: String,
    pub url: String,
    pub summary: String,
    /// Description converted to Markdown (ADF / wiki markup / Trello Markdown).
    #[serde(default)]
    pub description_md: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub status_id: String,
    #[serde(default)]
    pub status_category: StatusCategory,
    #[serde(default)]
    pub assignee: Option<RemoteUser>,
    #[serde(default)]
    pub reporter: Option<RemoteUser>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub priority_icon: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default, rename = "type")]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub type_icon: Option<String>,
    #[serde(default)]
    pub sprint: Option<String>,
    #[serde(default)]
    pub sprint_id: Option<String>,
    #[serde(default)]
    pub epic: Option<IssueRef>,
    #[serde(default)]
    pub parent: Option<IssueRef>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub story_points: Option<f64>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub subtasks: Vec<IssueRef>,
    #[serde(default)]
    pub attachments: Vec<RemoteAttachment>,
    #[serde(default)]
    pub project: Option<String>,
    /// Remote container (Trello board id) when relevant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// Custom fields (id → display value), filled when requested.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub issues: Vec<RemoteIssue>,
    /// Opaque continuation token (nextPageToken / startAt / page number).
    pub next: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: String,
    #[serde(default)]
    pub to_category: StatusCategory,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Priority {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteComment {
    pub id: String,
    pub author: Option<RemoteUser>,
    pub body_md: String,
    pub created: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdName {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Free-form extra (board type, filter JQL, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteColumn {
    pub name: String,
    /// Status ids (Jira) or list id (Trello) mapped to this column.
    pub statuses: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sprint {
    pub id: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBoard {
    pub id: String,
    pub name: String,
    /// `scrum` | `kanban` | `trello`.
    pub kind: String,
    pub columns: Vec<RemoteColumn>,
    #[serde(default)]
    pub sprints: Vec<Sprint>,
    #[serde(default)]
    pub project: Option<String>,
}

/// Issues of a board for one view, ordered by rank.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardIssues {
    pub issues: Vec<RemoteIssue>,
    /// Backlog view: one bucket per future sprint plus the `backlog` bucket.
    #[serde(default)]
    pub buckets: Vec<Bucket>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub id: String,
    pub name: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoardViewKind {
    #[default]
    Sprint,
    Backlog,
}

/// Simple filter builder (panel) — converted to JQL / provider search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FilterQuery {
    pub project: Option<String>,
    pub status: Option<String>,
    pub assignee: Option<String>,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub labels: Vec<String>,
    pub sprint: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssue {
    pub project: String,
    #[serde(default)]
    pub issue_type: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub description_md: String,
    /// Trello: list id.
    #[serde(default)]
    pub container: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    pub transitions: bool,
    pub assign: bool,
    pub labels: bool,
    pub priority: bool,
    pub comments: bool,
    pub create: bool,
    pub edit: bool,
    pub sprints: bool,
    pub jql: bool,
    pub story_points: bool,
    pub saved_filters: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub value: String,
    pub label: String,
}
