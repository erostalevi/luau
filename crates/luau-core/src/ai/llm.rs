//! LLM client for three kinds of source:
//! - **native**: Apple's on-device model (through [`super::apple`]);
//! - **local**: Ollama (`/api/chat`, `/api/tags`, `/api/pull`) and any
//!   OpenAI-compatible server (`/v1/chat/completions`, `/v1/models`: LM Studio,
//!   llama.cpp, vLLM, LocalAI…). Localhost by default; plain HTTP is accepted
//!   only for loopback hosts, anything else must be HTTPS;
//! - **remote**: Anthropic (Claude), OpenAI (ChatGPT), Google (Gemini) and
//!   OpenRouter, with an API key from the OS keychain. Remote services are only
//!   used when picked explicitly *and* the user consented (card text leaves the
//!   computer); `auto` never chooses one.
//!
//! Never logs prompts, model output or keys; only hosts and HTTP statuses.

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::apple::{self, AppleAvailability};
use super::prompt::Message;
use crate::error::{Error, Result};

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
/// Other well-known local servers probed in `auto` mode (LM Studio, llama.cpp).
const AUTO_CANDIDATES: &[&str] = &["http://localhost:1234", "http://localhost:8080"];
/// Hard cap on generated text (characters) to bound memory.
pub const MAX_OUTPUT_CHARS: usize = 64 * 1024;
/// Endpoint / model names shown for the Apple on-device provider.
pub const APPLE_ENDPOINT: &str = "apple:on-device";
pub const APPLE_MODEL: &str = "Apple on-device";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    #[default]
    Auto,
    /// Apple's on-device model (FoundationModels, macOS 26+).
    Apple,
    Ollama,
    /// Local OpenAI-compatible server (LM Studio, llama.cpp…).
    #[serde(rename = "openai")]
    OpenAi,
    /// Remote: Anthropic Messages API.
    Anthropic,
    /// Remote: OpenAI's own API.
    #[serde(rename = "chatgpt")]
    ChatGpt,
    /// Remote: Google Gemini (OpenAI-compatible endpoint).
    Gemini,
    /// Remote: OpenRouter (OpenAI-compatible).
    #[serde(rename = "openrouter")]
    OpenRouter,
    Off,
}

/// The remote services, in the order the settings list them.
pub const REMOTE_PROVIDERS: [Provider; 4] = [
    Provider::Anthropic,
    Provider::ChatGpt,
    Provider::Gemini,
    Provider::OpenRouter,
];

impl Provider {
    pub fn parse(s: &str) -> Provider {
        match s.trim().to_ascii_lowercase().as_str() {
            "apple" | "apple-on-device" | "foundationmodels" => Provider::Apple,
            "ollama" => Provider::Ollama,
            "openai" | "openai-compatible" | "lmstudio" => Provider::OpenAi,
            "anthropic" | "claude" => Provider::Anthropic,
            "chatgpt" | "openai-cloud" => Provider::ChatGpt,
            "gemini" | "google" => Provider::Gemini,
            "openrouter" => Provider::OpenRouter,
            "off" | "none" | "disabled" => Provider::Off,
            _ => Provider::Auto,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Auto => "auto",
            Provider::Apple => "apple",
            Provider::Ollama => "ollama",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::ChatGpt => "chatgpt",
            Provider::Gemini => "gemini",
            Provider::OpenRouter => "openrouter",
            Provider::Off => "off",
        }
    }

    /// Card text leaves this computer.
    pub fn is_remote(self) -> bool {
        REMOTE_PROVIDERS.contains(&self)
    }

    /// Fixed HTTPS base of a remote service (OpenAI-compatible ones end where
    /// `/chat/completions` and `/models` hang off).
    pub fn remote_base(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some("https://api.anthropic.com"),
            Provider::ChatGpt => Some("https://api.openai.com/v1"),
            Provider::Gemini => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
            Provider::OpenRouter => Some("https://openrouter.ai/api/v1"),
            _ => None,
        }
    }
}

/// An API key. `Debug` is redacted so it can never end up in a log line.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(pub String);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

/// Plausible API key: printable ASCII, no spaces, sane length.
pub fn valid_api_key(k: &str) -> bool {
    (16..=512).contains(&k.len()) && k.chars().all(|c| c.is_ascii_graphic())
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiConfig {
    pub provider: Provider,
    pub endpoint: String,
    pub model: Option<String>,
    pub temperature: f32,
    /// Whole-request timeout for a generation.
    pub timeout: Duration,
    /// Model for the remote service (`None` = a sensible default).
    pub remote_model: Option<String>,
    /// Filled by the app from the keychain, never from settings.
    pub remote_key: Option<ApiKey>,
    /// The user agreed to send card text to this remote service.
    pub remote_consent: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            provider: Provider::Auto,
            endpoint: DEFAULT_ENDPOINT.into(),
            model: None,
            temperature: 0.3,
            timeout: Duration::from_secs(180),
            remote_model: None,
            remote_key: None,
            remote_consent: false,
        }
    }
}

impl AiConfig {
    /// Read `ai.*` keys from the flat settings object.
    pub fn from_settings(v: &Value) -> AiConfig {
        let d = AiConfig::default();
        let str_of = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        };
        AiConfig {
            provider: str_of("ai.provider")
                .map(Provider::parse)
                .unwrap_or(d.provider),
            endpoint: str_of("ai.endpoint")
                .unwrap_or(DEFAULT_ENDPOINT)
                .to_string(),
            model: str_of("ai.model").map(str::to_string),
            temperature: v
                .get("ai.temperature")
                .and_then(Value::as_f64)
                .map(|t| t.clamp(0.0, 2.0) as f32)
                .unwrap_or(d.temperature),
            timeout: v
                .get("ai.timeoutSec")
                .and_then(Value::as_u64)
                .map(|s| Duration::from_secs(s.clamp(10, 1800)))
                .unwrap_or(d.timeout),
            remote_model: str_of("ai.remoteModel").map(str::to_string),
            remote_key: None,
            remote_consent: false,
        }
    }
}

/// Validated base URL (no trailing slash, no `/v1` suffix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub base: String,
    /// Not a loopback host: data leaves this computer.
    pub remote: bool,
}

fn is_loopback_host(host: &str) -> bool {
    let h = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".localhost") {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

pub fn parse_endpoint(raw: &str) -> Result<Endpoint> {
    let raw = raw.trim();
    let raw = if raw.is_empty() {
        DEFAULT_ENDPOINT
    } else {
        raw
    };
    let u = url::Url::parse(raw).map_err(|_| Error::invalid("ai endpoint is not a valid URL"))?;
    if !u.username().is_empty() || u.password().is_some() {
        return Err(Error::invalid("ai endpoint must not contain credentials"));
    }
    let host = u
        .host_str()
        .ok_or_else(|| Error::invalid("ai endpoint has no host"))?;
    let loopback = is_loopback_host(host);
    match u.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => {
            return Err(Error::invalid(
                "plain http is only allowed for localhost; use https",
            ));
        }
        _ => return Err(Error::invalid("ai endpoint must be http(s)")),
    }
    let mut base = format!("{}://{}", u.scheme(), u.host_str().unwrap_or_default());
    if host.contains(':') && !host.starts_with('[') {
        base = format!("{}://[{}]", u.scheme(), host);
    }
    if let Some(p) = u.port() {
        base.push_str(&format!(":{p}"));
    }
    let path = u.path().trim_end_matches('/');
    let path = path.strip_suffix("/v1").unwrap_or(path);
    base.push_str(path);
    Ok(Endpoint {
        base,
        remote: !loopback,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// `apple` | `ollama` | `openai` | `off` | `none` (nothing reachable).
    pub provider: String,
    pub available: bool,
    pub endpoint: String,
    pub model: Option<String>,
    pub models: Vec<String>,
    #[serde(default)]
    pub remote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Apple on-device model availability (macOS; `missing` elsewhere).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apple: Option<AppleAvailability>,
    /// Context window of the chosen model in tokens, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u32>,
}

/// A reachable provider with the model to use.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub provider: Provider,
    pub endpoint: Endpoint,
    pub model: String,
    /// Context window in tokens when it is small and known (Apple on-device);
    /// prompts are trimmed to fit it. `None` = no trimming.
    pub context: Option<u32>,
    /// Remote services only.
    pub key: Option<ApiKey>,
}

impl Resolved {
    /// Tokens reserved for the answer.
    pub fn response_tokens(&self) -> Option<u32> {
        self.context.map(response_tokens)
    }
}

/// Answer budget for a context window: a quarter, within 256…2048 tokens.
pub fn response_tokens(context: u32) -> u32 {
    (context / 4).clamp(256, 2048)
}

fn apple_status(a: AppleAvailability) -> AiStatus {
    AiStatus {
        provider: Provider::Apple.as_str().into(),
        available: a.available(),
        endpoint: APPLE_ENDPOINT.into(),
        model: a.available().then(|| APPLE_MODEL.to_string()),
        models: vec![APPLE_MODEL.to_string()],
        remote: false,
        error: (!a.available()).then(|| a.reason().to_string()),
        apple: Some(a),
        context_size: Some(a.context()),
    }
}

fn client(read_timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(read_timeout)
        .user_agent(concat!("Luau/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Other(format!("http client: {e}")))
}

fn http_err(e: reqwest::Error) -> Error {
    // `reqwest::Error` display includes the URL; our URLs carry no secrets.
    if e.is_timeout() {
        Error::Other("the local AI did not answer in time".into())
    } else if e.is_connect() {
        Error::Other("could not connect to the local AI".into())
    } else {
        Error::Other(format!("local AI request failed: {}", e.without_url()))
    }
}

async fn get_json(c: &reqwest::Client, url: &str, timeout: Duration) -> Result<Value> {
    let r = c.get(url).timeout(timeout).send().await.map_err(http_err)?;
    if !r.status().is_success() {
        return Err(Error::Other(format!("HTTP {}", r.status().as_u16())));
    }
    r.json::<Value>().await.map_err(http_err)
}

/// Models served at `ep` by `provider` (Ollama or OpenAI-compatible).
pub async fn list_models(provider: Provider, ep: &Endpoint) -> Result<Vec<ModelInfo>> {
    let c = client(Duration::from_secs(5))?;
    let t = Duration::from_secs(4);
    match provider {
        Provider::Ollama => {
            let v = get_json(&c, &format!("{}/api/tags", ep.base), t).await?;
            Ok(parse_ollama_models(&v))
        }
        Provider::OpenAi => {
            let v = get_json(&c, &format!("{}/v1/models", ep.base), t).await?;
            Ok(parse_openai_models(&v))
        }
        _ => Err(Error::invalid("provider has no models")),
    }
}

pub fn parse_ollama_models(v: &Value) -> Vec<ModelInfo> {
    v.get("models")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(ModelInfo {
                        name: m
                            .get("name")
                            .or_else(|| m.get("model"))?
                            .as_str()?
                            .to_string(),
                        size: m.get("size").and_then(Value::as_u64),
                        family: m
                            .get("details")
                            .and_then(|d| d.get("family"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_openai_models(v: &Value) -> Vec<ModelInfo> {
    v.get("data")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(ModelInfo {
                        name: m.get("id")?.as_str()?.to_string(),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Pick the configured model when it is served, else the first chat model.
pub fn choose_model(configured: Option<&str>, models: &[ModelInfo]) -> Option<String> {
    if let Some(m) = configured
        && (models.is_empty()
            || models
                .iter()
                .any(|x| x.name == m || x.name.split(':').next() == Some(m)))
    {
        return Some(m.to_string());
    }
    models
        .iter()
        .map(|m| m.name.as_str())
        .find(|n| {
            let l = n.to_ascii_lowercase();
            !(l.contains("embed")
                || l.contains("bge-")
                || l.contains("minilm")
                || l.contains("rerank")
                || l.contains("whisper"))
        })
        .map(str::to_string)
}

/// Probe one endpoint for a provider; returns the provider found and its models.
async fn probe(provider: Provider, ep: &Endpoint) -> Option<(Provider, Vec<ModelInfo>)> {
    let order: &[Provider] = match provider {
        Provider::Ollama => &[Provider::Ollama],
        Provider::OpenAi => &[Provider::OpenAi],
        _ => &[Provider::Ollama, Provider::OpenAi],
    };
    for p in order {
        if let Ok(m) = list_models(*p, ep).await {
            return Some((*p, m));
        }
    }
    None
}

/// In `auto` mode the Apple model goes first unless the user picked a model
/// name (which only an Ollama / OpenAI-compatible server can serve).
pub fn apple_first(cfg: &AiConfig) -> bool {
    match cfg.provider {
        Provider::Apple => true,
        Provider::Auto => cfg.model.is_none(),
        _ => false,
    }
}

/// Find a reachable provider according to the config. `auto` prefers the
/// Apple on-device model, then a running Ollama / LM Studio / llama.cpp.
pub async fn status(cfg: &AiConfig) -> AiStatus {
    if cfg.provider.is_remote() {
        return remote_status(cfg);
    }
    if cfg.provider == Provider::Off {
        return AiStatus {
            endpoint: cfg.endpoint.clone(),
            provider: "off".into(),
            ..Default::default()
        };
    }
    let apple = match cfg.provider {
        Provider::Apple | Provider::Auto => Some(apple::availability().await),
        _ => None,
    };
    if let Some(a) = apple
        && (cfg.provider == Provider::Apple || (a.available() && apple_first(cfg)))
    {
        return apple_status(a);
    }
    let mut st = http_status(cfg).await;
    if let Some(a) = apple {
        if !st.available && a.available() {
            return apple_status(a);
        }
        st.apple = Some(a);
    }
    st
}

/// A remote service is ready once it has a key and the user's consent; this
/// does not touch the network (use the connection test for that).
pub fn remote_status(cfg: &AiConfig) -> AiStatus {
    let base = cfg.provider.remote_base().unwrap_or_default();
    let error = if !cfg.remote_consent {
        Some("consent needed before card text is sent to this service".to_string())
    } else if cfg.remote_key.is_none() {
        Some("no API key".to_string())
    } else {
        None
    };
    AiStatus {
        provider: cfg.provider.as_str().into(),
        available: error.is_none(),
        endpoint: base.into(),
        model: cfg
            .remote_model
            .clone()
            .or_else(|| super::remote::choose_model(cfg.provider, &[])),
        models: vec![],
        remote: true,
        error,
        apple: None,
        context_size: None,
    }
}

async fn http_status(cfg: &AiConfig) -> AiStatus {
    let mut st = AiStatus {
        endpoint: cfg.endpoint.clone(),
        provider: "none".into(),
        ..Default::default()
    };
    let ep = match parse_endpoint(&cfg.endpoint) {
        Ok(e) => e,
        Err(e) => {
            st.error = Some(e.to_string());
            return st;
        }
    };
    let mut candidates = vec![ep.clone()];
    if cfg.provider == Provider::Auto && ep.base == DEFAULT_ENDPOINT {
        candidates.extend(
            AUTO_CANDIDATES
                .iter()
                .filter_map(|c| parse_endpoint(c).ok()),
        );
    }
    for cand in candidates {
        if let Some((p, models)) = probe(cfg.provider, &cand).await {
            st.provider = p.as_str().into();
            st.endpoint = cand.base.clone();
            st.remote = cand.remote;
            st.models = models.iter().map(|m| m.name.clone()).collect();
            st.model = choose_model(cfg.model.as_deref(), &models);
            st.available = st.model.is_some();
            if !st.available {
                st.error = Some("no models installed".into());
            }
            return st;
        }
    }
    st.remote = ep.remote;
    st.error = Some("no local AI server found".into());
    st
}

pub async fn resolve(cfg: &AiConfig) -> Result<Resolved> {
    if cfg.provider.is_remote() {
        let st = remote_status(cfg);
        if let Some(e) = st.error {
            return Err(Error::Other(e));
        }
        let key = cfg.remote_key.clone().expect("checked by remote_status");
        let model = match &cfg.remote_model {
            Some(m) => m.clone(),
            None => {
                let models = super::remote::list_models(cfg.provider, &key).await?;
                super::remote::choose_model(cfg.provider, &models).ok_or_else(|| {
                    Error::Other("no chat model available for this account".into())
                })?
            }
        };
        return Ok(Resolved {
            provider: cfg.provider,
            endpoint: Endpoint {
                base: st.endpoint,
                remote: true,
            },
            model,
            context: None,
            key: Some(key),
        });
    }
    let st = status(cfg).await;
    if !st.available {
        return Err(Error::Other(
            st.error.unwrap_or_else(|| "local AI unavailable".into()),
        ));
    }
    let provider = Provider::parse(&st.provider);
    if provider == Provider::Apple {
        return Ok(Resolved {
            provider,
            endpoint: Endpoint {
                base: APPLE_ENDPOINT.into(),
                remote: false,
            },
            model: APPLE_MODEL.into(),
            context: st.context_size,
            key: None,
        });
    }
    Ok(Resolved {
        provider,
        endpoint: parse_endpoint(&st.endpoint)?,
        model: st.model.unwrap_or_default(),
        context: None,
        key: None,
    })
}

/// Split chat messages into (instructions, prompt) for single-turn models.
pub fn split_messages(messages: &[Message]) -> (String, String) {
    let join = |sys: bool| {
        messages
            .iter()
            .filter(|m| (m.role == "system") == sys)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    (join(true), join(false))
}

async fn apple_chat(
    r: &Resolved,
    messages: &[Message],
    temperature: f32,
    timeout: Duration,
    schema: Option<&Value>,
    max_tokens: Option<u32>,
    on_chunk: &mut (dyn FnMut(&str) + Send),
) -> Result<String> {
    let (instructions, prompt) = split_messages(messages);
    let cap = r
        .response_tokens()
        .unwrap_or(response_tokens(apple::DEFAULT_CONTEXT));
    let req = apple::AppleRequest {
        instructions: &instructions,
        prompt: &prompt,
        max_tokens: max_tokens.map_or(cap, |m| m.min(cap)),
        temperature,
        schema,
        stream: schema.is_none(),
    };
    apple::generate(&req, timeout, MAX_OUTPUT_CHARS, on_chunk).await
}

/// Run a chat completion constrained to a JSON Schema and return the raw JSON
/// text (still untrusted: callers must validate it). Apple uses guided
/// generation; Ollama `format`; OpenAI-compatible `response_format`.
pub async fn chat_json(
    r: &Resolved,
    messages: &[Message],
    schema: &Value,
    temperature: f32,
    timeout: Duration,
) -> Result<String> {
    if r.provider.is_remote() {
        return tokio::time::timeout(timeout, super::remote::chat_json(r, messages, schema))
            .await
            .map_err(|_| Error::Other("the AI service did not answer in time".into()))?;
    }
    if r.provider == Provider::Apple {
        return apple_chat(
            r,
            messages,
            temperature,
            timeout,
            Some(schema),
            None,
            &mut |_| {},
        )
        .await;
    }
    let c = client(Duration::from_secs(90))?;
    let (url, body) = match r.provider {
        Provider::Ollama => (
            format!("{}/api/chat", r.endpoint.base),
            json!({ "model": r.model, "messages": messages, "stream": false, "format": schema, "options": { "temperature": temperature } }),
        ),
        _ => (
            format!("{}/v1/chat/completions", r.endpoint.base),
            json!({ "model": r.model, "messages": messages, "stream": false, "temperature": temperature,
                    "response_format": { "type": "json_schema", "json_schema": { "name": "cards", "strict": true, "schema": schema } } }),
        ),
    };
    let fut = async {
        let resp = c.post(&url).json(&body).send().await.map_err(http_err)?;
        if !resp.status().is_success() {
            return Err(Error::Other(format!(
                "local AI answered HTTP {}",
                resp.status().as_u16()
            )));
        }
        let v: Value = resp.json().await.map_err(http_err)?;
        let text = match r.provider {
            Provider::Ollama => v.pointer("/message/content"),
            _ => v.pointer("/choices/0/message/content"),
        }
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(MAX_OUTPUT_CHARS)
        .collect::<String>();
        Ok(text)
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| Error::Other("the local AI did not answer in time".into()))?
}

/// Extract the streamed text delta from one line of a streaming response.
/// Returns `(text, done)`.
pub fn parse_stream_line(provider: Provider, line: &str) -> Option<(String, bool)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match provider {
        Provider::Ollama => {
            let v: Value = serde_json::from_str(line).ok()?;
            let text = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some((
                text,
                v.get("done").and_then(Value::as_bool).unwrap_or(false),
            ))
        }
        _ => {
            let data = line.strip_prefix("data:")?.trim();
            if data == "[DONE]" {
                return Some((String::new(), true));
            }
            let v: Value = serde_json::from_str(data).ok()?;
            let c = v.get("choices")?.get(0)?;
            let text = c
                .get("delta")
                .and_then(|d| d.get("content"))
                .or_else(|| c.get("message").and_then(|m| m.get("content")))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let done = c.get("finish_reason").is_some_and(|f| !f.is_null());
            Some((text, done))
        }
    }
}

/// Run a chat completion, streaming text deltas to `on_chunk`. Returns the full text.
/// `max_tokens` caps the answer where the provider supports it (Apple).
pub async fn chat(
    r: &Resolved,
    messages: &[Message],
    temperature: f32,
    timeout: Duration,
    max_tokens: Option<u32>,
    on_chunk: &mut (dyn FnMut(&str) + Send),
) -> Result<String> {
    if r.provider.is_remote() {
        return tokio::time::timeout(
            timeout,
            super::remote::chat(r, messages, max_tokens, on_chunk),
        )
        .await
        .map_err(|_| Error::Other("the AI service did not answer in time".into()))?;
    }
    if r.provider == Provider::Apple {
        return apple_chat(
            r,
            messages,
            temperature,
            timeout,
            None,
            max_tokens,
            on_chunk,
        )
        .await;
    }
    let c = client(Duration::from_secs(90))?;
    let (url, body) = match r.provider {
        Provider::Ollama => (
            format!("{}/api/chat", r.endpoint.base),
            json!({ "model": r.model, "messages": messages, "stream": true, "options": { "temperature": temperature } }),
        ),
        _ => (
            format!("{}/v1/chat/completions", r.endpoint.base),
            json!({ "model": r.model, "messages": messages, "stream": true, "temperature": temperature }),
        ),
    };
    let fut = async {
        let resp = c.post(&url).json(&body).send().await.map_err(http_err)?;
        if !resp.status().is_success() {
            return Err(Error::Other(format!(
                "local AI answered HTTP {}",
                resp.status().as_u16()
            )));
        }
        let mut out = String::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        'outer: while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(http_err)?;
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line);
                if let Some((text, done)) = parse_stream_line(r.provider, &line) {
                    if !text.is_empty() {
                        out.push_str(&text);
                        on_chunk(&text);
                    }
                    if done || out.len() > MAX_OUTPUT_CHARS {
                        break 'outer;
                    }
                }
            }
        }
        if !buf.is_empty()
            && let Some((text, _)) = parse_stream_line(r.provider, &String::from_utf8_lossy(&buf))
        {
            out.push_str(&text);
            on_chunk(&text);
        }
        Ok(out)
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| Error::Other("the local AI did not answer in time".into()))?
}

/// Ollama model names: `name[:tag]`, optionally `namespace/name`.
pub fn valid_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['-', '.', '/'])
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub name: String,
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub done: bool,
}

/// Pull (download) an Ollama model, reporting progress.
pub async fn pull_model(
    ep: &Endpoint,
    name: &str,
    on_progress: &mut (dyn FnMut(PullProgress) + Send),
) -> Result<()> {
    if !valid_model_name(name) {
        return Err(Error::invalid("invalid model name"));
    }
    let c = client(Duration::from_secs(120))?;
    let resp = c
        .post(format!("{}/api/pull", ep.base))
        .json(&json!({ "model": name, "name": name, "stream": true }))
        .send()
        .await
        .map_err(http_err)?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "pull failed: HTTP {}",
            resp.status().as_u16()
        )));
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk.map_err(http_err)?);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let Ok(v) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if let Some(e) = v.get("error").and_then(Value::as_str) {
                return Err(Error::Other(format!(
                    "pull failed: {}",
                    e.chars().take(200).collect::<String>()
                )));
            }
            let status = v
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let done = status == "success";
            on_progress(PullProgress {
                name: name.to_string(),
                status,
                completed: v.get("completed").and_then(Value::as_u64),
                total: v.get("total").and_then(Value::as_u64),
                done,
            });
            if done {
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_validated() {
        let e = parse_endpoint("http://localhost:11434/").unwrap();
        assert_eq!(e.base, "http://localhost:11434");
        assert!(!e.remote);
        assert_eq!(
            parse_endpoint("http://127.0.0.1:1234/v1").unwrap().base,
            "http://127.0.0.1:1234"
        );
        assert_eq!(
            parse_endpoint("http://[::1]:8080").unwrap().base,
            "http://[::1]:8080"
        );
        assert!(parse_endpoint("https://ai.example.com").unwrap().remote);
        assert!(parse_endpoint("http://ai.example.com").is_err());
        assert!(parse_endpoint("http://user:pw@localhost:1").is_err());
        assert!(parse_endpoint("file:///etc/passwd").is_err());
        assert_eq!(parse_endpoint("").unwrap().base, DEFAULT_ENDPOINT);
    }

    #[test]
    fn config_from_settings() {
        let c = AiConfig::from_settings(
            &json!({"ai.provider": "openai", "ai.model": " llama3 ", "ai.temperature": 9.0, "ai.timeoutSec": 1}),
        );
        assert_eq!(c.provider, Provider::OpenAi);
        assert_eq!(c.model.as_deref(), Some("llama3"));
        assert_eq!(c.temperature, 2.0);
        assert_eq!(c.timeout, Duration::from_secs(10));
        assert_eq!(AiConfig::from_settings(&json!({})), AiConfig::default());
    }

    #[test]
    fn models_and_choice() {
        let o = parse_ollama_models(
            &json!({"models": [{"name": "nomic-embed-text:latest"}, {"name": "llama3.2:3b", "size": 5, "details": {"family": "llama"}}]}),
        );
        assert_eq!(o.len(), 2);
        assert_eq!(choose_model(None, &o).as_deref(), Some("llama3.2:3b"));
        assert_eq!(
            choose_model(Some("llama3.2"), &o).as_deref(),
            Some("llama3.2")
        );
        assert_eq!(
            choose_model(Some("missing"), &o).as_deref(),
            Some("llama3.2:3b")
        );
        let oa = parse_openai_models(&json!({"data": [{"id": "qwen2.5-7b-instruct"}]}));
        assert_eq!(
            choose_model(None, &oa).as_deref(),
            Some("qwen2.5-7b-instruct")
        );
        assert_eq!(choose_model(None, &[]), None);
    }

    #[test]
    fn stream_lines() {
        assert_eq!(
            parse_stream_line(
                Provider::Ollama,
                r#"{"message":{"role":"assistant","content":"Hi"},"done":false}"#
            ),
            Some(("Hi".into(), false))
        );
        assert_eq!(
            parse_stream_line(Provider::Ollama, r#"{"done":true}"#),
            Some(("".into(), true))
        );
        assert_eq!(
            parse_stream_line(
                Provider::OpenAi,
                r#"data: {"choices":[{"delta":{"content":"Yo"},"finish_reason":null}]}"#
            ),
            Some(("Yo".into(), false))
        );
        assert_eq!(
            parse_stream_line(Provider::OpenAi, "data: [DONE]"),
            Some(("".into(), true))
        );
        assert_eq!(parse_stream_line(Provider::OpenAi, ": keep-alive"), None);
    }

    #[test]
    fn apple_provider_and_order() {
        assert_eq!(Provider::parse("apple"), Provider::Apple);
        assert_eq!(Provider::Apple.as_str(), "apple");
        let mut c = AiConfig::default();
        assert!(apple_first(&c), "auto prefers Apple");
        c.model = Some("llama3".into());
        assert!(!apple_first(&c), "a picked model means an HTTP server");
        c.provider = Provider::Apple;
        assert!(apple_first(&c));
        c.provider = Provider::Ollama;
        assert!(!apple_first(&c));
        assert_eq!(response_tokens(4096), 1024);
        assert_eq!(response_tokens(8192), 2048);
        assert_eq!(response_tokens(512), 256);
        let (i, p) = split_messages(&[
            Message {
                role: "system",
                content: "S".into(),
            },
            Message {
                role: "user",
                content: "U1".into(),
            },
            Message {
                role: "user",
                content: "U2".into(),
            },
        ]);
        assert_eq!((i.as_str(), p.as_str()), ("S", "U1\n\nU2"));
    }

    #[test]
    fn remote_needs_consent_and_key_and_is_never_auto() {
        assert!(
            REMOTE_PROVIDERS
                .iter()
                .all(|p| p.is_remote() && p.remote_base().unwrap().starts_with("https://"))
        );
        assert!(!Provider::Auto.is_remote() && !Provider::OpenAi.is_remote());
        assert_eq!(Provider::parse("claude"), Provider::Anthropic);
        assert_eq!(Provider::parse("chatgpt"), Provider::ChatGpt);
        let mut c = AiConfig {
            provider: Provider::Anthropic,
            ..Default::default()
        };
        let st = remote_status(&c);
        assert!(!st.available && st.error.unwrap().contains("consent"));
        c.remote_consent = true;
        assert!(remote_status(&c).error.unwrap().contains("API key"));
        c.remote_key = Some(ApiKey("sk-ant-0123456789abcdef".into()));
        let st = remote_status(&c);
        assert!(st.available && st.remote);
        assert_eq!(st.model.as_deref(), Some("claude-opus-5-5"));
        assert!(
            !format!("{c:?}").contains("0123456789"),
            "keys never show in Debug"
        );
        // Settings can't smuggle a key or consent in.
        let s = AiConfig::from_settings(
            &json!({"ai.provider": "anthropic", "ai.remoteKey": "x", "ai.remoteConsent": true}),
        );
        assert!(s.remote_key.is_none() && !s.remote_consent);
        assert!(
            valid_api_key("sk-ant-api03-abcdefghijk")
                && !valid_api_key("short")
                && !valid_api_key("has space in it ok ok")
        );
    }

    #[test]
    fn model_names() {
        assert!(valid_model_name("llama3.2:3b"));
        assert!(valid_model_name("library/qwen2.5"));
        assert!(!valid_model_name("../etc"));
        assert!(!valid_model_name("a b"));
        assert!(!valid_model_name("-rm"));
    }
}
