//! Known boards (a cache: discovery is the source of truth).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::fsutil::atomic_write;
use crate::json_fmt;
use crate::model::BoardKind;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardEntry {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: BoardKind,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub missing: bool,
    /// Remote mirror (stored in app data, not a board folder).
    #[serde(default)]
    pub mirror: bool,
    /// Explorer section override: "boards" or "mirrors".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened: Option<i64>,
    #[serde(default)]
    pub last_seen: i64,
    /// Other paths holding the same id (copies awaiting a decision).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub duplicates: Vec<String>,
    /// Plain folder opened "as is" (no `.lull`, read-only; see `io::loose`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub loose: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Registry {
    pub boards: Vec<BoardEntry>,
    /// Manual explorer order (ids); empty = alphabetical.
    pub order: Vec<String>,
    pub mirror_order: Vec<String>,
}

impl Registry {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let s = json_fmt::to_string(self).unwrap_or_default();
        atomic_write(path, s.as_bytes())
    }

    pub fn get(&self, id: &str) -> Option<&BoardEntry> {
        self.boards.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut BoardEntry> {
        self.boards.iter_mut().find(|b| b.id == id)
    }

    pub fn by_path(&self, path: &str) -> Option<&BoardEntry> {
        let norm = normalize(path);
        self.boards.iter().find(|b| normalize(&b.path) == norm)
    }

    /// Insert or update an entry found at `path`. Returns true when changed.
    pub fn upsert(
        &mut self,
        id: &str,
        path: &str,
        name: &str,
        kind: BoardKind,
        mirror: bool,
    ) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(e) = self.get_mut(id) {
            let mut changed = false;
            if normalize(&e.path) != normalize(path) {
                if !e.missing && Path::new(&e.path).join(crate::brand::MARKER_DIR).exists() {
                    // Same id at two live paths: record a duplicate instead of flapping.
                    if !e.duplicates.iter().any(|d| normalize(d) == normalize(path)) {
                        e.duplicates.push(path.to_string());
                        changed = true;
                    }
                } else {
                    e.path = path.to_string();
                    changed = true;
                }
            }
            if e.name != name || e.kind != kind || e.missing || e.mirror != mirror {
                e.name = name.to_string();
                e.kind = kind;
                e.missing = false;
                e.mirror = mirror;
                changed = true;
            }
            e.last_seen = now;
            changed
        } else {
            self.boards.push(BoardEntry {
                id: id.to_string(),
                path: path.to_string(),
                name: name.to_string(),
                kind,
                pinned: false,
                hidden: false,
                missing: false,
                mirror,
                section: None,
                last_opened: None,
                last_seen: now,
                duplicates: vec![],
                loose: false,
            });
            true
        }
    }
}

fn normalize(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}
