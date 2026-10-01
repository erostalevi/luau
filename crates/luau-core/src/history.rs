//! Persistent history: an append-only journal per board plus content-addressed
//! compressed blobs for card versions.
//!
//! ```text
//! <board>/.luau/history/2026-09-30.jsonl
//! <board>/.luau/history/blobs/ab/abcdef….gz
//! ```

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, sha256_hex};
use crate::store::marker_dir;

/// Who caused a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    You,
    External,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    /// RFC 3339 timestamp.
    pub ts: String,
    pub board: String,
    /// Operation kind (`createCard`, `move`, `edit`, `trash`, `externalEdit`, `remoteSync`, `externalWrite`, …).
    pub kind: String,
    pub origin: Origin,
    /// Human label shown in the timeline.
    pub label: String,
    /// Ids affected (cards and lanes).
    pub ids: Vec<String>,
    /// Structured details (titles, from/to lane names, fields changed, service…).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

pub fn history_dir(root: &Path) -> PathBuf {
    marker_dir(root).join("history")
}

fn blob_path(root: &Path, hash: &str) -> PathBuf {
    history_dir(root)
        .join("blobs")
        .join(&hash[..2.min(hash.len())])
        .join(format!("{hash}.gz"))
}

/// Store content; returns its sha256 hash. Deduplicated by hash.
pub fn put_blob(root: &Path, content: &str) -> Result<String> {
    let hash = sha256_hex(content.as_bytes());
    if !has_marker(root) {
        return Ok(hash);
    }
    let p = blob_path(root, &hash);
    if !p.exists() {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(content.as_bytes())
            .map_err(|e| Error::io(&p, e))?;
        let bytes = enc.finish().map_err(|e| Error::io(&p, e))?;
        atomic_write(&p, &bytes)?;
    }
    Ok(hash)
}

pub fn get_blob(root: &Path, hash: &str) -> Result<String> {
    if hash.len() < 8 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::invalid("bad hash"));
    }
    let p = blob_path(root, hash);
    let f = fs::File::open(&p).map_err(|e| Error::io(&p, e))?;
    let mut s = String::new();
    GzDecoder::new(f)
        .read_to_string(&mut s)
        .map_err(|e| Error::io(&p, e))?;
    Ok(s)
}

/// History lives in the board marker dir; folders opened "as is" (no marker)
/// have no history, and we never create a marker dir implicitly.
fn has_marker(root: &Path) -> bool {
    marker_dir(root).is_dir()
}

pub fn append(root: &Path, entry: &JournalEntry) -> Result<()> {
    if !has_marker(root) {
        return Ok(());
    }
    let dir = history_dir(root);
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    let day = entry.ts.get(..10).unwrap_or("unknown");
    let p = dir.join(format!("{day}.jsonl"));
    let mut line = serde_json::to_string(entry).map_err(|e| Error::Json {
        path: p.clone(),
        source: e,
    })?;
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .map_err(|e| Error::io(&p, e))?;
    f.write_all(line.as_bytes()).map_err(|e| Error::io(&p, e))?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryFilter {
    /// Inclusive RFC 3339 / date bounds.
    pub from: Option<String>,
    pub to: Option<String>,
    pub ids: Vec<String>,
    pub kinds: Vec<String>,
    pub origins: Vec<Origin>,
    pub limit: Option<usize>,
}

/// Read entries (newest first) matching the filter.
pub fn query(root: &Path, f: &HistoryFilter) -> Vec<JournalEntry> {
    let dir = history_dir(root);
    let mut days: Vec<String> = fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_str()
                        .and_then(|n| n.strip_suffix(".jsonl"))
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    days.sort();
    days.reverse();
    let from_day = f.from.as_deref().and_then(|s| s.get(..10));
    let to_day = f.to.as_deref().and_then(|s| s.get(..10));
    let limit = f.limit.unwrap_or(500);
    let mut out = Vec::new();
    for day in days {
        if from_day.is_some_and(|d| day.as_str() < d) {
            break;
        }
        if to_day.is_some_and(|d| day.as_str() > d) {
            continue;
        }
        let Ok(text) = fs::read_to_string(dir.join(format!("{day}.jsonl"))) else {
            continue;
        };
        let mut day_entries: Vec<JournalEntry> = text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        day_entries.reverse();
        for e in day_entries {
            if f.from.as_deref().is_some_and(|from| e.ts.as_str() < from) {
                continue;
            }
            if f.to.as_deref().is_some_and(|to| e.ts.as_str() > to) {
                continue;
            }
            if !f.ids.is_empty() && !e.ids.iter().any(|i| f.ids.contains(i)) {
                continue;
            }
            if !f.kinds.is_empty() && !f.kinds.contains(&e.kind) {
                continue;
            }
            if !f.origins.is_empty() && !f.origins.contains(&e.origin) {
                continue;
            }
            out.push(e);
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

/// Prune blobs not referenced by entries newer than `keep_days`, and blobs
/// once total size exceeds `max_bytes` (oldest first). Structure entries are kept.
pub fn prune(root: &Path, keep_days: u32, max_bytes: u64) -> Result<usize> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days as i64)).to_rfc3339();
    let recent = query(
        root,
        &HistoryFilter {
            from: Some(cutoff),
            limit: Some(usize::MAX),
            ..Default::default()
        },
    );
    let keep: std::collections::HashSet<String> = recent
        .iter()
        .flat_map(|e| e.before.iter().chain(e.after.iter()).cloned())
        .collect();
    let blobs_dir = history_dir(root).join("blobs");
    let mut all: Vec<(PathBuf, std::time::SystemTime, u64, String)> = Vec::new();
    for e in walkdir::WalkDir::new(&blobs_dir).into_iter().flatten() {
        if e.file_type().is_file() {
            let name = e
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".gz")
                .to_string();
            let meta = e.metadata().ok();
            all.push((
                e.path().to_path_buf(),
                meta.as_ref()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::UNIX_EPOCH),
                meta.map(|m| m.len()).unwrap_or(0),
                name,
            ));
        }
    }
    all.sort_by_key(|x| x.1);
    let mut total: u64 = all.iter().map(|x| x.2).sum();
    let mut removed = 0;
    for (p, _, size, hash) in all {
        let over = total > max_bytes;
        if (!keep.contains(&hash) || over) && fs::remove_file(&p).is_ok() {
            removed += 1;
            total = total.saturating_sub(size);
        }
    }
    Ok(removed)
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_roundtrip_and_dedupe() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(marker_dir(d.path())).unwrap();
        let h1 = put_blob(d.path(), "# Hello\n").unwrap();
        let h2 = put_blob(d.path(), "# Hello\n").unwrap();
        assert_eq!(h1, h2);
        assert_eq!(get_blob(d.path(), &h1).unwrap(), "# Hello\n");
        assert!(get_blob(d.path(), "../../etc").is_err());
    }

    #[test]
    fn append_and_query() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(marker_dir(d.path())).unwrap();
        for (i, kind) in ["createCard", "move", "edit"].iter().enumerate() {
            append(
                d.path(),
                &JournalEntry {
                    ts: format!("2026-09-{:02}T10:00:00.000Z", 28 + i),
                    board: "b1".into(),
                    kind: kind.to_string(),
                    origin: Origin::You,
                    label: kind.to_string(),
                    ids: vec![format!("c{i}")],
                    details: Value::Null,
                    before: None,
                    after: None,
                },
            )
            .unwrap();
        }
        let all = query(d.path(), &HistoryFilter::default());
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].kind, "edit"); // newest first
        let some = query(
            d.path(),
            &HistoryFilter {
                from: Some("2026-09-29".into()),
                ..Default::default()
            },
        );
        assert_eq!(some.len(), 2);
        let ids = query(
            d.path(),
            &HistoryFilter {
                ids: vec!["c0".into()],
                ..Default::default()
            },
        );
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn no_history_without_marker_dir() {
        let d = tempfile::tempdir().unwrap();
        let e = JournalEntry {
            ts: now(),
            board: "b1".into(),
            kind: "edit".into(),
            origin: Origin::You,
            label: "x".into(),
            ids: vec![],
            details: Value::Null,
            before: None,
            after: None,
        };
        append(d.path(), &e).unwrap();
        put_blob(d.path(), "x").unwrap();
        assert!(!marker_dir(d.path()).exists());
    }
}
