//! Card templates on disk and "new board from template".
//!
//! - Card templates: `<board>/.luau/templates/*.md` (scope `board`) and
//!   `<appData>/templates/*.md` (scope `global`). Built-in card templates live
//!   in the UI (localized).
//! - Board templates are described by the UI (localized names and starter
//!   cards) as a [`BoardTemplate`] and created here in one journaled batch.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::app::Core;
use crate::brand::MARKER_DIR;
use crate::error::{Error, Result};
use crate::ids::{IdKind, new_id};
use crate::model::{BoardKind, BoardSnapshot, Parent};
use crate::store::Op;

pub const MAX_TEMPLATES: usize = 200;
pub const MAX_TEMPLATE_BYTES: u64 = 256 * 1024;
pub const MAX_LANES: usize = 50;
pub const MAX_CARDS: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardTemplate {
    pub name: String,
    pub content: String,
    #[serde(default)]
    pub scope: String,
}

/// `*.md` files of `dir` as templates (sorted by name, size-capped).
pub fn read_dir_templates(dir: &Path, scope: &str) -> Vec<CardTemplate> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<CardTemplate> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !super::is_md(&name) {
                return None;
            }
            if e.metadata().ok()?.len() > MAX_TEMPLATE_BYTES {
                return None;
            }
            Some(CardTemplate {
                name: super::md_stem(&name).to_string(),
                content: crate::fsutil::read_to_string(&e.path()).ok()?,
                scope: scope.to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| super::natural_cmp(&a.name, &b.name));
    out.truncate(MAX_TEMPLATES);
    out
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateLane {
    pub name: String,
    #[serde(default)]
    pub cards: Vec<String>,
}

/// A board template as sent by the UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardTemplate {
    #[serde(default)]
    pub kind: BoardKind,
    #[serde(default)]
    pub lanes: Vec<TemplateLane>,
    /// Root documents (files boards).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl BoardTemplate {
    pub fn validate(&self) -> Result<()> {
        let cards: usize =
            self.lanes.iter().map(|l| l.cards.len()).sum::<usize>() + self.notes.len();
        if self.lanes.len() > MAX_LANES || cards > MAX_CARDS {
            return Err(Error::invalid("template too large"));
        }
        if self.kind == BoardKind::Files && !self.lanes.is_empty() {
            return Err(Error::invalid("files boards have no lanes"));
        }
        if self.kind == BoardKind::Kanban && !self.notes.is_empty() {
            return Err(Error::invalid("kanban templates put cards in lanes"));
        }
        let too_long = |s: &str| s.len() as u64 > MAX_TEMPLATE_BYTES;
        if self.lanes.iter().any(|l| {
            l.name.trim().is_empty() || l.name.len() > 200 || l.cards.iter().any(|c| too_long(c))
        }) || self.notes.iter().any(|c| too_long(c))
        {
            return Err(Error::invalid("invalid template content"));
        }
        Ok(())
    }

    /// Ops creating the lanes and starter cards (fresh ids).
    pub fn ops(&self, new_card: &mut dyn FnMut() -> String) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut lane_ids: Vec<String> = Vec::new();
        for l in &self.lanes {
            let k = new_id(IdKind::Lane, |id| lane_ids.iter().any(|x| x == id));
            lane_ids.push(k.clone());
            ops.push(Op::CreateLane {
                id: k.clone(),
                name: l.name.trim().to_string(),
                index: None,
            });
            for c in &l.cards {
                ops.push(Op::CreateCard {
                    id: new_card(),
                    parent: Parent::Lane(k.clone()),
                    index: None,
                    content: c.clone(),
                });
            }
        }
        for c in &self.notes {
            ops.push(Op::CreateCard {
                id: new_card(),
                parent: Parent::Root,
                index: None,
                content: c.clone(),
            });
        }
        ops
    }
}

impl Core {
    /// Card templates of a board (if given) followed by global ones.
    pub fn card_templates(&self, board: Option<&str>) -> Vec<CardTemplate> {
        let mut out = Vec::new();
        if let Some(b) = board
            && let Ok(root) = self.board_root(b)
        {
            out.extend(read_dir_templates(
                &root.join(MARKER_DIR).join("templates"),
                "board",
            ));
        }
        out.extend(read_dir_templates(
            &self.paths.data.join("templates"),
            "global",
        ));
        out
    }

    /// Create a board and fill it from `tpl` (one undo step).
    pub fn create_from_template(
        self: &Arc<Self>,
        path: &Path,
        name: &str,
        tpl: &BoardTemplate,
        git: bool,
    ) -> Result<BoardSnapshot> {
        tpl.validate()?;
        let snap = self.create_board(path, name, tpl.kind, &[], git)?;
        let id = snap.header.id.clone();
        let ops = tpl.ops(&mut || self.new_card_id());
        if ops.is_empty() {
            return Ok(snap);
        }
        self.apply(&id, Op::Batch { ops }, "New board from template", None)?;
        self.snapshot(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_templates() {
        let ok = BoardTemplate {
            kind: BoardKind::Kanban,
            lanes: vec![TemplateLane {
                name: "To do".into(),
                cards: vec!["# Hi\n".into()],
            }],
            notes: vec![],
        };
        assert!(ok.validate().is_ok());
        let mut n = 0;
        let ops = ok.ops(&mut || {
            n += 1;
            format!("c00000{n}")
        });
        assert_eq!(ops.len(), 2);
        let bad = BoardTemplate {
            kind: BoardKind::Files,
            lanes: ok.lanes.clone(),
            notes: vec![],
        };
        assert!(bad.validate().is_err());
        let blank = BoardTemplate {
            kind: BoardKind::Kanban,
            lanes: vec![TemplateLane {
                name: " ".into(),
                cards: vec![],
            }],
            notes: vec![],
        };
        assert!(blank.validate().is_err());
    }

    #[test]
    fn reads_md_templates_only() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("Bug.md"), "# Bug\n").unwrap();
        fs::write(d.path().join("x.txt"), "no").unwrap();
        fs::write(d.path().join(".hidden.md"), "no").unwrap();
        let t = read_dir_templates(d.path(), "global");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].name, "Bug");
        assert_eq!(t[0].scope, "global");
    }
}
