//! Slack (bot or user token): connection test, channel list (cursor
//! pagination), post a message, find the newest message that @mentions a
//! member. Message text is never logged.

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

/// A message that mentions the user ("Task from Slack").
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    pub channel: String,
    pub channel_name: String,
    pub ts: String,
    pub author: String,
    /// Markdown (Slack markup converted).
    pub text: String,
    pub permalink: Option<String>,
}

/// Slack mrkdwn → Markdown: `<@U1>` → `@name` (`me` → `@you`), `<#C1|x>` →
/// `#x`, `<url|label>` → `[label](url)`, `<url>` → `url`, `*b*` → `**b**`,
/// HTML entities decoded.
pub fn slack_to_markdown(text: &str, me: &str, names: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let inner = &rest[i + 1..i + j];
        let (target, label) = inner
            .split_once('|')
            .map_or((inner, None), |(a, b)| (a, Some(b)));
        if let Some(uid) = target.strip_prefix('@') {
            if uid == me {
                out.push_str("@you");
            } else {
                out.push('@');
                out.push_str(
                    &label
                        .map(str::to_string)
                        .or_else(|| names(uid))
                        .unwrap_or_else(|| uid.to_string())
                        .replace(' ', ""),
                );
            }
        } else if let Some(chan) = target.strip_prefix('#') {
            out.push('#');
            out.push_str(label.unwrap_or(chan));
        } else if let Some(cmd) = target.strip_prefix('!') {
            out.push('@');
            out.push_str(label.unwrap_or(cmd.split('^').next().unwrap_or(cmd)));
        } else if target.starts_with("http") || target.starts_with("mailto:") {
            match label {
                Some(l) => out.push_str(&format!("[{l}]({target})")),
                None => out.push_str(target),
            }
        } else {
            out.push_str(inner);
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    // *bold* (Slack) → **bold** (Markdown); leave `**` alone.
    let bold = regex::Regex::new(r"(^|[\s(])\*([^*\n]+)\*([\s).,!?:;]|$)").expect("regex");
    let out = bold.replace_all(&out, "$1**$2**$3").into_owned();
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The newest message among `messages` (conversations.history items) that mentions `me`.
pub fn newest_mention<'a>(messages: &'a [Value], me: &str) -> Option<&'a Value> {
    let tag = format!("<@{me}");
    messages
        .iter()
        .filter(|m| {
            m.get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains(&tag))
        })
        .filter(|m| m.get("user").and_then(Value::as_str) != Some(me))
        .max_by(|a, b| {
            let ts = |m: &Value| {
                m.get("ts")
                    .and_then(Value::as_str)
                    .and_then(|t| t.parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            ts(a).total_cmp(&ts(b))
        })
}

impl Slack {
    /// Slack member id of the token's owner (`user_id` in auth.test).
    pub async fn whoami(&self) -> Result<String> {
        let v = check(self.http.post("/api/auth.test", &json!({})).await?)?;
        v.get("user_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::Other("slack:no_user".into()))
    }

    async fn user_name(&self, id: &str) -> Option<String> {
        let v = check(
            self.http
                .get("/api/users.info", &[("user", id.to_string())])
                .await
                .ok()?,
        )
        .ok()?;
        let u = v.get("user")?;
        u.pointer("/profile/display_name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .or_else(|| u.pointer("/profile/real_name").and_then(Value::as_str))
            .or_else(|| u.get("name").and_then(Value::as_str))
            .map(str::to_string)
    }

    async fn finish(
        &self,
        me: &str,
        channel: &str,
        channel_name: &str,
        m: &Value,
        permalink: Option<String>,
    ) -> Mention {
        let ts = m
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let author_id = m
            .get("user")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let author = match m.get("username").and_then(Value::as_str) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => self.user_name(&author_id).await.unwrap_or(author_id),
        };
        let permalink = match permalink {
            Some(p) => Some(p),
            None => check(
                self.http
                    .get(
                        "/api/chat.getPermalink",
                        &[("channel", channel.to_string()), ("message_ts", ts.clone())],
                    )
                    .await
                    .unwrap_or_default(),
            )
            .ok()
            .and_then(|v| {
                v.get("permalink")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        };
        let raw = m.get("text").and_then(Value::as_str).unwrap_or_default();
        Mention {
            channel: channel.to_string(),
            channel_name: channel_name.to_string(),
            ts,
            author,
            text: slack_to_markdown(raw, me, &|_| None),
            permalink: permalink.filter(|p| p.starts_with("https://")),
        }
    }

    /// Newest mention via `search.messages` (user tokens with `search:read`).
    pub async fn search_mention(&self, me: &str) -> Result<Option<Mention>> {
        let q = [
            ("query", format!("<@{me}>")),
            ("sort", "timestamp".into()),
            ("sort_dir", "desc".into()),
            ("count", "10".into()),
        ];
        let v = check(self.http.get("/api/search.messages", &q).await?)?;
        let matches: Vec<Value> = v
            .pointer("/messages/matches")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(m) = newest_mention(&matches, me) else {
            return Ok(None);
        };
        let channel = m
            .pointer("/channel/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = m
            .pointer("/channel/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let permalink = m
            .get("permalink")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Some(self.finish(me, &channel, &name, m, permalink).await))
    }

    /// Newest mention by reading recent history of the channels this token is
    /// a member of (bot tokens; needs `channels:history` / `groups:history`).
    pub async fn scan_mention(&self, me: &str, oldest: i64) -> Result<Option<Mention>> {
        let v = check(
            self.http
                .get(
                    "/api/users.conversations",
                    &[
                        ("types", "public_channel,private_channel".into()),
                        ("exclude_archived", "true".into()),
                        ("limit", "200".into()),
                    ],
                )
                .await?,
        )?;
        let chans: Vec<(String, String)> = v
            .get("channels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some((
                    c.get("id")?.as_str()?.to_string(),
                    c.get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ))
            })
            .take(40)
            .collect();
        let mut best: Option<(f64, String, String, Value)> = None;
        for (id, name) in chans {
            let q = [
                ("channel", id.clone()),
                ("oldest", oldest.to_string()),
                ("limit", "200".into()),
            ];
            let Ok(h) = self
                .http
                .get("/api/conversations.history", &q)
                .await
                .and_then(check)
            else {
                continue;
            };
            let msgs: Vec<Value> = h
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(m) = newest_mention(&msgs, me) {
                let ts = m
                    .get("ts")
                    .and_then(Value::as_str)
                    .and_then(|t| t.parse::<f64>().ok())
                    .unwrap_or(0.0);
                if best.as_ref().is_none_or(|b| ts > b.0) {
                    best = Some((ts, id, name, m.clone()));
                }
            }
        }
        match best {
            Some((_, id, name, m)) => Ok(Some(self.finish(me, &id, &name, &m, None).await)),
            None => Ok(None),
        }
    }

    /// Channel id for `#name` (or the id itself).
    pub async fn channel_id(&self, channel: &str) -> Result<String> {
        let Some(name) = channel.strip_prefix('#') else {
            return valid_channel(channel).map(str::to_string);
        };
        self.channels()
            .await?
            .into_iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .map(|c| c.id)
            .ok_or_else(|| Error::not_found(format!("Slack channel #{name}")))
    }

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
    fn slack_markup_to_markdown() {
        let names = |id: &str| (id == "U2").then(|| "Ana Ruiz".to_string());
        assert_eq!(
            slack_to_markdown(
                "<@U1> can you *fix* <https://x.io/a|the login> in <#C9|dev> with <@U2>? &lt;3",
                "U1",
                &names
            ),
            "@you can you **fix** [the login](https://x.io/a) in #dev with @AnaRuiz? <3"
        );
        assert_eq!(
            slack_to_markdown("see <https://x.io> <!here>", "U1", &|_| None),
            "see https://x.io @here"
        );
    }

    #[test]
    fn newest_mention_skips_own_messages() {
        let msgs = vec![
            json!({"ts": "100.1", "user": "U2", "text": "hey <@U1> one"}),
            json!({"ts": "300.1", "user": "U1", "text": "<@U1> note to self"}),
            json!({"ts": "200.5", "user": "U3", "text": "<@U1|me> two"}),
            json!({"ts": "400.0", "user": "U3", "text": "no mention"}),
        ];
        assert_eq!(newest_mention(&msgs, "U1").unwrap()["ts"], "200.5");
        assert!(newest_mention(&msgs, "U9").is_none());
    }

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
