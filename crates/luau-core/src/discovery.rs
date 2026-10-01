//! Board discovery: walk search roots looking for `.luau/board.json` markers.
//!
//! Prunes system and heavy folders, never follows symlinks, and stops
//! descending at board roots (nested boards are separate boards).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::brand::{BOARD_FILE, MARKER_DIR};
use crate::model::{BoardKind, BoardManifest};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub id: String,
    pub name: String,
    pub kind: BoardKind,
    pub path: String,
}

/// Default folder names skipped everywhere.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    ".git",
    ".hg",
    ".svn",
    "Library",
    "AppData",
    "Applications",
    "Program Files",
    "Program Files (x86)",
    "Windows",
    "$Recycle.Bin",
    "System Volume Information",
    "venv",
    ".venv",
    "__pycache__",
    ".cache",
    ".Trash",
    "Pictures",
    "Music",
    "Movies",
    "Photos Library.photoslibrary",
    "go",
    "vendor",
    ".npm",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    "snap",
    "proc",
    "sys",
    "dev",
];

pub fn default_roots() -> Vec<PathBuf> {
    dirs::home_dir().into_iter().collect()
}

pub struct Options {
    pub roots: Vec<PathBuf>,
    pub excludes: Vec<String>,
    pub max_depth: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            roots: default_roots(),
            excludes: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
            max_depth: 12,
        }
    }
}

pub fn read_marker(root: &Path) -> Option<Found> {
    let p = root.join(MARKER_DIR).join(BOARD_FILE);
    let text = std::fs::read_to_string(p).ok()?;
    // A damaged manifest still marks a board: salvage it (read-only on open).
    let m: BoardManifest =
        serde_json::from_str(&text).unwrap_or_else(|_| crate::store::salvage_manifest(root, &text));
    Some(Found {
        id: m.id,
        name: m.name,
        kind: m.kind,
        path: root.to_string_lossy().into_owned(),
    })
}

/// Walk all roots and return every board found. `cancel` is polled regularly.
pub fn scan(opts: &Options, cancel: &dyn Fn() -> bool) -> Vec<Found> {
    let mut out = Vec::new();
    for root in &opts.roots {
        let mut it = walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(opts.max_depth)
            .into_iter();
        while let Some(entry) = it.next() {
            if cancel() {
                return out;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            if entry.depth() > 0 {
                let hidden = name.starts_with('.') && name != MARKER_DIR;
                if hidden
                    || opts.excludes.iter().any(|x| x.eq_ignore_ascii_case(&name))
                    || name.ends_with(".app")
                {
                    it.skip_current_dir();
                    continue;
                }
            }
            if name == MARKER_DIR {
                it.skip_current_dir();
                continue;
            }
            if let Some(found) = read_marker(entry.path()) {
                out.push(found);
                // Boards are leaves for discovery: nested boards are not allowed.
                it.skip_current_dir();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BoardStore;

    #[test]
    fn finds_boards_and_skips_excluded() {
        let d = tempfile::tempdir().unwrap();
        BoardStore::create(
            &d.path().join("a/Work"),
            "b00000a".into(),
            "Work".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join("node_modules/x"),
            "b00000b".into(),
            "Hidden".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join(".secret/y"),
            "b00000c".into(),
            "Dot".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join("notes"),
            "b00000d".into(),
            "Notes".into(),
            BoardKind::Files,
        )
        .unwrap();
        let opts = Options {
            roots: vec![d.path().to_path_buf()],
            ..Default::default()
        };
        let mut found: Vec<String> = scan(&opts, &|| false).into_iter().map(|f| f.name).collect();
        found.sort();
        assert_eq!(found, vec!["Notes", "Work"]);
    }
}
