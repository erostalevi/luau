//! App-data persistence for this module (never inside board folders):
//!
//! ```text
//! <data>/summaries/schedules.json      scheduled summaries
//! <data>/summaries/saved/<id>.json     generated summaries (history, capped)
//! <data>/code-trust.json               boards allowed to run code cells
//! <data>/ai-consent.json                remote AI services the user agreed to send card text to
//! <data>/cache/code/<sha>.json         last output per code cell
//! <data>/cache/previews/<sha>.json     link previews
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::schedule::Schedule;
use crate::error::{Error, Result};
use crate::fsutil::atomic_write;

/// Generated summaries kept in history.
pub const MAX_SAVED: usize = 200;

pub fn read_json<T: DeserializeOwned>(p: &Path) -> Option<T> {
    fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

pub fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(v).map_err(|e| Error::Json {
        path: p.to_path_buf(),
        source: e,
    })?;
    atomic_write(p, s.as_bytes())
}

/// `[a-z0-9]` ids we generate; anything else is rejected before touching the FS.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn new_id(prefix: &str) -> String {
    let ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    format!("{prefix}{}{}", radix36(ms), crate::ids::random_suffix(4))
}

fn radix36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut s = Vec::new();
    loop {
        s.push(D[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedSummary {
    pub id: String,
    /// RFC 3339 (UTC).
    pub created: String,
    pub title: String,
    pub from: String,
    pub to: String,
    pub boards: Vec<String>,
    pub detail: u8,
    pub prompt: String,
    /// `ai` | `basic`.
    pub engine: String,
    pub model: Option<String>,
    pub schedule_id: Option<String>,
    pub markdown: String,
    /// Delivery results, e.g. `notification: ok`, `slack: Slack not connected`.
    pub delivery: Vec<String>,
}

/// Summary without the body (history lists).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedMeta {
    pub id: String,
    pub created: String,
    pub title: String,
    pub from: String,
    pub to: String,
    pub engine: String,
    pub model: Option<String>,
    pub schedule_id: Option<String>,
    pub delivery: Vec<String>,
    /// First characters of the body.
    pub excerpt: String,
}

pub struct AiStore {
    data: PathBuf,
}

impl AiStore {
    pub fn new(data: &Path) -> Self {
        AiStore {
            data: data.to_path_buf(),
        }
    }

    fn dir(&self) -> PathBuf {
        self.data.join("summaries")
    }

    // --- schedules --------------------------------------------------------------

    pub fn schedules(&self) -> Vec<Schedule> {
        read_json(&self.dir().join("schedules.json")).unwrap_or_default()
    }

    pub fn save_schedules(&self, list: &[Schedule]) -> Result<()> {
        write_json(&self.dir().join("schedules.json"), &list)
    }

    // --- saved summaries -----------------------------------------------------------

    fn saved_dir(&self) -> PathBuf {
        self.dir().join("saved")
    }

    pub fn save_summary(&self, s: &SavedSummary) -> Result<()> {
        if !valid_id(&s.id) {
            return Err(Error::invalid("bad summary id"));
        }
        write_json(&self.saved_dir().join(format!("{}.json", s.id)), s)?;
        self.prune_saved();
        Ok(())
    }

    pub fn get_summary(&self, id: &str) -> Result<SavedSummary> {
        if !valid_id(id) {
            return Err(Error::invalid("bad summary id"));
        }
        read_json(&self.saved_dir().join(format!("{id}.json")))
            .ok_or_else(|| Error::not_found(format!("summary {id}")))
    }

    pub fn delete_summary(&self, id: &str) -> Result<()> {
        if !valid_id(id) {
            return Err(Error::invalid("bad summary id"));
        }
        let p = self.saved_dir().join(format!("{id}.json"));
        if p.exists() {
            fs::remove_file(&p).map_err(|e| Error::io(&p, e))?;
        }
        Ok(())
    }

    fn all_saved(&self) -> Vec<SavedSummary> {
        let Ok(rd) = fs::read_dir(self.saved_dir()) else {
            return vec![];
        };
        let mut v: Vec<SavedSummary> = rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| read_json(&e.path()))
            .collect();
        v.sort_by(|a, b| b.created.cmp(&a.created));
        v
    }

    pub fn list_summaries(&self, limit: usize) -> Vec<SavedMeta> {
        self.all_saved()
            .into_iter()
            .take(limit)
            .map(|s| SavedMeta {
                excerpt: super::render::plain_text(&s.markdown)
                    .lines()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(160)
                    .collect(),
                id: s.id,
                created: s.created,
                title: s.title,
                from: s.from,
                to: s.to,
                engine: s.engine,
                model: s.model,
                schedule_id: s.schedule_id,
                delivery: s.delivery,
            })
            .collect()
    }

    fn prune_saved(&self) {
        for old in self.all_saved().into_iter().skip(MAX_SAVED) {
            let _ = fs::remove_file(self.saved_dir().join(format!("{}.json", old.id)));
        }
    }

    // --- code trust ------------------------------------------------------------

    fn trust_path(&self) -> PathBuf {
        self.data.join("code-trust.json")
    }

    pub fn trusted_boards(&self) -> Vec<String> {
        read_json(&self.trust_path()).unwrap_or_default()
    }

    pub fn is_trusted(&self, board: &str) -> bool {
        self.trusted_boards().iter().any(|b| b == board)
    }

    pub fn set_trusted(&self, board: &str, trusted: bool) -> Result<()> {
        let mut v = self.trusted_boards();
        v.retain(|b| b != board);
        if trusted {
            v.push(board.to_string());
        }
        write_json(&self.trust_path(), &v)
    }

    // --- remote AI consent -------------------------------------------------------
    // Kept here (app data), not in settings, so an imported settings bundle
    // can never turn on sending card text to a remote service.

    fn consent_path(&self) -> PathBuf {
        self.data.join("ai-consent.json")
    }

    pub fn remote_consents(&self) -> Vec<String> {
        read_json(&self.consent_path()).unwrap_or_default()
    }

    pub fn has_remote_consent(&self, provider: &str) -> bool {
        self.remote_consents().iter().any(|p| p == provider)
    }

    pub fn set_remote_consent(&self, provider: &str, granted: bool) -> Result<()> {
        let mut v = self.remote_consents();
        v.retain(|p| p != provider);
        if granted {
            v.push(provider.to_string());
        }
        write_json(&self.consent_path(), &v)
    }

    // --- caches ------------------------------------------------------------------

    pub fn cache_path(&self, kind: &str, hash: &str) -> PathBuf {
        self.data
            .join("cache")
            .join(kind)
            .join(format!("{hash}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_summaries_roundtrip_and_prune() {
        let d = tempfile::tempdir().unwrap();
        let st = AiStore::new(d.path());
        for i in 0..(MAX_SAVED + 3) {
            st.save_summary(&SavedSummary {
                id: format!("s{i:04}"),
                created: format!("2026-09-30T00:{:02}:{:02}.000Z", i / 60, i % 60),
                title: "T".into(),
                markdown: "# Activity summary\n- did things".into(),
                ..Default::default()
            })
            .unwrap();
        }
        let list = st.list_summaries(1000);
        assert_eq!(list.len(), MAX_SAVED);
        assert_eq!(list[0].id, format!("s{:04}", MAX_SAVED + 2));
        assert_eq!(list[0].excerpt, "• did things");
        assert!(st.get_summary("s0000").is_err()); // pruned
        st.delete_summary(&list[0].id).unwrap();
        assert!(st.get_summary(&list[0].id).is_err());
        assert!(st.get_summary("../etc").is_err());
    }

    #[test]
    fn trust_and_schedules() {
        let d = tempfile::tempdir().unwrap();
        let st = AiStore::new(d.path());
        assert!(!st.is_trusted("b1"));
        st.set_trusted("b1", true).unwrap();
        assert!(st.is_trusted("b1"));
        st.set_trusted("b1", false).unwrap();
        assert!(!st.is_trusted("b1"));
        assert!(!st.has_remote_consent("anthropic"));
        st.set_remote_consent("anthropic", true).unwrap();
        assert!(st.has_remote_consent("anthropic") && !st.has_remote_consent("gemini"));
        st.set_remote_consent("anthropic", false).unwrap();
        assert!(!st.has_remote_consent("anthropic"));
        assert!(st.schedules().is_empty());
        st.save_schedules(&[Schedule {
            id: "x".into(),
            name: "n".into(),
            ..Default::default()
        }])
        .unwrap();
        assert_eq!(st.schedules()[0].name, "n");
        assert!(new_id("s").starts_with('s'));
    }
}
