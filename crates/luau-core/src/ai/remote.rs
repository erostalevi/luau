//! Remote AI services: Anthropic (Messages API) and the OpenAI-compatible
//! clouds (OpenAI, Gemini, OpenRouter). Only reached through [`super::llm`],
//! which checks the user's consent and key first.
//!
//! Keys travel only in request headers to the fixed HTTPS bases in
//! [`Provider::remote_base`]; they are never logged, and errors carry the
//! HTTP status and the service's short message, never the request.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde_json::{Value, json};

use super::llm::{ApiKey, MAX_OUTPUT_CHARS, Provider, Resolved};
use super::prompt::Message;
use crate::error::{Error, Result};

const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Server-side refusal fallback (routes a declined request to another model).
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Default answer budget; streaming keeps long answers safe from timeouts.
const MAX_TOKENS: u32 = 16_000;
const MODELS_TTL: Duration = Duration::from_secs(600);

/// Default Claude model (current flagship).
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";

fn client(read_timeout: Duration) -> Result<reqwest::Client> {
    // Unlike the local client, remote calls honour the system proxy.
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(read_timeout)
        .https_only(true)
        .user_agent(concat!("Luau/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Other(format!("http client: {e}")))
}

fn net_err(e: reqwest::Error) -> Error {
    if e.is_timeout() {
        Error::Other("the AI service did not answer in time".into())
    } else if e.is_connect() {
        Error::Other("could not reach the AI service (check your connection)".into())
    } else {
        Error::Other(format!("AI service request failed: {}", e.without_url()))
    }
}

/// Human message for an HTTP error from a service (`body` may carry a short reason).
pub fn status_error(status: u16, body: &str) -> Error {
    let reason = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/0/error/message"))
                .or_else(|| v.get("message"))
                .and_then(Value::as_str)
                .map(|s| s.chars().take(200).collect::<String>())
        })
        .unwrap_or_default();
    let what = match status {
        401 | 403 => "the API key was rejected".to_string(),
        402 => "the account has no credit left".to_string(),
        404 => "the model was not found".to_string(),
        429 => "rate limited, try again in a moment".to_string(),
        s if s >= 500 => format!("the AI service had a problem (HTTP {s})"),
        s => format!("the AI service answered HTTP {s}"),
    };
    if reason.is_empty() {
        Error::Other(what)
    } else {
        Error::Other(format!("{what}: {reason}"))
    }
}

fn authed(p: Provider, rb: reqwest::RequestBuilder, key: &ApiKey) -> reqwest::RequestBuilder {
    match p {
        Provider::Anthropic => rb
            .header("x-api-key", &key.0)
            .header("anthropic-version", ANTHROPIC_VERSION),
        Provider::OpenRouter => rb
            .bearer_auth(&key.0)
            .header("X-Title", "Luau")
            .header("HTTP-Referer", "https://luau.app"),
        _ => rb.bearer_auth(&key.0),
    }
}

fn base(p: Provider) -> Result<&'static str> {
    p.remote_base()
        .ok_or_else(|| Error::invalid("not a remote AI service"))
}

// --- models -------------------------------------------------------------------

static MODELS: LazyLock<Mutex<HashMap<&'static str, (Instant, Vec<String>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Forget cached model lists (after a key change).
pub fn forget_models(p: Provider) {
    MODELS.lock().remove(p.as_str());
}

pub fn parse_models(p: Provider, v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .get("data")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .map(|id| id.strip_prefix("models/").unwrap_or(id).to_string())
                .collect()
        })
        .unwrap_or_default();
    if p != Provider::OpenRouter {
        out.retain(|m| is_chat_model(m));
    }
    out
}

fn is_chat_model(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    ![
        "embed",
        "audio",
        "realtime",
        "tts",
        "transcribe",
        "whisper",
        "dall-e",
        "image",
        "moderation",
        "search",
        "davinci",
        "babbage",
        "aqa",
        "imagen",
        "veo",
        "live",
        "computer-use",
    ]
    .iter()
    .any(|x| l.contains(x))
}

/// Models the account can use (cached for a few minutes).
pub async fn list_models(p: Provider, key: &ApiKey) -> Result<Vec<String>> {
    if let Some((at, list)) = MODELS.lock().get(p.as_str())
        && at.elapsed() < MODELS_TTL
    {
        return Ok(list.clone());
    }
    let url = match p {
        Provider::Anthropic => format!("{}/v1/models?limit=1000", base(p)?),
        _ => format!("{}/models", base(p)?),
    };
    let c = client(Duration::from_secs(15))?;
    let r = authed(p, c.get(&url), key)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(net_err)?;
    let status = r.status().as_u16();
    let text = r.text().await.map_err(net_err)?;
    if !(200..300).contains(&status) {
        return Err(status_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| Error::Other("unexpected answer from the AI service".into()))?;
    let list = parse_models(p, &v);
    MODELS
        .lock()
        .insert(p.as_str(), (Instant::now(), list.clone()));
    Ok(list)
}

/// The model to use when none is picked.
pub fn choose_model(p: Provider, models: &[String]) -> Option<String> {
    let pick = |pred: &dyn Fn(&str) -> bool| models.iter().filter(|m| pred(m)).max().cloned();
    match p {
        Provider::Anthropic => Some(ANTHROPIC_DEFAULT_MODEL.into()),
        Provider::OpenRouter => {
            let want = format!("anthropic/{ANTHROPIC_DEFAULT_MODEL}");
            models
                .iter()
                .find(|m| **m == want)
                .cloned()
                .or_else(|| pick(&|m| m.starts_with("anthropic/claude")))
                .or_else(|| models.first().cloned())
        }
        // Newest general model by name (dated snapshots sort after their base).
        Provider::ChatGpt => {
            pick(&|m| m.starts_with("gpt-") && !m.contains("instruct") && !m.contains("nano"))
                .or_else(|| pick(&|_| true))
        }
        Provider::Gemini => pick(&|m| m.starts_with("gemini-") && m.contains("pro"))
            .or_else(|| pick(&|m| m.starts_with("gemini-"))),
        _ => None,
    }
}

// --- schemas ------------------------------------------------------------------

/// Make a JSON Schema acceptable to strict structured outputs: drop numeric /
/// length / count constraints (validated by our own parsers anyway), close
/// every object and require all of its properties.
pub fn strict_schema(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = serde_json::Map::new();
            for (k, x) in m {
                if matches!(
                    k.as_str(),
                    "minimum"
                        | "maximum"
                        | "exclusiveMinimum"
                        | "exclusiveMaximum"
                        | "multipleOf"
                        | "minLength"
                        | "maxLength"
                        | "minItems"
                        | "maxItems"
                        | "uniqueItems"
                        | "pattern"
                        | "x-order"
                ) {
                    continue;
                }
                out.insert(k.clone(), strict_schema(x));
            }
            if out.get("type").and_then(Value::as_str) == Some("object")
                || out.contains_key("properties")
            {
                out.insert("additionalProperties".into(), Value::Bool(false));
                if let Some(Value::Object(props)) = out.get("properties") {
                    let req: Vec<Value> = props.keys().map(|k| Value::String(k.clone())).collect();
                    out.insert("required".into(), Value::Array(req));
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(strict_schema).collect()),
        x => x.clone(),
    }
}

// --- Anthropic --------------------------------------------------------------

/// Models that take the server-side refusal fallback.
fn takes_fallbacks(model: &str) -> bool {
    [
        "claude-fable-5-1",
        "claude-opus-5-5",
        "claude-opus-5",
        "claude-sonnet-5-5",
    ]
    .contains(&model)
}

pub fn anthropic_body(
    model: &str,
    messages: &[Message],
    max_tokens: u32,
    stream: bool,
    schema: Option<&Value>,
) -> Value {
    let system = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let msgs: Vec<Value> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    let mut b =
        json!({ "model": model, "max_tokens": max_tokens, "messages": msgs, "stream": stream });
    if !system.is_empty() {
        b["system"] = Value::String(system);
    }
    if let Some(s) = schema {
        b["output_config"] =
            json!({ "format": { "type": "json_schema", "schema": strict_schema(s) } });
    }
    if takes_fallbacks(model) {
        b["fallbacks"] = json!("default");
    }
    b
}

#[derive(Debug, PartialEq)]
pub enum AnthropicEvent {
    Text(String),
    Done,
    Refused,
    Error(String),
}

/// One `data:` line of the Messages SSE stream.
pub fn parse_anthropic_line(line: &str) -> Option<AnthropicEvent> {
    let data = line.trim().strip_prefix("data:")?.trim();
    let v: Value = serde_json::from_str(data).ok()?;
    match v.get("type").and_then(Value::as_str)? {
        "content_block_delta" => {
            let d = v.get("delta")?;
            (d.get("type").and_then(Value::as_str) == Some("text_delta")).then(|| {
                AnthropicEvent::Text(
                    d.get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )
            })
        }
        "message_delta" => (v.pointer("/delta/stop_reason").and_then(Value::as_str)
            == Some("refusal"))
        .then_some(AnthropicEvent::Refused),
        "message_stop" => Some(AnthropicEvent::Done),
        "error" => Some(AnthropicEvent::Error(
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .chars()
                .take(200)
                .collect(),
        )),
        _ => None,
    }
}

const REFUSED: &str = "the AI service declined this request";

fn post(
    r: &Resolved,
    c: &reqwest::Client,
    url: &str,
    body: &Value,
) -> Result<reqwest::RequestBuilder> {
    let key = r
        .key
        .as_ref()
        .ok_or_else(|| Error::invalid("no API key for this AI service"))?;
    let mut rb = authed(r.provider, c.post(url), key).json(body);
    if r.provider == Provider::Anthropic && body.get("fallbacks").is_some() {
        rb = rb.header("anthropic-beta", FALLBACK_BETA);
    }
    Ok(rb)
}

async fn checked(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(resp);
    }
    let text = resp.text().await.unwrap_or_default();
    Err(status_error(status, &text))
}

fn chat_url(r: &Resolved) -> Result<String> {
    Ok(match r.provider {
        Provider::Anthropic => format!("{}/v1/messages", base(r.provider)?),
        p => format!("{}/chat/completions", base(p)?),
    })
}

/// Streaming chat. Returns the full text.
pub async fn chat(
    r: &Resolved,
    messages: &[Message],
    max_tokens: Option<u32>,
    on_chunk: &mut (dyn FnMut(&str) + Send),
) -> Result<String> {
    let max = max_tokens.unwrap_or(MAX_TOKENS);
    let body = match r.provider {
        Provider::Anthropic => anthropic_body(&r.model, messages, max, true, None),
        // No temperature: several current cloud models only accept the default.
        _ => json!({ "model": r.model, "messages": messages, "stream": true, "max_tokens": max }),
    };
    let c = client(Duration::from_secs(120))?;
    let resp = checked(
        post(r, &c, &chat_url(r)?, &body)?
            .send()
            .await
            .map_err(net_err)?,
    )
    .await?;
    let mut out = String::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    let mut handle = |line: &str, out: &mut String| -> Result<bool> {
        if r.provider == Provider::Anthropic {
            match parse_anthropic_line(line) {
                Some(AnthropicEvent::Text(t)) => {
                    out.push_str(&t);
                    on_chunk(&t);
                }
                Some(AnthropicEvent::Done) => return Ok(true),
                Some(AnthropicEvent::Refused) if out.is_empty() => {
                    return Err(Error::Other(REFUSED.into()));
                }
                Some(AnthropicEvent::Refused) => return Ok(true),
                Some(AnthropicEvent::Error(e)) => {
                    return Err(Error::Other(format!("AI service error: {e}")));
                }
                None => {}
            }
        } else if let Some((t, done)) = super::llm::parse_stream_line(Provider::OpenAi, line) {
            if !t.is_empty() {
                out.push_str(&t);
                on_chunk(&t);
            }
            if done {
                return Ok(true);
            }
        }
        Ok(out.len() > MAX_OUTPUT_CHARS)
    };
    'outer: while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.map_err(net_err)?);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            if handle(&String::from_utf8_lossy(&line), &mut out)? {
                break 'outer;
            }
        }
    }
    if !buf.is_empty() {
        handle(&String::from_utf8_lossy(&buf), &mut out)?;
    }
    Ok(out)
}

/// Chat constrained to a JSON Schema; returns the raw JSON text (untrusted).
pub async fn chat_json(r: &Resolved, messages: &[Message], schema: &Value) -> Result<String> {
    let body = match r.provider {
        Provider::Anthropic => anthropic_body(&r.model, messages, MAX_TOKENS, false, Some(schema)),
        _ => json!({ "model": r.model, "messages": messages, "stream": false,
                     "response_format": { "type": "json_schema", "json_schema": { "name": "result", "strict": true, "schema": strict_schema(schema) } } }),
    };
    let c = client(Duration::from_secs(180))?;
    let resp = checked(
        post(r, &c, &chat_url(r)?, &body)?
            .send()
            .await
            .map_err(net_err)?,
    )
    .await?;
    let v: Value = resp.json().await.map_err(net_err)?;
    let text = if r.provider == Provider::Anthropic {
        if v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
            return Err(Error::Other(REFUSED.into()));
        }
        v.get("content")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
            .unwrap_or_default()
    } else {
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Ok(text.chars().take(MAX_OUTPUT_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_stream_lines() {
        assert_eq!(
            parse_anthropic_line(
                r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#
            ),
            Some(AnthropicEvent::Text("Hi".into()))
        );
        assert_eq!(
            parse_anthropic_line(
                r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"x"}}"#
            ),
            None
        );
        assert_eq!(
            parse_anthropic_line(r#"data: {"type":"message_stop"}"#),
            Some(AnthropicEvent::Done)
        );
        assert_eq!(
            parse_anthropic_line(
                r#"data: {"type":"message_delta","delta":{"stop_reason":"refusal"}}"#
            ),
            Some(AnthropicEvent::Refused)
        );
        assert_eq!(parse_anthropic_line("event: ping"), None);
    }

    #[test]
    fn anthropic_body_shape() {
        let m = [
            Message {
                role: "system",
                content: "S".into(),
            },
            Message {
                role: "user",
                content: "U".into(),
            },
        ];
        let b = anthropic_body(
            "claude-opus-5-5",
            &m,
            100,
            true,
            Some(&json!({"type":"object","properties":{"a":{"type":"string","maxLength":3}}})),
        );
        assert_eq!(b["system"], "S");
        assert_eq!(b["messages"], json!([{ "role": "user", "content": "U" }]));
        assert_eq!(b["fallbacks"], "default");
        assert!(b.get("temperature").is_none());
        let s = &b["output_config"]["format"]["schema"];
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"], json!(["a"]));
        assert!(s["properties"]["a"].get("maxLength").is_none());
        assert!(
            anthropic_body("claude-haiku-4-5", &m, 1, false, None)
                .get("fallbacks")
                .is_none()
        );
    }

    #[test]
    fn model_choice() {
        let oa = parse_models(
            Provider::ChatGpt,
            &json!({"data":[{"id":"gpt-4o"},{"id":"text-embedding-3-small"},{"id":"gpt-5"},{"id":"gpt-4o-realtime"}]}),
        );
        assert_eq!(oa, vec!["gpt-4o", "gpt-5"]);
        assert_eq!(
            choose_model(Provider::ChatGpt, &oa).as_deref(),
            Some("gpt-5")
        );
        let g = parse_models(
            Provider::Gemini,
            &json!({"data":[{"id":"models/gemini-2.5-flash"},{"id":"models/gemini-2.5-pro"}]}),
        );
        assert_eq!(
            choose_model(Provider::Gemini, &g).as_deref(),
            Some("gemini-2.5-pro")
        );
        assert_eq!(
            choose_model(Provider::Anthropic, &[]).as_deref(),
            Some(ANTHROPIC_DEFAULT_MODEL)
        );
        let or = vec![
            "openai/gpt-5".to_string(),
            "anthropic/claude-sonnet-5-5".to_string(),
        ];
        assert_eq!(
            choose_model(Provider::OpenRouter, &or).as_deref(),
            Some("anthropic/claude-sonnet-5-5")
        );
    }

    #[test]
    fn errors_name_the_problem_not_the_request() {
        let e = status_error(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        );
        assert!(e.to_string().contains("API key was rejected"));
        assert!(status_error(429, "").to_string().contains("rate limited"));
    }
}
