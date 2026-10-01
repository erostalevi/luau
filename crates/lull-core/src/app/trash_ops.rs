//! Trash maintenance for the History panel's Trash view: permanently delete
//! individual entries ("Delete forever"). Retention cleanup and "Empty trash"
//! reuse [`Core::trash_purge`].
//!
//! Permanent deletions are journaled (kind `purge`) so there is an auditable
//! trace of what was destroyed and when. Only titles and ids are recorded,
//! never card bodies.

use serde_json::json;

use super::Core;
use crate::error::{Error, Result};
use crate::fsutil::remove_path;
use crate::history::{self, JournalEntry, Origin};
use crate::ids::{IdKind, is_id};
use crate::store::trash;

impl Core {
    /// Permanently delete trash entries by id. Unknown ids are skipped; malformed
    /// ids are rejected (they would otherwise be joined into a path).
    /// Returns the number of entries removed.
    pub fn trash_delete(&self, board: &str, ids: &[String]) -> Result<usize> {
        if ids.iter().any(|id| !is_id(id, IdKind::Trash)) {
            return Err(Error::invalid("bad trash id"));
        }
        if ids.is_empty() {
            return Ok(0);
        }
        let root = self.board_root(board)?;
        let mut titles = Vec::new();
        let mut items = Vec::new();
        for id in ids {
            let Ok(entry) = trash::read_entry(&root, id) else {
                continue;
            };
            remove_path(&trash::trash_root(&root).join(id))?;
            titles.push(entry.title);
            items.push(entry.item_id);
        }
        if !items.is_empty() {
            let entry = JournalEntry {
                ts: history::now(),
                board: board.to_string(),
                kind: "purge".into(),
                origin: Origin::You,
                label: "Deleted forever".into(),
                ids: items.clone(),
                details: json!({ "titles": titles }),
                before: None,
                after: None,
            };
            if let Err(e) = history::append(&root, &entry) {
                tracing::warn!("journal purge: {e}");
            }
        }
        Ok(items.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use crate::app::{AppPaths, Core, CoreEvent, EventSink};
    use crate::history::HistoryFilter;
    use crate::model::{BoardKind, Parent};
    use crate::store::Op;

    struct Sink(Mutex<Vec<String>>);
    impl EventSink for Sink {
        fn emit(&self, _e: CoreEvent) {
            self.0.lock().push(String::new());
        }
    }

    #[test]
    fn delete_forever_removes_entry_and_journals() {
        let d = tempfile::tempdir().unwrap();
        let c = Core::new(
            AppPaths::under(&d.path().join("app")),
            Arc::new(Sink(Mutex::new(vec![]))),
        )
        .unwrap();
        let snap = c
            .create_board(
                &d.path().join("Work"),
                "Work",
                BoardKind::Kanban,
                &["To do".into()],
                false,
            )
            .unwrap();
        let b = snap.header.id.clone();
        let k = snap.lanes[0].id.clone();
        let id = c.new_card_id();
        c.apply(
            &b,
            Op::CreateCard {
                id: id.clone(),
                parent: Parent::Lane(k),
                index: None,
                content: "# Old idea\n".into(),
            },
            "New card",
            None,
        )
        .unwrap();
        let r = c
            .apply(
                &b,
                Op::Trash {
                    nodes: vec![id.clone()],
                    lanes: vec![],
                },
                "Delete",
                None,
            )
            .unwrap();
        assert_eq!(c.trash_list(&b).unwrap().len(), 1);

        // Malformed ids never touch the file system.
        assert!(c.trash_delete(&b, &["../../etc".into()]).is_err());
        assert!(c.trash_delete(&b, &["c123456".into()]).is_err());
        // Unknown but well-formed ids are skipped.
        assert_eq!(c.trash_delete(&b, &["tzzzzzz".into()]).unwrap(), 0);

        assert_eq!(c.trash_delete(&b, &r.trashed).unwrap(), 1);
        assert!(c.trash_list(&b).unwrap().is_empty());
        let h = c.history(&b, &HistoryFilter::default()).unwrap();
        let purge = h
            .iter()
            .find(|e| e.kind == "purge")
            .expect("purge journaled");
        assert_eq!(purge.ids, vec![id]);
        assert_eq!(purge.details["titles"][0], "Old idea");
    }
}
