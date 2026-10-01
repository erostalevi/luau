//! Slack (bot token): connection test, channel list (cursor pagination),
//! post a message. Message text is never logged.

use serde_json::{Value, json};

use super::http::Http;
use super::types::IdName;
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct Slack {
    pub http: Http,
}

/// Slack answers HTTP 200 with `{ ok: false, error }` on failures.
pub fn check(v: Value) -> Result<Value> {
    if v.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(v)
    } else {
        let e = v.get("error").and_then(Value::as_str).unwrap_or("unknown");
        let code: String = e
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .take(48)
            .collect();
        Err(Error::Other(format!("slack:{code}")))
    }
}

pub fn next_cursor(v: &Value) -> Option<String> {
    v.pointer("/response_metadata/next_cursor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

pub fn valid_channel(id: &str) -> Result<&str> {
    if !id.is_empty() && id.len() <= 24 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(Error::invalid("invalid_channel"))
    }
}

impl Slack {
    /// `(team, bot user)` for the connection test.
    pub async fn auth_test(&self) -> Result<(String, String)> {
        let v = check(self.http.post("/api/auth.test", &json!({})).await?)?;
        let g = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Ok((g("team"), g("user")))
    }

    pub async fn channels(&self) -> Result<Vec<IdName>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut q = vec![
                ("types", "public_channel,private_channel".to_string()),
                ("exclude_archived", "true".into()),
                ("limit", "200".into()),
            ];
            if let Some(c) = &cursor {
                q.push(("cursor", c.clone()));
            }
            let v = check(self.http.get("/api/conversations.list", &q).await?)?;
            for c in v
                .get("channels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.push(IdName {
                    id: c
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    name: c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    key: None,
                    icon: None,
                    detail: (c.get("is_private").and_then(Value::as_bool) == Some(true))
                        .then(|| "private".into()),
                });
            }
            cursor = next_cursor(&v);
            if cursor.is_none() || out.len() >= 2000 {
                break;
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    pub async fn post(&self, channel: &str, text: &str) -> Result<()> {
        if text.trim().is_empty() || text.len() > 40_000 {
            return Err(Error::invalid("invalid_message"));
        }
        check(
            self.http
                .post(
                    "/api/chat.postMessage",
                    &json!({ "channel": valid_channel(channel)?, "text": text }),
                )
                .await?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_and_errors() {
        assert!(check(json!({ "ok": true })).is_ok());
        let e = check(json!({ "ok": false, "error": "not_in_channel" })).unwrap_err();
        assert_eq!(e.to_string(), "slack:not_in_channel");
        assert!(check(json!({})).is_err());
    }

    #[test]
    fn cursor_pagination() {
        assert_eq!(
            next_cursor(&json!({ "response_metadata": { "next_cursor": "dXNl" } })),
            Some("dXNl".into())
        );
        assert_eq!(
            next_cursor(&json!({ "response_metadata": { "next_cursor": "" } })),
            None
        );
        assert_eq!(next_cursor(&json!({})), None);
    }

    #[test]
    fn channel_ids() {
        assert!(valid_channel("C0123ABC").is_ok());
        assert!(valid_channel("#general").is_err());
    }
}
