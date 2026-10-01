//! `impl Core` entry points for import / export (used by `src-tauri/src/exports.rs`).
//!
//! Import modes for a folder (SPEC §15, QUESTIONNAIRE L.85):
//! - **copy** (B, default): a new board is created at `dest` and filled from
//!   the interchange in one journaled batch; the source is only read.
//! - **overwrite** (A): an existing board's lanes/cards are trashed and
//!   replaced in one batch (one undo step restores everything).
//! - **inPlace** (C): a Lull board is just opened; a plain folder is opened
//!   "as is", read-only (see [`super::loose`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::archive;
use super::export;
use super::interchange::{self, Interchange, PlanOptions};
use super::vault;
use crate::app::Core;
use crate::error::{Error, Result};
use crate::ids::{IdKind, new_id};
use crate::model::*;
use crate::store::{self, Op};

pub const MAX_JSON: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// Board folder as `.zip` (without cache/trash; history optional).
    Zip,
    /// One Markdown file.
    Md,
    /// `.zip` of readable Markdown folders + attachments.
    MdBundle,
    /// Self-contained HTML.
    Html,
    /// Interchange JSON.
    Json,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    Board,
    BoardZip,
    Zip,
    Folder,
    Interchange,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspect {
    pub kind: SourceKind,
    pub name: String,
    /// Lull board id (boards only) and whether it is already registered.
    pub board_id: Option<String>,
    pub registered: bool,
    pub schema: Option<u32>,
    pub board_kind: Option<BoardKind>,
    pub lanes: usize,
    pub notes: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    #[default]
    Copy,
    Overwrite,
    InPlace,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportRequest {
    pub path: String,
    pub mode: ImportMode,
    /// New board folder (copy mode).
    pub dest: Option<String>,
    /// Board to replace (overwrite mode).
    pub target: Option<String>,
    pub name: Option<String>,
    /// Localized name of the lane receiving root notes on kanban boards.
    pub inbox_lane: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub board_id: String,
    pub cards: usize,
    pub lanes: usize,
    pub attachments: usize,
    pub warnings: Vec<String>,
}

fn is_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Content reader for a (possibly loose) board state.
fn reader(st: &BoardState) -> impl Fn(&str) -> String + '_ {
    move |id| {
        if let Some(t) = st.loose.as_ref().and_then(|l| l.synthetic.get(id)) {
            return t.clone();
        }
        st.node_file(id)
            .and_then(|p| crate::fsutil::read_to_string(&p).ok())
            .unwrap_or_default()
    }
}

/// Temporary extraction folder removed on drop.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Core {
    fn export_state(&self, board: &str) -> Result<BoardState> {
        self.flush_edits(Some(board));
        Ok(self.board(board)?.lock().state.clone())
    }

    /// Export a board (or one card subtree with `card`) to `dest`.
    pub fn io_export(
        &self,
        board: &str,
        card: Option<&str>,
        format: ExportFormat,
        dest: &Path,
        include_history: bool,
    ) -> Result<ExportResult> {
        let st = self.export_state(board)?;
        if let Some(c) = card
            && !st.nodes.contains_key(c)
        {
            return Err(Error::not_found(c));
        }
        if super::is_within(dest, &st.root) {
            return Err(Error::invalid("the export file cannot be inside the board"));
        }
        let read = reader(&st);
        let write = |text: String| -> Result<(usize, u64)> {
            crate::fsutil::atomic_write(dest, text.as_bytes())?;
            Ok((1, text.len() as u64))
        };
        let (files, bytes) = match format {
            ExportFormat::Zip if card.is_none() && st.loose.is_none() => {
                archive::zip_board(&st.root, dest, include_history)?
            }
            ExportFormat::Zip | ExportFormat::MdBundle => {
                let mut sub = st.clone();
                if let Some(c) = card {
                    // Bundle of one card: present it as the only root item.
                    sub.lanes.clear();
                    sub.root_order = vec![c.to_string()];
                }
                export::markdown_bundle(&sub, &read, dest)?
            }
            ExportFormat::Md => write(export::markdown_doc(&st, &read, card))?,
            ExportFormat::Html => write(export::html_doc(&st, &read, card))?,
            ExportFormat::Json => {
                let ix = interchange::from_state(&st, &read, card);
                write(crate::json_fmt::to_string(&ix).map_err(|e| Error::Other(e.to_string()))?)?
            }
        };
        tracing::info!("exported board {board} as {format:?} ({files} files, {bytes} bytes)");
        Ok(ExportResult {
            path: dest.to_string_lossy().into_owned(),
            files,
            bytes,
        })
    }

    /// HTML of a board or card for printing (PDF through the print dialog).
    pub fn io_render_html(&self, board: &str, card: Option<&str>) -> Result<String> {
        let st = self.export_state(board)?;
        let read = reader(&st);
        Ok(export::html_doc(&st, &read, card))
    }

    /// Detect what `path` holds before choosing an import mode.
    pub fn io_inspect(&self, path: &Path) -> Result<Inspect> {
        let base = Inspect {
            kind: SourceKind::Folder,
            name: path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            board_id: None,
            registered: false,
            schema: None,
            board_kind: None,
            lanes: 0,
            notes: 0,
            truncated: false,
        };
        if path.is_dir() {
            if let Some(m) = crate::discovery::read_marker(path) {
                let st = store::load_board(path, None)?;
                return Ok(Inspect {
                    kind: SourceKind::Board,
                    name: st.manifest.name.clone(),
                    registered: self.registry.lock().get(&m.id).is_some(),
                    board_id: Some(m.id),
                    schema: Some(st.manifest.schema),
                    board_kind: Some(st.manifest.kind),
                    lanes: st.lanes.len(),
                    notes: st.nodes.len(),
                    ..base
                });
            }
            let t = vault::scan(path)?;
            return Ok(Inspect {
                name: t.name.clone(),
                board_kind: Some(t.kind),
                lanes: t.lanes.len(),
                notes: t.notes,
                truncated: t.truncated,
                ..base
            });
        }
        if is_ext(path, "zip") {
            let board = archive::archive_has_board(path)?;
            return Ok(Inspect {
                kind: if board {
                    SourceKind::BoardZip
                } else {
                    SourceKind::Zip
                },
                ..base
            });
        }
        if is_ext(path, "json") {
            let ix = read_interchange(path)?;
            return Ok(Inspect {
                kind: SourceKind::Interchange,
                name: ix.board.name.clone(),
                board_kind: Some(ix.board.kind),
                lanes: ix.lanes.len(),
                notes: ix.cards.len(),
                ..base
            });
        }
        Err(Error::invalid("unsupported import source"))
    }

    /// Run an import (see module docs for modes).
    pub fn io_import(self: &Arc<Self>, req: &ImportRequest) -> Result<ImportResult> {
        let path = PathBuf::from(&req.path);
        if req.path.trim().is_empty() || !path.exists() {
            return Err(Error::not_found(&req.path));
        }
        if req.mode == ImportMode::InPlace {
            if !path.is_dir() {
                return Err(Error::invalid("in_place_needs_folder"));
            }
            let snap = if store::is_board(&path) {
                self.open_board(&path)?
            } else {
                self.open_loose(&path)?
            };
            return Ok(ImportResult {
                board_id: snap.header.id,
                cards: snap.nodes.len(),
                lanes: snap.lanes.len(),
                attachments: 0,
                warnings: vec![],
            });
        }
        let _tmp;
        let ix = if path.is_dir() {
            folder_interchange(&path)?
        } else if is_ext(&path, "zip") {
            let dir = self
                .paths
                .data
                .join("cache")
                .join(format!("import-{}", crate::ids::random_suffix(8)));
            _tmp = TempDir(dir.clone());
            archive::extract(&path, &dir)?;
            match archive::find_board_root(&dir) {
                Some(r) => folder_interchange(&r)?,
                None => {
                    // A Markdown bundle: its single top folder is the vault.
                    let subs: Vec<PathBuf> = std::fs::read_dir(&dir)
                        .map_err(|e| Error::io(&dir, e))?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.is_dir()
                                && !p
                                    .file_name()
                                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                        })
                        .collect();
                    let root = if subs.len() == 1 {
                        subs[0].clone()
                    } else {
                        dir.clone()
                    };
                    let mut ix = vault::to_interchange(&vault::scan(&root)?);
                    if root == dir {
                        ix.board.name = path
                            .file_stem()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                    }
                    ix
                }
            }
        } else if is_ext(&path, "json") {
            read_interchange(&path)?
        } else {
            return Err(Error::invalid("unsupported import source"));
        };
        let inbox = req
            .inbox_lane
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Inbox".into());
        match req.mode {
            ImportMode::Copy => {
                let dest = req
                    .dest
                    .as_deref()
                    .filter(|d| !d.trim().is_empty())
                    .ok_or_else(|| Error::invalid("dest required"))?;
                let dest = PathBuf::from(dest);
                if path.is_dir()
                    && (super::is_within(&dest, &path) || super::is_within(&path, &dest))
                {
                    return Err(Error::invalid(
                        "the new board cannot overlap the source folder",
                    ));
                }
                let name = req
                    .name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| ix.board.name.clone());
                let snap = self.create_board(&dest, &name, ix.board.kind, &[], false)?;
                self.apply_interchange(&snap.header.id, &ix, &inbox, false)
            }
            ImportMode::Overwrite => {
                let target = req
                    .target
                    .as_deref()
                    .ok_or_else(|| Error::invalid("target required"))?;
                if self.board(target).is_err() {
                    self.open_board_by_id(target)?;
                }
                self.apply_interchange(target, &ix, &inbox, true)
            }
            ImportMode::InPlace => unreachable!(),
        }
    }

    /// Apply an interchange to an open board as one journaled batch, then copy attachments.
    pub fn apply_interchange(
        &self,
        board: &str,
        ix: &Interchange,
        inbox: &str,
        replace: bool,
    ) -> Result<ImportResult> {
        let b = self.board(board)?;
        let (kind, existing_lanes, clear) = {
            let s = b.lock();
            let st = &s.state;
            if let Some(r) = &st.read_only {
                return Err(Error::ReadOnly(r.clone()));
            }
            let lanes: HashSet<String> = st.lanes.iter().map(|l| l.id.clone()).collect();
            let clear = Op::Trash {
                nodes: st.root_order.clone(),
                lanes: st.lanes.iter().map(|l| l.id.clone()).collect(),
            };
            (st.manifest.kind, lanes, clear)
        };
        let mut new_card = || self.new_card_id();
        let mut new_lane = || new_id(IdKind::Lane, |id| existing_lanes.contains(id));
        let plan = interchange::plan(
            ix,
            PlanOptions {
                kind,
                inbox_lane: inbox,
                new_card_id: &mut new_card,
                new_lane_id: &mut new_lane,
            },
        );
        let mut ops = Vec::new();
        if replace
            && (matches!(&clear, Op::Trash { nodes, lanes } if !nodes.is_empty() || !lanes.is_empty()))
        {
            ops.push(clear);
        }
        ops.extend(plan.ops.iter().cloned());
        if !ops.is_empty() {
            self.apply(board, Op::Batch { ops }, "Import", None)?;
        }
        let mut warnings = plan.warnings.clone();
        let mut copied = 0;
        if !plan.copies.is_empty() {
            let mut s = b.lock();
            for c in &plan.copies {
                let Some(dir) = s.state.attachment_dir(&c.card) else {
                    continue;
                };
                let dest = dir.join(&c.file);
                match super::copy_regular(&c.src, &dest) {
                    Ok(_) => {
                        s.touch(&dest);
                        copied += 1;
                    }
                    Err(_) => warnings.push(format!("missing_attachment:{}", c.file)),
                }
            }
            let ch = s.reload()?;
            self.index_changes(&mut s, &ch);
            self.emit_delta(&s, &ch);
        }
        tracing::info!(
            "imported into {board}: {} cards, {} lanes, {copied} files",
            plan.cards,
            plan.lanes
        );
        Ok(ImportResult {
            board_id: board.to_string(),
            cards: plan.cards,
            lanes: plan.lanes,
            attachments: copied,
            warnings,
        })
    }
}

/// Interchange of a folder: a Lull board (any schema we can read) or a plain folder.
pub fn folder_interchange(path: &Path) -> Result<Interchange> {
    if store::is_board(path) {
        let st = store::load_board(path, None)?;
        let read = reader(&st);
        Ok(interchange::from_state(&st, &read, None))
    } else {
        Ok(vault::to_interchange(&vault::scan(path)?))
    }
}

/// Read an interchange JSON. Attachments resolve inside `sourceRoot` only when
/// it is a Lull board folder, else next to the JSON file (a crafted
/// `sourceRoot` could otherwise pull arbitrary files into a board).
pub fn read_interchange(path: &Path) -> Result<Interchange> {
    let text = archive::read_capped(path, MAX_JSON)?;
    let mut ix: Interchange =
        serde_json::from_str(&text).map_err(|e| Error::invalid(format!("interchange: {e}")))?;
    ix.validate()?;
    let trusted = ix
        .source_root
        .as_deref()
        .is_some_and(|r| Path::new(r).is_absolute() && store::is_board(Path::new(r)));
    if !trusted {
        ix.source_root = path.parent().map(|p| p.to_string_lossy().into_owned());
    }
    Ok(ix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppPaths, NullSink};
    use std::fs;

    fn core(d: &Path) -> Arc<Core> {
        Core::new(AppPaths::under(&d.join("app")), Arc::new(NullSink)).unwrap()
    }

    fn titles(core: &Core, board: &str) -> Vec<(String, String)> {
        let b = core.board(board).unwrap();
        let s = b.lock();
        let mut v: Vec<(String, String)> = s
            .state
            .lanes
            .iter()
            .flat_map(|l| {
                let st = &s.state;
                l.order
                    .iter()
                    .map(move |id| (l.name.clone(), st.nodes[id].meta.title.clone()))
            })
            .collect();
        v.sort();
        v
    }

    fn sample(core: &Arc<Core>, d: &Path) -> String {
        let snap = core
            .create_board(
                &d.join("Src"),
                "Src",
                BoardKind::Kanban,
                &["Todo".into(), "Done".into()],
                false,
            )
            .unwrap();
        let id = snap.header.id.clone();
        let (todo, done) = (snap.lanes[0].id.clone(), snap.lanes[1].id.clone());
        let a = core.new_card_id();
        let b = core.new_card_id();
        let c = core.new_card_id();
        core.apply(
            &id,
            Op::Batch {
                ops: vec![
                    Op::CreateCard {
                        id: a.clone(),
                        parent: Parent::Lane(todo.clone()),
                        index: None,
                        content: "# Alpha\n\n#work see [[B]]\n".replace("B", &b),
                    },
                    Op::CreateCard {
                        id: b.clone(),
                        parent: Parent::Lane(done),
                        index: None,
                        content: "# Beta\n".into(),
                    },
                    Op::CreateCard {
                        id: c,
                        parent: Parent::Card(a.clone()),
                        index: None,
                        content: "# Child\n".into(),
                    },
                ],
            },
            "seed",
            None,
        )
        .unwrap();
        let att = core
            .add_attachment(&id, &a, crate::app::files::Source::Bytes(b"PNG"), "pic.png")
            .unwrap();
        let body = format!("# Alpha\n\n#work see [[{b}]]\n\n![pic]({})\n", att.file);
        core.apply(
            &id,
            Op::WriteCard {
                id: a.clone(),
                content: body,
            },
            "edit",
            None,
        )
        .unwrap();
        core.flush_edits(None);
        id
    }

    #[test]
    fn export_import_round_trips_through_zip_json_and_bundle() {
        let d = tempfile::tempdir().unwrap();
        let core = core(d.path());
        let src = sample(&core, d.path());
        let expect = titles(&core, &src);
        for (fmt, file) in [
            (ExportFormat::Zip, "b.zip"),
            (ExportFormat::Json, "b.json"),
            (ExportFormat::MdBundle, "md.zip"),
        ] {
            let out = d.path().join("out").join(file);
            fs::create_dir_all(out.parent().unwrap()).unwrap();
            core.io_export(&src, None, fmt, &out, false).unwrap();
            let r = core
                .io_import(&ImportRequest {
                    path: out.to_string_lossy().into(),
                    dest: Some(
                        d.path()
                            .join(format!("Copy-{file}"))
                            .to_string_lossy()
                            .into(),
                    ),
                    ..Default::default()
                })
                .unwrap();
            assert_ne!(r.board_id, src);
            assert_eq!(r.cards, 3, "{fmt:?}");
            assert_eq!(titles(&core, &r.board_id), expect, "{fmt:?}");
            let b = core.board(&r.board_id).unwrap();
            let s = b.lock();
            let alpha = s
                .state
                .nodes
                .values()
                .find(|n| n.meta.title == "Alpha")
                .unwrap();
            assert!(alpha.is_group, "{fmt:?}");
            assert_eq!(alpha.children.len(), 1);
            assert_eq!(alpha.attachments.len(), 1, "{fmt:?}");
            let beta = s
                .state
                .nodes
                .values()
                .find(|n| n.meta.title == "Beta")
                .unwrap();
            let body = s.read_content(&alpha.id).unwrap();
            assert!(
                body.contains(&format!("[[{}]]", beta.id)),
                "{fmt:?}: {body}"
            );
            assert!(alpha.meta.tags.contains(&"work".to_string()));
        }
        // The source board is untouched.
        assert_eq!(titles(&core, &src), expect);
    }

    #[test]
    fn overwrite_replaces_in_one_undo_step() {
        let d = tempfile::tempdir().unwrap();
        let core = core(d.path());
        let src = sample(&core, d.path());
        let other = core
            .create_board(
                &d.path().join("Other"),
                "Other",
                BoardKind::Kanban,
                &["Old".into()],
                false,
            )
            .unwrap();
        let ix = folder_interchange(&d.path().join("Src")).unwrap();
        core.apply_interchange(&other.header.id, &ix, "Inbox", true)
            .unwrap();
        assert_eq!(titles(&core, &other.header.id), titles(&core, &src));
        core.undo(&other.header.id).unwrap();
        let b = core.board(&other.header.id).unwrap();
        let names: Vec<String> = b
            .lock()
            .state
            .lanes
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, vec!["Old"]);
    }

    #[test]
    fn foreign_folder_copy_and_in_place() {
        let d = tempfile::tempdir().unwrap();
        let core = core(d.path());
        let v = d.path().join("Vault");
        fs::create_dir_all(v.join("Ideas")).unwrap();
        fs::create_dir_all(v.join("img")).unwrap();
        fs::write(v.join("Ideas/One.md"), "First line\n![x](../img/p.png)\n").unwrap();
        fs::write(v.join("img/p.png"), "PNG").unwrap();
        fs::write(v.join("Loose.md"), "# Loose note\n").unwrap();
        let info = core.io_inspect(&v).unwrap();
        assert_eq!(info.kind, SourceKind::Folder);
        assert_eq!((info.lanes, info.notes), (1, 2));

        let r = core
            .io_import(&ImportRequest {
                path: v.to_string_lossy().into(),
                dest: Some(d.path().join("Imported").to_string_lossy().into()),
                inbox_lane: Some("Bandeja".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!((r.cards, r.lanes, r.attachments), (2, 2, 1));
        let t = titles(&core, &r.board_id);
        assert_eq!(
            t,
            vec![
                ("Bandeja".into(), "Loose note".into()),
                ("Ideas".into(), "One".into())
            ]
        );
        // Originals untouched, no marker created.
        assert_eq!(
            fs::read_to_string(v.join("Ideas/One.md")).unwrap(),
            "First line\n![x](../img/p.png)\n"
        );
        assert!(!v.join(crate::brand::MARKER_DIR).exists());

        let r = core
            .io_import(&ImportRequest {
                path: v.to_string_lossy().into(),
                mode: ImportMode::InPlace,
                ..Default::default()
            })
            .unwrap();
        let snap = core.snapshot(&r.board_id).unwrap();
        assert!(snap.header.read_only.is_some());
        assert!(!v.join(crate::brand::MARKER_DIR).exists());
        assert!(core.board_file(&r.board_id, "img/p.png").is_ok());
        assert!(core.board_file(&r.board_id, "../x").is_err());
    }

    #[test]
    fn exports_markdown_and_html() {
        let d = tempfile::tempdir().unwrap();
        let core = core(d.path());
        let src = sample(&core, d.path());
        let md = d.path().join("b.md");
        core.io_export(&src, None, ExportFormat::Md, &md, false)
            .unwrap();
        let text = fs::read_to_string(&md).unwrap();
        assert!(text.starts_with("# Src\n\n## Todo\n\n### Alpha"), "{text}");
        assert!(text.contains("see Beta"), "links become titles: {text}");
        assert!(text.contains("#### Child"));
        let html = core.io_render_html(&src, None).unwrap();
        assert!(html.contains("<h2>Todo</h2>") && html.contains("Alpha"));
        assert!(html.contains("class=\"card-link\" href=\"#card-"));
        assert!(!html.contains("<script"));
        assert!(
            html.contains("src=\"data:image/png;base64,"),
            "images are embedded"
        );
        assert!(
            core.io_export(
                &src,
                None,
                ExportFormat::Html,
                &d.path().join("Src/inside.html"),
                false
            )
            .is_err()
        );
    }
}
