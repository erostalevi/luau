//! Local AI providers, activity summaries, scheduler, code runner, link previews.
//!
//! Layout (hexagonal):
//! - pure domain: [`stage`] (lane → stage inference), [`facts`] (journal →
//!   activity facts), [`prompt`] (facts → chat messages), [`render`]
//!   (deterministic Markdown), [`cards`] (clipboard → card drafts),
//!   [`schedule`] (due-time computation), and the
//!   SSRF address rules in [`web`].
//! - adapters: [`apple`] (on-device model helper), [`llm`] (Ollama /
//!   OpenAI-compatible HTTP), [`remote`] (Anthropic / OpenAI / Gemini / OpenRouter), [`code`] (python
//!   subprocess), [`web`] (link previews), [`store`] (app-data JSON files).
//! - application: [`service`] (`impl Core`) and [`scheduler`] (background thread).
//!
//! Slack delivery is pluggable: the Slack integration registers a sender with
//! [`set_slack_sender`]; scheduled summaries call it when present.

pub mod apple;
pub mod cards;
pub mod code;
pub mod facts;
pub mod llm;
pub mod locale;
pub mod prompt;
pub mod remote;
pub mod render;
pub mod schedule;
pub mod scheduler;
pub mod service;
pub mod setup;
pub mod stage;
pub mod store;
pub mod web;

use std::sync::RwLock;

/// Sends `text` to a Slack `channel` (`#name` or channel id).
pub type SlackSender = Box<dyn Fn(&str, &str) -> crate::Result<()> + Send + Sync>;

static SLACK: RwLock<Option<SlackSender>> = RwLock::new(None);

/// Register (or replace) the Slack sender. Called by the Slack integration.
pub fn set_slack_sender(f: SlackSender) {
    if let Ok(mut g) = SLACK.write() {
        *g = Some(f);
    }
}

/// Remove the Slack sender (e.g. when Slack is disconnected).
pub fn clear_slack_sender() {
    if let Ok(mut g) = SLACK.write() {
        *g = None;
    }
}

pub fn slack_connected() -> bool {
    SLACK.read().map(|g| g.is_some()).unwrap_or(false)
}

/// Send through the registered sender. `Err(NotFound)` when Slack is not connected.
pub fn send_to_slack(channel: &str, text: &str) -> crate::Result<()> {
    let g = SLACK
        .read()
        .map_err(|_| crate::Error::Other("slack sender poisoned".into()))?;
    match g.as_ref() {
        Some(f) => f(channel, text),
        None => Err(crate::Error::not_found("Slack not connected")),
    }
}
