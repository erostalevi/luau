//! Trello adapter (REST v1). Auth via the `Authorization: OAuth …` header so the
//! key and token never appear in URLs. Lists play the role of statuses.

use serde_json::{Value, json};

use super::http::Http;
use super::types::*;
use crate::error::{Error, Result};

const CARD_FIELDS: &str =
    "name,desc,idList,idBoard,shortLink,shortUrl,url,labels,due,dateLastActivity,idShort";

#[derive(Clone)]
pub struct Trello {
    pub http: Http,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn valid_id(id: &str) -> Result<&str> {
    if !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(Error::invalid("invalid_id"))
    }
}

pub fn map_member(v: &Value) -> Option<RemoteUser> {
    Some(RemoteUser {
        id: s(v, "id")?,
        name: s(v, "fullName")
            .or_else(|| s(v, "username"))
            .unwrap_or_default(),
        avatar: s(v, "avatarUrl").map(|u| format!("{u}/50.png")),
        email: None,
        key: s(v, "username"),
    })
}

/// Map a Trello card. `list_name` resolves `idList` to its list (the "status").
pub fn map_card(v: &Value, list_name: &dyn Fn(&str) -> Option<String>) -> RemoteIssue {
    let id_list = s(v, "idList").unwrap_or_default();
    let status = list_name(&id_list).unwrap_or_default();
    let lower = status.to_lowercase();
    let cat = if lower.contains("done") || lower.contains("hecho") || lower.contains("feito") {
        StatusCategory::Done
    } else if lower.contains("doing") || lower.contains("progress") || lower.contains("curso") {
        StatusCategory::InProgress
    } else {
        StatusCategory::Todo
    };
    RemoteIssue {
        key: s(v, "shortLink").unwrap_or_default(),
        id: s(v, "id").unwrap_or_default(),
        url: s(v, "shortUrl").or_else(|| s(v, "url")).unwrap_or_default(),
        summary: s(v, "name").unwrap_or_default(),
        description_md: s(v, "desc").unwrap_or_default(),
        status,
        status_id: id_list,
        status_category: cat,
        assignee: v
            .get("members")
            .and_then(Value::as_array)
            .and_then(|m| m.first())
            .and_then(map_member),
        labels: v
            .get("labels")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| s(l, "name").filter(|n| !n.is_empty()))
                    .collect()
            })
            .unwrap_or_default(),
        due: s(v, "due"),
        updated: s(v, "dateLastActivity"),
        attachments: v
            .get("attachments")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|x| x.get("isUpload").and_then(Value::as_bool) == Some(true))
                    .map(|x| RemoteAttachment {
                        id: s(x, "id").unwrap_or_default(),
                        filename: s(x, "name").unwrap_or_default(),
                        mime: s(x, "mimeType").unwrap_or_default(),
                        size: x.get("bytes").and_then(Value::as_u64).unwrap_or(0),
                        url: s(x, "url").unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        project: s(v, "idBoard"),
        container: s(v, "idBoard"),
        issue_type: Some("Card".into()),
        ..Default::default()
    }
}

impl Trello {
    pub async fn me(&self) -> Result<RemoteUser> {
        let v = self
            .http
            .get(
                "/1/members/me",
                &[("fields", "fullName,username,avatarUrl".into())],
            )
            .await?;
        map_member(&v).ok_or_else(|| Error::Other("remote_bad_json".into()))
    }

    pub async fn boards(&self) -> Result<Vec<IdName>> {
        let v = self
            .http
            .get(
                "/1/members/me/boards",
                &[("filter", "open".into()), ("fields", "name,url".into())],
            )
            .await?;
        Ok(v.as_array()
            .map(|a| {
                a.iter()
                    .map(|b| IdName {
                        id: s(b, "id").unwrap_or_default(),
                        name: s(b, "name").unwrap_or_default(),
                        key: None,
                        icon: None,
                        detail: Some("trello".into()),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn lists(&self, board: &str) -> Result<Vec<IdName>> {
        let board = valid_id(board)?;
        let v = self
            .http
            .get(
                &format!("/1/boards/{board}/lists"),
                &[("filter", "open".into())],
            )
            .await?;
        Ok(v.as_array()
            .map(|a| {
                a.iter()
                    .map(|l| IdName {
                        id: s(l, "id").unwrap_or_default(),
                        name: s(l, "name").unwrap_or_default(),
                        ..Default::default()
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn list_names(&self, board: &str) -> Result<Vec<IdName>> {
        self.lists(board).await
    }

    pub async fn board(&self, board: &str) -> Result<(RemoteBoard, Vec<RemoteIssue>)> {
        let board = valid_id(board)?;
        let meta = self
            .http
            .get(&format!("/1/boards/{board}"), &[("fields", "name".into())])
            .await?;
        let lists = self.lists(board).await?;
        let cards = self
            .http
            .get(
                &format!("/1/boards/{board}/cards"),
                &[
                    ("filter", "open".into()),
                    ("fields", CARD_FIELDS.into()),
                    ("attachments", "true".into()),
                    ("members", "true".into()),
                ],
            )
            .await?;
        let name_of = |id: &str| lists.iter().find(|l| l.id == id).map(|l| l.name.clone());
        let issues = cards
            .as_array()
            .map(|a| a.iter().map(|c| map_card(c, &name_of)).collect())
            .unwrap_or_default();
        Ok((
            RemoteBoard {
                id: board.into(),
                name: s(&meta, "name").unwrap_or_default(),
                kind: "trello".into(),
                columns: lists
                    .iter()
                    .map(|l| RemoteColumn {
                        name: l.name.clone(),
                        statuses: vec![l.id.clone()],
                    })
                    .collect(),
                sprints: vec![],
                project: Some(board.into()),
            },
            issues,
        ))
    }

    pub async fn card(&self, key: &str) -> Result<RemoteIssue> {
        let key = valid_id(key)?;
        let v = self
            .http
            .get(
                &format!("/1/cards/{key}"),
                &[
                    ("fields", CARD_FIELDS.into()),
                    ("attachments", "true".into()),
                    ("members", "true".into()),
                    ("list", "true".into()),
                ],
            )
            .await?;
        let list = v.get("list").cloned().unwrap_or(Value::Null);
        Ok(map_card(&v, &|id| {
            (s(&list, "id").as_deref() == Some(id))
                .then(|| s(&list, "name"))
                .flatten()
        }))
    }

    /// Card search (pages are 0-based page numbers).
    pub async fn search(&self, q: &str, next: Option<&str>) -> Result<SearchPage> {
        let page: usize = next.and_then(|n| n.parse().ok()).unwrap_or(0);
        let query = if q.trim().is_empty() {
            "is:open".to_string()
        } else {
            q.to_string()
        };
        let v = self
            .http
            .get(
                "/1/search",
                &[
                    ("query", query),
                    ("modelTypes", "cards".into()),
                    ("cards_limit", "50".into()),
                    ("cards_page", page.to_string()),
                    ("card_fields", CARD_FIELDS.into()),
                    ("card_list", "true".into()),
                ],
            )
            .await?;
        let cards = v
            .get("cards")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let issues: Vec<RemoteIssue> = cards
            .iter()
            .map(|c| {
                let list = c.get("list").cloned().unwrap_or(Value::Null);
                map_card(c, &|_| s(&list, "name"))
            })
            .collect();
        Ok(SearchPage {
            next: (issues.len() == 50).then(|| (page + 1).to_string()),
            total: None,
            issues,
        })
    }

    /// Transitions = the other lists of the card's board.
    pub async fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        let c = self.card(key).await?;
        let board = c.container.clone().unwrap_or_default();
        Ok(self
            .list_names(&board)
            .await?
            .into_iter()
            .filter(|l| l.id != c.status_id)
            .map(|l| Transition {
                id: l.id,
                name: l.name.clone(),
                to: l.name,
                to_category: StatusCategory::Todo,
            })
            .collect())
    }

    pub async fn transition(&self, key: &str, list: &str) -> Result<()> {
        let key = valid_id(key)?;
        self.http
            .put(
                &format!("/1/cards/{key}"),
                &json!({ "idList": valid_id(list)? }),
            )
            .await?;
        Ok(())
    }

    pub async fn comments(&self, key: &str) -> Result<Vec<RemoteComment>> {
        let key = valid_id(key)?;
        let v = self
            .http
            .get(
                &format!("/1/cards/{key}/actions"),
                &[("filter", "commentCard".into())],
            )
            .await?;
        let mut out: Vec<RemoteComment> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| RemoteComment {
                        id: s(x, "id").unwrap_or_default(),
                        author: x.get("memberCreator").and_then(map_member),
                        body_md: x
                            .pointer("/data/text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        created: s(x, "date"),
                        updated: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.reverse();
        Ok(out)
    }

    pub async fn add_comment(&self, key: &str, md: &str) -> Result<()> {
        let key = valid_id(key)?;
        self.http
            .post(
                &format!("/1/cards/{key}/actions/comments"),
                &json!({ "text": md }),
            )
            .await?;
        Ok(())
    }

    pub async fn update(&self, key: &str, summary: Option<&str>, md: Option<&str>) -> Result<()> {
        let key = valid_id(key)?;
        let mut body = serde_json::Map::new();
        if let Some(t) = summary {
            body.insert("name".into(), json!(t));
        }
        if let Some(m) = md {
            body.insert("desc".into(), json!(m));
        }
        self.http
            .put(&format!("/1/cards/{key}"), &Value::Object(body))
            .await?;
        Ok(())
    }

    pub async fn create(&self, req: &CreateIssue) -> Result<String> {
        let list = req
            .container
            .as_deref()
            .ok_or_else(|| Error::invalid("missing_list"))?;
        let v = self
            .http
            .post("/1/cards", &json!({ "idList": valid_id(list)?, "name": req.summary, "desc": req.description_md }))
            .await?;
        s(&v, "shortLink").ok_or_else(|| Error::Other("remote_bad_json".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_card() {
        let v = json!({
            "id": "5f1", "shortLink": "AbC123", "shortUrl": "https://trello.com/c/AbC123",
            "name": "Write docs", "desc": "**md** body", "idList": "L2", "idBoard": "B1",
            "labels": [ { "name": "docs" }, { "name": "" } ],
            "members": [ { "id": "m1", "fullName": "Ana" } ],
            "attachments": [ { "id": "a1", "name": "x.png", "isUpload": true, "mimeType": "image/png", "url": "https://trello.com/1/cards/5f1/attachments/a1/download/x.png" }, { "id": "a2", "isUpload": false } ]
        });
        let i = map_card(&v, &|id| (id == "L2").then(|| "Doing".to_string()));
        assert_eq!(i.key, "AbC123");
        assert_eq!(i.status, "Doing");
        assert_eq!(i.status_category, StatusCategory::InProgress);
        assert_eq!(i.labels, vec!["docs"]);
        assert_eq!(i.assignee.unwrap().name, "Ana");
        assert_eq!(i.attachments.len(), 1);
        assert_eq!(i.container.as_deref(), Some("B1"));
    }

    #[test]
    fn ids_validated() {
        assert!(valid_id("AbC123").is_ok());
        assert!(valid_id("a/../b").is_err());
    }
}
