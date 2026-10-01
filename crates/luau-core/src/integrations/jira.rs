//! Jira adapter: Cloud (REST v3, ADF, `/search/jql` + `nextPageToken`) and
//! Server / Data Center (REST v2, wiki markup, `/search` + `startAt`).

use serde_json::{Value, json};

use super::http::Http;
use super::types::*;
use super::{adf, wiki};
use crate::error::{Error, Result};

pub const FIELDS: &[&str] = &[
    "summary",
    "description",
    "status",
    "assignee",
    "reporter",
    "priority",
    "labels",
    "issuetype",
    "parent",
    "duedate",
    "updated",
    "created",
    "subtasks",
    "attachment",
    "project",
    "sprint",
];
const PAGE: usize = 50;
/// Upper bound for whole-board fetches (mirrors).
const MAX_BOARD_ISSUES: usize = 2000;

#[derive(Clone)]
pub struct Jira {
    pub http: Http,
    pub cloud: bool,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

pub fn map_user(v: &Value, cloud: bool) -> Option<RemoteUser> {
    if !v.is_object() {
        return None;
    }
    let id = if cloud {
        s(v, "accountId")
    } else {
        s(v, "name").or_else(|| s(v, "key"))
    }?;
    Some(RemoteUser {
        name: s(v, "displayName").unwrap_or_else(|| id.clone()),
        id,
        avatar: v
            .pointer("/avatarUrls/48x48")
            .and_then(Value::as_str)
            .map(str::to_string),
        // Emails are not kept: they are not needed by the UI.
        email: None,
        key: if cloud { None } else { s(v, "key") },
    })
}

fn issue_ref(v: &Value) -> Option<IssueRef> {
    let key = s(v, "key")?;
    let f = v.get("fields").cloned().unwrap_or(Value::Null);
    Some(IssueRef {
        key,
        summary: s(&f, "summary").unwrap_or_default(),
        status: f
            .pointer("/status/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        status_category: f
            .pointer("/status/statusCategory/key")
            .and_then(Value::as_str)
            .map(StatusCategory::from_jira),
    })
}

pub fn description_md(v: &Value, cloud: bool) -> String {
    if cloud {
        adf::to_markdown(v)
    } else {
        wiki::to_markdown(v.as_str().unwrap_or_default())
    }
}

pub fn description_remote(md: &str, cloud: bool) -> Value {
    if cloud {
        adf::from_markdown(md)
    } else {
        Value::String(wiki::from_markdown(md))
    }
}

/// Normalize a Jira issue JSON (REST or Agile) into a [`RemoteIssue`].
pub fn map_issue(v: &Value, base: &str, cloud: bool) -> RemoteIssue {
    let f = v.get("fields").cloned().unwrap_or(Value::Null);
    let key = s(v, "key").unwrap_or_default();
    let status = f.get("status").cloned().unwrap_or(Value::Null);
    RemoteIssue {
        url: format!("{}/browse/{key}", base.trim_end_matches('/')),
        id: s(v, "id").unwrap_or_default(),
        summary: s(&f, "summary").unwrap_or_default(),
        description_md: description_md(f.get("description").unwrap_or(&Value::Null), cloud),
        status: s(&status, "name").unwrap_or_default(),
        status_id: s(&status, "id").unwrap_or_default(),
        status_category: status
            .pointer("/statusCategory/key")
            .and_then(Value::as_str)
            .map(StatusCategory::from_jira)
            .unwrap_or_default(),
        assignee: f.get("assignee").and_then(|u| map_user(u, cloud)),
        reporter: f.get("reporter").and_then(|u| map_user(u, cloud)),
        priority: f
            .pointer("/priority/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        priority_icon: f
            .pointer("/priority/iconUrl")
            .and_then(Value::as_str)
            .map(str::to_string),
        labels: f
            .get("labels")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        issue_type: f
            .pointer("/issuetype/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        type_icon: f
            .pointer("/issuetype/iconUrl")
            .and_then(Value::as_str)
            .map(str::to_string),
        sprint: f
            .pointer("/sprint/name")
            .and_then(Value::as_str)
            .map(str::to_string),
        sprint_id: f.pointer("/sprint/id").map(|x| x.to_string()),
        epic: None,
        parent: f.get("parent").and_then(issue_ref),
        due: s(&f, "duedate"),
        story_points: None,
        updated: s(&f, "updated"),
        created: s(&f, "created"),
        subtasks: f
            .get("subtasks")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(issue_ref).collect())
            .unwrap_or_default(),
        attachments: f
            .get("attachment")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| RemoteAttachment {
                        id: s(x, "id").unwrap_or_default(),
                        filename: s(x, "filename").unwrap_or_default(),
                        mime: s(x, "mimeType").unwrap_or_default(),
                        size: x.get("size").and_then(Value::as_u64).unwrap_or(0),
                        url: s(x, "content").unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        project: f
            .pointer("/project/key")
            .and_then(Value::as_str)
            .map(str::to_string),
        container: None,
        custom: Default::default(),
        key,
    }
}

/// Next `startAt` token for offset pagination, `None` when exhausted.
pub fn next_offset(start_at: usize, got: usize, total: Option<usize>) -> Option<String> {
    if got == 0 {
        return None;
    }
    let next = start_at + got;
    match total {
        Some(t) if next >= t => None,
        _ => Some(next.to_string()),
    }
}

/// Cloud `/search/jql` continuation.
pub fn next_token(v: &Value) -> Option<String> {
    if v.get("isLast").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    s(v, "nextPageToken").filter(|t| !t.is_empty())
}

fn validate_key(key: &str) -> Result<&str> {
    let ok = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(key)
    } else {
        Err(Error::invalid("invalid_issue_key"))
    }
}

impl Jira {
    fn api(&self) -> &'static str {
        if self.cloud {
            "/rest/api/3"
        } else {
            "/rest/api/2"
        }
    }
    fn base(&self) -> String {
        self.http.base().as_str().trim_end_matches('/').to_string()
    }

    pub async fn myself(&self) -> Result<RemoteUser> {
        let v = self
            .http
            .get(&format!("{}/myself", self.api()), &[])
            .await?;
        map_user(&v, self.cloud).ok_or_else(|| Error::Other("remote_bad_json".into()))
    }

    /// `(deploymentType, version)` from serverInfo.
    pub async fn server_info(&self) -> Result<(String, String)> {
        let v = self.http.get("/rest/api/2/serverInfo", &[]).await?;
        Ok((
            s(&v, "deploymentType").unwrap_or_default(),
            s(&v, "version").unwrap_or_default(),
        ))
    }

    pub async fn search(&self, jql: &str, next: Option<&str>) -> Result<SearchPage> {
        let fields: Vec<&str> = FIELDS.to_vec();
        if self.cloud {
            let mut body = json!({ "jql": jql, "maxResults": PAGE, "fields": fields });
            if let Some(t) = next {
                body["nextPageToken"] = json!(t);
            }
            let v = self.http.post("/rest/api/3/search/jql", &body).await?;
            Ok(SearchPage {
                issues: self.issues_of(&v),
                next: next_token(&v),
                total: None,
            })
        } else {
            let start: usize = next.and_then(|n| n.parse().ok()).unwrap_or(0);
            let body =
                json!({ "jql": jql, "startAt": start, "maxResults": PAGE, "fields": fields });
            let v = self.http.post("/rest/api/2/search", &body).await?;
            let issues = self.issues_of(&v);
            let total = v.get("total").and_then(Value::as_u64);
            Ok(SearchPage {
                next: next_offset(start, issues.len(), total.map(|t| t as usize)),
                total,
                issues,
            })
        }
    }

    fn issues_of(&self, v: &Value) -> Vec<RemoteIssue> {
        let base = self.base();
        v.get("issues")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|i| map_issue(i, &base, self.cloud)).collect())
            .unwrap_or_default()
    }

    pub async fn issue(&self, key: &str) -> Result<RemoteIssue> {
        let key = validate_key(key)?;
        let v = self
            .http
            .get(
                &format!("{}/issue/{key}", self.api()),
                &[("fields", FIELDS.join(","))],
            )
            .await?;
        Ok(map_issue(&v, &self.base(), self.cloud))
    }

    /// Fetch many issues by key (`key in (…)`, chunks of 100).
    pub async fn issues(&self, keys: &[String]) -> Result<Vec<RemoteIssue>> {
        let mut out = Vec::new();
        for chunk in keys.chunks(100) {
            let list: Vec<&str> = chunk.iter().filter_map(|k| validate_key(k).ok()).collect();
            if list.is_empty() {
                continue;
            }
            let jql = format!("key in ({})", list.join(","));
            let mut next: Option<String> = None;
            loop {
                let page = self.search(&jql, next.as_deref()).await?;
                out.extend(page.issues);
                match page.next {
                    Some(n) => next = Some(n),
                    None => break,
                }
            }
        }
        Ok(out)
    }

    pub async fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        let key = validate_key(key)?;
        let v = self
            .http
            .get(&format!("{}/issue/{key}/transitions", self.api()), &[])
            .await?;
        Ok(v.get("transitions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|t| Transition {
                        id: s(t, "id").unwrap_or_default(),
                        name: s(t, "name").unwrap_or_default(),
                        to: t
                            .pointer("/to/name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        to_category: t
                            .pointer("/to/statusCategory/key")
                            .and_then(Value::as_str)
                            .map(StatusCategory::from_jira)
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn transition(&self, key: &str, id: &str) -> Result<()> {
        let key = validate_key(key)?;
        self.http
            .post(
                &format!("{}/issue/{key}/transitions", self.api()),
                &json!({ "transition": { "id": id } }),
            )
            .await?;
        Ok(())
    }

    pub async fn comments(&self, key: &str) -> Result<Vec<RemoteComment>> {
        let key = validate_key(key)?;
        let v = self
            .http
            .get(
                &format!("{}/issue/{key}/comment", self.api()),
                &[("orderBy", "created".into()), ("maxResults", "100".into())],
            )
            .await?;
        Ok(v.get("comments")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|c| RemoteComment {
                        id: s(c, "id").unwrap_or_default(),
                        author: c.get("author").and_then(|u| map_user(u, self.cloud)),
                        body_md: description_md(c.get("body").unwrap_or(&Value::Null), self.cloud),
                        created: s(c, "created"),
                        updated: s(c, "updated"),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn add_comment(&self, key: &str, md: &str) -> Result<()> {
        let key = validate_key(key)?;
        self.http
            .post(
                &format!("{}/issue/{key}/comment", self.api()),
                &json!({ "body": description_remote(md, self.cloud) }),
            )
            .await?;
        Ok(())
    }

    pub async fn update(&self, key: &str, summary: Option<&str>, md: Option<&str>) -> Result<()> {
        let key = validate_key(key)?;
        let mut fields = serde_json::Map::new();
        if let Some(t) = summary {
            fields.insert("summary".into(), json!(t));
        }
        if let Some(m) = md {
            fields.insert("description".into(), description_remote(m, self.cloud));
        }
        self.http
            .put(
                &format!("{}/issue/{key}", self.api()),
                &json!({ "fields": fields }),
            )
            .await?;
        Ok(())
    }

    pub async fn create(&self, req: &CreateIssue) -> Result<String> {
        let mut fields = json!({
            "project": { "key": req.project },
            "summary": req.summary,
            "issuetype": { "name": req.issue_type.clone().unwrap_or_else(|| "Task".into()) },
        });
        if !req.description_md.trim().is_empty() {
            fields["description"] = description_remote(&req.description_md, self.cloud);
        }
        let v = self
            .http
            .post(
                &format!("{}/issue", self.api()),
                &json!({ "fields": fields }),
            )
            .await?;
        s(&v, "key").ok_or_else(|| Error::Other("remote_bad_json".into()))
    }

    pub async fn assign(&self, key: &str, user: Option<&RemoteUser>) -> Result<()> {
        let key = validate_key(key)?;
        let body = match (user, self.cloud) {
            (Some(u), true) => json!({ "accountId": u.id }),
            (Some(u), false) => json!({ "name": u.id }),
            (None, true) => json!({ "accountId": null }),
            (None, false) => json!({ "name": null }),
        };
        self.http
            .put(&format!("{}/issue/{key}/assignee", self.api()), &body)
            .await?;
        Ok(())
    }

    pub async fn users(&self, q: &str) -> Result<Vec<RemoteUser>> {
        let (path, param) = if self.cloud {
            ("/rest/api/3/user/search", "query")
        } else {
            ("/rest/api/2/user/search", "username")
        };
        let v = self
            .http
            .get(path, &[(param, q.to_string()), ("maxResults", "20".into())])
            .await?;
        Ok(v.as_array()
            .map(|a| a.iter().filter_map(|u| map_user(u, self.cloud)).collect())
            .unwrap_or_default())
    }

    pub async fn projects(&self) -> Result<Vec<IdName>> {
        let v = if self.cloud {
            self.http
                .get(
                    "/rest/api/3/project/search",
                    &[("maxResults", "100".into())],
                )
                .await?["values"]
                .clone()
        } else {
            self.http.get("/rest/api/2/project", &[]).await?
        };
        Ok(v.as_array()
            .map(|a| {
                a.iter()
                    .map(|p| IdName {
                        id: s(p, "id").unwrap_or_default(),
                        name: s(p, "name").unwrap_or_default(),
                        key: s(p, "key"),
                        icon: None,
                        detail: None,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn issue_types(&self, project: &str) -> Result<Vec<IdName>> {
        let project = validate_key(project)?;
        let v = self
            .http
            .get(&format!("{}/project/{project}", self.api()), &[])
            .await?;
        Ok(v.get("issueTypes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|t| t.get("subtask").and_then(Value::as_bool) != Some(true))
                    .map(|t| IdName {
                        id: s(t, "id").unwrap_or_default(),
                        name: s(t, "name").unwrap_or_default(),
                        key: None,
                        icon: s(t, "iconUrl"),
                        detail: None,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn boards(&self) -> Result<Vec<IdName>> {
        let mut out = Vec::new();
        let mut start = 0usize;
        loop {
            let v = self
                .http
                .get(
                    "/rest/agile/1.0/board",
                    &[("startAt", start.to_string()), ("maxResults", "50".into())],
                )
                .await?;
            let vals = v
                .get("values")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for b in &vals {
                out.push(IdName {
                    id: b
                        .get("id")
                        .map(|x| x.to_string().trim_matches('"').to_string())
                        .unwrap_or_default(),
                    name: s(b, "name").unwrap_or_default(),
                    key: b
                        .pointer("/location/projectKey")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    icon: None,
                    detail: s(b, "type"),
                });
            }
            let last = v.get("isLast").and_then(Value::as_bool).unwrap_or(true);
            if last || vals.is_empty() || out.len() >= 500 {
                break;
            }
            start += vals.len();
        }
        Ok(out)
    }

    /// Board columns (with their status ids) and the issues on the board, by rank.
    pub async fn board(&self, id: &str) -> Result<(RemoteBoard, Vec<RemoteIssue>)> {
        if !id.chars().all(|c| c.is_ascii_digit()) || id.is_empty() {
            return Err(Error::invalid("invalid_board_id"));
        }
        let cfg = self
            .http
            .get(&format!("/rest/agile/1.0/board/{id}/configuration"), &[])
            .await?;
        let columns = cfg
            .pointer("/columnConfig/columns")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|c| RemoteColumn {
                        name: s(c, "name").unwrap_or_default(),
                        statuses: c
                            .get("statuses")
                            .and_then(Value::as_array)
                            .map(|st| st.iter().filter_map(|x| s(x, "id")).collect())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let board = RemoteBoard {
            id: id.into(),
            name: s(&cfg, "name").unwrap_or_default(),
            kind: s(&cfg, "type").unwrap_or_else(|| "kanban".into()),
            columns,
            sprints: vec![],
            project: cfg
                .pointer("/location/key")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        let mut issues = Vec::new();
        let mut start = 0usize;
        let base = self.base();
        loop {
            let v = self
                .http
                .get(
                    &format!("/rest/agile/1.0/board/{id}/issue"),
                    &[
                        ("startAt", start.to_string()),
                        ("maxResults", "100".into()),
                        ("fields", FIELDS.join(",")),
                    ],
                )
                .await?;
            let got: Vec<RemoteIssue> = v
                .get("issues")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|i| map_issue(i, &base, self.cloud)).collect())
                .unwrap_or_default();
            let total = v.get("total").and_then(Value::as_u64).map(|t| t as usize);
            let n = got.len();
            issues.extend(got);
            match next_offset(start, n, total) {
                Some(nx) if issues.len() < MAX_BOARD_ISSUES => {
                    start = nx.parse().unwrap_or(start + n)
                }
                _ => break,
            }
        }
        Ok((board, issues))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud_issue() -> Value {
        json!({
            "id": "10001", "key": "LUAU-7",
            "fields": {
                "summary": "Fix login",
                "description": { "type": "doc", "version": 1, "content": [
                    { "type": "paragraph", "content": [ { "type": "text", "text": "Steps " }, { "type": "text", "text": "here", "marks": [ { "type": "strong" } ] } ] }
                ]},
                "status": { "id": "3", "name": "In Progress", "statusCategory": { "key": "indeterminate" } },
                "assignee": { "accountId": "abc", "displayName": "Ana Pérez", "emailAddress": "ana@x.io", "avatarUrls": { "48x48": "https://a/48.png" } },
                "priority": { "name": "High", "iconUrl": "https://a/p.svg" },
                "labels": ["auth", "web"],
                "issuetype": { "name": "Bug" },
                "parent": { "key": "LUAU-1", "fields": { "summary": "Epic", "status": { "name": "To Do", "statusCategory": { "key": "new" } } } },
                "attachment": [ { "id": "9", "filename": "shot.png", "mimeType": "image/png", "size": 12, "content": "https://acme.atlassian.net/rest/api/3/attachment/content/9" } ],
                "project": { "key": "LUAU" },
                "sprint": { "id": 4, "name": "Sprint 4" }
            }
        })
    }

    #[test]
    fn maps_cloud_issue() {
        let i = map_issue(&cloud_issue(), "https://acme.atlassian.net/", true);
        assert_eq!(i.key, "LUAU-7");
        assert_eq!(i.url, "https://acme.atlassian.net/browse/LUAU-7");
        assert_eq!(i.description_md, "Steps **here**");
        assert_eq!(i.status, "In Progress");
        assert_eq!(i.status_category, StatusCategory::InProgress);
        let a = i.assignee.unwrap();
        assert_eq!((a.id.as_str(), a.name.as_str()), ("abc", "Ana Pérez"));
        assert!(a.email.is_none(), "emails are never kept");
        assert_eq!(i.labels, vec!["auth", "web"]);
        assert_eq!(i.issue_type.as_deref(), Some("Bug"));
        assert_eq!(
            i.parent.unwrap().status_category,
            Some(StatusCategory::Todo)
        );
        assert_eq!(i.attachments[0].filename, "shot.png");
        assert_eq!(i.sprint.as_deref(), Some("Sprint 4"));
        assert_eq!(i.project.as_deref(), Some("LUAU"));
    }

    #[test]
    fn maps_server_issue_with_wiki() {
        let v = json!({ "id": "1", "key": "OPS-2", "fields": {
            "summary": "Disk", "description": "h2. Title\n*bold* text",
            "status": { "id": "10", "name": "Done", "statusCategory": { "key": "done" } },
            "assignee": { "name": "luis", "key": "JIRAUSER1", "displayName": "Luis" }
        }});
        let i = map_issue(&v, "https://jira.corp", false);
        assert_eq!(i.description_md, "## Title\n\n**bold** text");
        assert_eq!(i.status_category, StatusCategory::Done);
        let a = i.assignee.unwrap();
        assert_eq!(a.id, "luis");
        assert_eq!(a.key.as_deref(), Some("JIRAUSER1"));
    }

    #[test]
    fn pagination_tokens() {
        assert_eq!(next_offset(0, 50, Some(120)), Some("50".into()));
        assert_eq!(next_offset(100, 20, Some(120)), None);
        assert_eq!(next_offset(0, 0, None), None);
        assert_eq!(next_offset(0, 50, None), Some("50".into()));
        assert_eq!(
            next_token(&json!({ "nextPageToken": "abc", "isLast": false })),
            Some("abc".into())
        );
        assert_eq!(
            next_token(&json!({ "nextPageToken": "abc", "isLast": true })),
            None
        );
        assert_eq!(next_token(&json!({})), None);
    }

    #[test]
    fn keys_are_validated() {
        assert!(validate_key("LUAU-7").is_ok());
        assert!(validate_key("../etc").is_err());
        assert!(validate_key("A B").is_err());
        assert!(validate_key("").is_err());
    }
}
