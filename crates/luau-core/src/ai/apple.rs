//! Apple on-device model adapter (FoundationModels, macOS 26+ with Apple
//! Intelligence). The model is reached through the bundled `apple-llm` helper
//! (`src-tauri/helpers/apple-llm/main.swift`), one child process per request,
//! speaking JSON over stdin/stdout:
//!
//! - `apple-llm availability` → `{"status": "...", "contextSize": n}`
//! - `apple-llm generate` (request on stdin) → NDJSON `{"delta"}`* then
//!   `{"done": true, "text"}` or `{"error": "<code>"}`.
//!
//! The helper path is set once by the desktop shell ([`set_helper_path`]); it
//! is never taken from settings or the webview. Prompts and answers are never
//! logged; only error codes.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::error::{Error, Result};

/// Message of the error returned when the prompt does not fit the context
/// window; callers may retry with a smaller budget.
pub const ERR_CONTEXT: &str = "the request is too long for the on-device model";
/// Context window assumed when the helper does not report one (macOS 26).
pub const DEFAULT_CONTEXT: u32 = 4096;
/// Requests larger than this are refused before spawning the helper.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

static HELPER: RwLock<Option<PathBuf>> = RwLock::new(None);
static CACHE: Mutex<Option<(Instant, AppleAvailability)>> = Mutex::new(None);

/// Register the helper executable (called by the desktop shell at startup).
pub fn set_helper_path(p: PathBuf) {
    if let Ok(mut g) = HELPER.write() {
        *g = Some(p);
    }
    *CACHE.lock() = None;
}

fn helper_path() -> Option<PathBuf> {
    let p = HELPER.read().ok()?.clone()?;
    p.is_file().then_some(p)
}

/// Availability of the on-device model, as reported by the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AppleState {
    Available,
    /// Apple Intelligence is turned off in System Settings.
    AppleIntelligenceNotEnabled,
    /// Not an Apple Silicon Mac (or not eligible for Apple Intelligence).
    DeviceNotEligible,
    /// The model is still downloading or preparing.
    ModelNotReady,
    /// macOS older than 26.
    UnsupportedOs,
    /// No helper in this build / platform.
    #[default]
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppleAvailability {
    pub status: AppleState,
    /// Context window in tokens (0 when unknown).
    pub context_size: u32,
}

impl AppleAvailability {
    pub fn available(&self) -> bool {
        self.status == AppleState::Available
    }
    /// Context window to plan prompts with.
    pub fn context(&self) -> u32 {
        if self.context_size >= 1024 {
            self.context_size
        } else {
            DEFAULT_CONTEXT
        }
    }
    /// Short, user-facing reason when unavailable.
    pub fn reason(&self) -> &'static str {
        match self.status {
            AppleState::Available => "",
            AppleState::AppleIntelligenceNotEnabled => "Apple Intelligence is turned off",
            AppleState::DeviceNotEligible => "this Mac cannot run Apple Intelligence",
            AppleState::ModelNotReady => "the Apple on-device model is still being prepared",
            AppleState::UnsupportedOs => "the Apple on-device model needs macOS 26 or later",
            AppleState::Missing => "the Apple on-device model is not available in this build",
            AppleState::Unknown => "the Apple on-device model is not available",
        }
    }
}

/// Parse the helper's `availability` answer (unknown values are tolerated).
pub fn parse_availability(s: &str) -> AppleAvailability {
    let v: Value = serde_json::from_str(s.trim()).unwrap_or(Value::Null);
    let status = match v.get("status").and_then(Value::as_str) {
        Some("available") => AppleState::Available,
        Some("appleIntelligenceNotEnabled") => AppleState::AppleIntelligenceNotEnabled,
        Some("deviceNotEligible") => AppleState::DeviceNotEligible,
        Some("modelNotReady") => AppleState::ModelNotReady,
        Some("unsupportedOs") => AppleState::UnsupportedOs,
        _ => AppleState::Unknown,
    };
    let context_size = v
        .get("contextSize")
        .and_then(Value::as_u64)
        .map(|n| n.min(1 << 20) as u32)
        .unwrap_or(0);
    AppleAvailability {
        status,
        context_size,
    }
}

/// Probe the helper (cached: 60 s when available, 10 s otherwise).
pub async fn availability() -> AppleAvailability {
    if let Some((at, a)) = *CACHE.lock() {
        let ttl = if a.available() { 60 } else { 10 };
        if at.elapsed() < Duration::from_secs(ttl) {
            return a;
        }
    }
    let a = probe().await;
    *CACHE.lock() = Some((Instant::now(), a));
    a
}

async fn probe() -> AppleAvailability {
    if !cfg!(target_os = "macos") {
        return AppleAvailability::default();
    }
    let Some(path) = helper_path() else {
        return AppleAvailability::default();
    };
    let mut cmd = tokio::process::Command::new(path);
    cmd.arg("availability")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(PROBE_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => parse_availability(&String::from_utf8_lossy(&out.stdout)),
        _ => AppleAvailability {
            status: AppleState::Unknown,
            context_size: 0,
        },
    }
}

/// One generation request for the helper.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleRequest<'a> {
    pub instructions: &'a str,
    pub prompt: &'a str,
    pub max_tokens: u32,
    pub temperature: f32,
    /// JSON Schema for guided generation (the answer is then JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<&'a Value>,
    pub stream: bool,
}

/// One line of the helper's `generate` output.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Delta(String),
    Done(String),
    Failed(String),
}

pub fn parse_line(line: &str) -> Option<Line> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if let Some(code) = v.get("error").and_then(Value::as_str) {
        return Some(Line::Failed(code.chars().take(40).collect()));
    }
    if v.get("done").and_then(Value::as_bool) == Some(true) {
        return Some(Line::Done(
            v.get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        ));
    }
    v.get("delta")
        .and_then(Value::as_str)
        .map(|d| Line::Delta(d.to_string()))
}

/// Map a helper error code to a user-facing error (never includes user text).
pub fn error_for(code: &str) -> Error {
    let msg = match code {
        "context" => ERR_CONTEXT,
        "guardrail" => "the on-device model declined this request",
        "language" => "the on-device model does not support this language",
        "unavailable" => "the Apple on-device model is not available",
        "rate" => "the on-device model is busy; try again in a moment",
        "decoding" | "invalidSchema" => "the on-device model returned an unexpected answer",
        "tooLarge" | "invalidRequest" => "invalid request for the on-device model",
        _ => "the on-device model failed",
    };
    Error::Other(msg.into())
}

/// Run a generation through the helper, streaming text deltas to `on_chunk`.
pub async fn generate(
    req: &AppleRequest<'_>,
    timeout: Duration,
    max_chars: usize,
    on_chunk: &mut (dyn FnMut(&str) + Send),
) -> Result<String> {
    let path = helper_path().ok_or_else(|| error_for("unavailable"))?;
    let body = serde_json::to_vec(req).map_err(|e| Error::Other(format!("request: {e}")))?;
    if body.len() > MAX_REQUEST_BYTES {
        return Err(error_for("tooLarge"));
    }
    let fut = async {
        let mut child = tokio::process::Command::new(path)
            .arg("generate")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| error_for("unavailable"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&body)
                .await
                .map_err(|_| error_for("failed"))?;
            // Dropping closes the pipe: the helper reads to EOF.
        }
        let stdout = child.stdout.take().ok_or_else(|| error_for("failed"))?;
        let mut lines = BufReader::new(stdout).lines();
        let mut streamed = String::new();
        let mut result: Option<Result<String>> = None;
        while let Some(line) = lines.next_line().await.map_err(|_| error_for("failed"))? {
            match parse_line(&line) {
                Some(Line::Delta(d)) => {
                    if streamed.len() < max_chars {
                        streamed.push_str(&d);
                        on_chunk(&d);
                    }
                }
                Some(Line::Done(text)) => {
                    result = Some(Ok(text));
                    break;
                }
                Some(Line::Failed(code)) => {
                    tracing::debug!(code = %code, "apple-llm generation failed");
                    result = Some(Err(error_for(&code)));
                    break;
                }
                None => {}
            }
        }
        let _ = child.wait().await;
        match result {
            Some(Ok(text)) => Ok(cap_chars(&text, max_chars)),
            Some(Err(e)) => Err(e),
            None if !streamed.is_empty() => Ok(cap_chars(&streamed, max_chars)),
            None => Err(error_for("failed")),
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| Error::Other("the on-device model did not answer in time".into()))?
}

fn cap_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn availability_answers() {
        let a = parse_availability(r#"{"status":"available","contextSize":8192}"#);
        assert!(a.available());
        assert_eq!(a.context(), 8192);
        let b = parse_availability(r#"{"status":"appleIntelligenceNotEnabled","contextSize":0}"#);
        assert_eq!(b.status, AppleState::AppleIntelligenceNotEnabled);
        assert_eq!(b.context(), DEFAULT_CONTEXT);
        assert!(b.reason().contains("turned off"));
        assert_eq!(parse_availability("garbage").status, AppleState::Unknown);
        assert_eq!(
            parse_availability(r#"{"status":"deviceNotEligible"}"#).status,
            AppleState::DeviceNotEligible
        );
    }

    #[test]
    fn generate_lines() {
        assert_eq!(
            parse_line(r#"{"delta":"Hi"}"#),
            Some(Line::Delta("Hi".into()))
        );
        assert_eq!(
            parse_line(r#"{"done":true,"text":"Hi there"}"#),
            Some(Line::Done("Hi there".into()))
        );
        assert_eq!(
            parse_line(r#"{"error":"context"}"#),
            Some(Line::Failed("context".into()))
        );
        assert_eq!(parse_line("not json"), None);
        assert_eq!(error_for("context").to_string(), ERR_CONTEXT);
    }

    #[test]
    fn caps_on_char_boundary() {
        assert_eq!(cap_chars("añb", 2), "a");
        assert_eq!(cap_chars("abc", 10), "abc");
    }

    /// Path of a helper built by `src-tauri/build.rs` (or `LUAU_APPLE_LLM`).
    pub(crate) fn built_helper() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("LUAU_APPLE_LLM") {
            return Some(PathBuf::from(p));
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/binaries");
        std::fs::read_dir(root)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("apple-llm-"))
                    && !std::fs::read(p).is_ok_and(|b| b.starts_with(b"#!"))
            })
    }

    /// Real generation on this Mac; skipped when the model is unavailable.
    #[tokio::test]
    async fn real_generation_when_available() {
        let Some(p) = built_helper() else {
            eprintln!("skipped: apple-llm helper not built");
            return;
        };
        set_helper_path(p);
        let a = availability().await;
        if !a.available() {
            eprintln!("skipped: Apple on-device model {:?}", a.status);
            return;
        }
        let t = Instant::now();
        let mut chunks = 0;
        let out = generate(
            &AppleRequest {
                instructions: "Answer with one short word.",
                prompt: "What colour is the sky on a clear day?",
                max_tokens: 20,
                temperature: 0.0,
                schema: None,
                stream: true,
            },
            Duration::from_secs(60),
            10_000,
            &mut |_| chunks += 1,
        )
        .await
        .expect("generation");
        eprintln!("apple-llm: {:?} in {:?}", out, t.elapsed());
        assert!(!out.trim().is_empty());
        assert!(chunks >= 1);
    }
}
