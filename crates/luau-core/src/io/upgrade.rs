//! Board schema upgrades.
//!
//! Rules (QUESTIONNAIRE C.18):
//! - `schema == SCHEMA`: normal.
//! - `schema < SCHEMA`: opened normally; "upgrade" runs the migrations and
//!   stamps the current schema.
//! - `schema > SCHEMA`: opened **read-only** by the store (`newer_schema:N`).
//!   "Convert to this version" (after a warning in the UI) stamps this app's
//!   schema. Unknown JSON keys are kept as they are; only `schema` changes.
//!
//! Only JSON manifests are touched; card Markdown is never rewritten.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::app::Core;
use crate::brand::{BOARD_FILE, INDEX_JSON, MARKER_DIR, SCHEMA};
use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, read_to_string};
use crate::model::BoardSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SchemaStatus {
    Current,
    Older,
    Newer,
}

pub fn status(schema: u32) -> SchemaStatus {
    match schema.cmp(&SCHEMA) {
        std::cmp::Ordering::Equal => SchemaStatus::Current,
        std::cmp::Ordering::Less => SchemaStatus::Older,
        std::cmp::Ordering::Greater => SchemaStatus::Newer,
    }
}

/// Migration from `from` to `from + 1` over a JSON manifest (none yet: v1 is the first).
type Migration = fn(&mut Value);
const MIGRATIONS: &[(u32, Migration)] = &[];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeReport {
    pub from: u32,
    pub to: u32,
    pub files: usize,
}

fn rewrite(path: &Path) -> Result<bool> {
    let text = read_to_string(path)?;
    let Ok(mut v) = serde_json::from_str::<Value>(&text) else {
        return Ok(false); // malformed files are recovered by the loader instead
    };
    let Some(obj) = v.as_object_mut() else {
        return Ok(false);
    };
    let cur = obj.get("schema").and_then(Value::as_u64).unwrap_or(0) as u32;
    if cur == SCHEMA {
        return Ok(false);
    }
    if cur < SCHEMA {
        for (from, m) in MIGRATIONS {
            if *from >= cur {
                m(&mut v);
            }
        }
    }
    if let Some(obj) = v.as_object_mut() {
        obj.insert("schema".into(), Value::from(SCHEMA));
    }
    atomic_write(path, crate::json_fmt::format_value(&v, 80).as_bytes())?;
    Ok(true)
}

/// Stamp this app's schema on a board folder (manifest + every `index.json`).
pub fn upgrade_board(root: &Path) -> Result<UpgradeReport> {
    let manifest = root.join(MARKER_DIR).join(BOARD_FILE);
    let v: Value = serde_json::from_str(&read_to_string(&manifest)?)
        .map_err(|e| Error::invalid(format!("board.json: {e}")))?;
    let from = v.get("schema").and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut files = 0;
    if from == SCHEMA {
        return Ok(UpgradeReport {
            from,
            to: SCHEMA,
            files,
        });
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if name.starts_with('.') || ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if !crate::store::is_board(&e.path()) {
                    stack.push(e.path());
                }
            } else if name == INDEX_JSON && rewrite(&e.path())? {
                files += 1;
            }
        }
    }
    if rewrite(&manifest)? {
        files += 1; // manifest last: an interrupted run stays read-only
    }
    Ok(UpgradeReport {
        from,
        to: SCHEMA,
        files,
    })
}

impl Core {
    /// Convert an open board to this app's schema and reopen it.
    pub fn upgrade_board_schema(
        self: &Arc<Self>,
        board: &str,
    ) -> Result<(UpgradeReport, BoardSnapshot)> {
        let root = self.board_root(board)?;
        if !crate::store::is_board(&root) {
            return Err(Error::invalid("not a board"));
        }
        let report = upgrade_board(&root)?;
        self.close_board(board);
        let snap = self.open_board(&root)?;
        tracing::info!("board {board} schema {} -> {}", report.from, report.to);
        Ok((report, snap))
    }

    /// Rewrite a damaged `board.json` from the salvaged state (lanes, order,
    /// archive flags as found on disk) and reopen the board. Only allowed for
    /// boards opened read-only because of a corrupt manifest; the original is
    /// kept in `.luau/cache/recovered/`.
    pub fn repair_manifest(self: &Arc<Self>, board: &str) -> Result<BoardSnapshot> {
        let b = self.board(board)?;
        let root = {
            let mut s = b.lock();
            if s.state.read_only.as_deref() != Some(crate::store::CORRUPT_MANIFEST) {
                return Err(Error::invalid("board_not_damaged"));
            }
            s.save_manifest()?;
            s.state.root.clone()
        };
        self.close_board(board);
        let snap = self.open_board(&root)?;
        tracing::info!("board {board} manifest repaired");
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_manifest_opens_read_only_without_rewrite_and_repairs() {
        use crate::app::{AppPaths, CoreEvent, EventSink};
        struct Nop;
        impl EventSink for Nop {
            fn emit(&self, _e: CoreEvent) {}
        }
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(AppPaths::under(&d.path().join("app")), Arc::new(Nop)).unwrap();
        let root = d.path().join("Work");
        let snap = c
            .create_board(
                &root,
                "Work",
                BoardKind::Kanban,
                &["To do".into(), "Done".into()],
                false,
            )
            .unwrap();
        let id = snap.header.id.clone();
        c.close_board(&id);
        let mp = root.join(".luau").join("board.json");
        let good = std::fs::read_to_string(&mp).unwrap();
        // Half-written by a sync tool: truncated JSON.
        let broken = &good[..good.len() / 2];
        std::fs::write(&mp, broken).unwrap();
        let s = c.open_board(&root).unwrap();
        assert_eq!(s.header.id, id, "id salvaged");
        assert_eq!(s.header.name, "Work", "name salvaged");
        assert_eq!(s.lanes.len(), 2, "lanes found on disk");
        assert_eq!(
            s.header.read_only.as_deref(),
            Some(crate::store::CORRUPT_MANIFEST)
        );
        assert_eq!(
            std::fs::read_to_string(&mp).unwrap(),
            broken,
            "never rewritten on load"
        );
        // Reloading does not pile up recovered copies.
        c.close_board(&id);
        c.open_board(&root).unwrap();
        let rec = std::fs::read_dir(root.join(".luau/cache/recovered"))
            .unwrap()
            .count();
        assert_eq!(rec, 1);
        // Explicit repair writes a valid manifest and reopens writable.
        let s = c.repair_manifest(&id).unwrap();
        assert!(s.header.read_only.is_none());
        let m: crate::model::BoardManifest =
            serde_json::from_str(&std::fs::read_to_string(&mp).unwrap()).unwrap();
        assert_eq!(m.lanes.len(), 2);
    }
    use crate::model::BoardKind;
    use crate::store::BoardStore;

    #[test]
    fn schema_rules() {
        assert_eq!(status(SCHEMA), SchemaStatus::Current);
        assert_eq!(status(SCHEMA + 1), SchemaStatus::Newer);
        assert_eq!(status(0), SchemaStatus::Older);
    }

    #[test]
    fn newer_board_is_read_only_until_converted() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("B");
        let mut s =
            BoardStore::create(&root, "b123456".into(), "B".into(), BoardKind::Kanban).unwrap();
        s.apply(crate::store::Op::CreateLane {
            id: "k123456".into(),
            name: "L".into(),
            index: None,
        })
        .unwrap();
        drop(s);
        let mp = root.join(MARKER_DIR).join(BOARD_FILE);
        let lane = root.join("k123456").join(INDEX_JSON);
        let newer = SCHEMA + 1;
        for p in [&mp, &lane] {
            let t = fs::read_to_string(p).unwrap();
            let mut v: Value = serde_json::from_str(&t).unwrap();
            v["schema"] = Value::from(newer);
            v["futureKey"] = Value::from("kept");
            fs::write(p, serde_json::to_string(&v).unwrap()).unwrap();
        }
        let s = BoardStore::open(&root).unwrap();
        assert_eq!(
            s.state.read_only.as_deref(),
            Some(format!("newer_schema:{newer}").as_str())
        );
        drop(s);

        let r = upgrade_board(&root).unwrap();
        assert_eq!((r.from, r.to, r.files), (newer, SCHEMA, 2));
        let s = BoardStore::open(&root).unwrap();
        assert!(s.state.read_only.is_none());
        let v: Value = serde_json::from_str(&fs::read_to_string(&mp).unwrap()).unwrap();
        assert_eq!(v["futureKey"], "kept");
        // Idempotent.
        assert_eq!(upgrade_board(&root).unwrap().files, 0);
    }
}
