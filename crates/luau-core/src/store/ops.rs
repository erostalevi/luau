//! Invertible board operations.
//!
//! Every mutation is an [`Op`]; applying one returns its inverse, which powers
//! undo/redo. Group conversion is automatic: placing a card into a plain card
//! turns it into a group; emptying a group turns it back into a plain card.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{BoardStore, Changes, list_attachments, parse_attachment_name, trash};
use crate::brand::{INDEX_JSON, INDEX_MD};
use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, move_path, remove_path};
use crate::ids::{IdKind, is_id};
use crate::markdown;
use crate::model::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub id: String,
    pub parent: Parent,
    pub index: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LanePatch {
    pub name: Option<String>,
    /// `Some("")` clears the color.
    pub color: Option<String>,
    /// `Some(0)` clears the width.
    pub width: Option<u32>,
    /// `Some(0)` clears the WIP limit.
    pub wip: Option<u32>,
    pub collapsed: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BoardPatch {
    pub name: Option<String>,
    pub view: Option<ViewSettings>,
    pub tag_colors: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum Op {
    #[serde(rename_all = "camelCase")]
    CreateCard {
        id: String,
        parent: Parent,
        index: Option<usize>,
        content: String,
    },
    #[serde(rename_all = "camelCase")]
    WriteCard { id: String, content: String },
    #[serde(rename_all = "camelCase")]
    Move {
        ids: Vec<String>,
        to: Parent,
        before: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Place { items: Vec<Placement> },
    #[serde(rename_all = "camelCase")]
    Trash {
        nodes: Vec<String>,
        lanes: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Restore { entries: Vec<String> },
    #[serde(rename_all = "camelCase")]
    SetArchived {
        nodes: Vec<(String, bool)>,
        lanes: Vec<(String, bool)>,
    },
    #[serde(rename_all = "camelCase")]
    CreateLane {
        id: String,
        name: String,
        index: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    UpdateLane { id: String, patch: LanePatch },
    #[serde(rename_all = "camelCase")]
    MoveLane { id: String, index: usize },
    #[serde(rename_all = "camelCase")]
    UpdateBoard { patch: BoardPatch },
    #[serde(rename_all = "camelCase")]
    SetCover { id: String, cover: Option<Cover> },
    #[serde(rename_all = "camelCase")]
    SetKind { kind: BoardKind },
    #[serde(rename_all = "camelCase")]
    Batch { ops: Vec<Op> },
    /// Handled by the application layer (e.g. cross-board moves); a store refuses it.
    #[serde(rename_all = "camelCase")]
    External { token: String },
}

impl Op {
    /// Short machine name used by the journal.
    pub fn kind(&self) -> &'static str {
        match self {
            Op::CreateCard { .. } => "createCard",
            Op::WriteCard { .. } => "writeCard",
            Op::Move { .. } => "move",
            Op::Place { .. } => "place",
            Op::Trash { .. } => "trash",
            Op::Restore { .. } => "restore",
            Op::SetArchived { .. } => "setArchived",
            Op::CreateLane { .. } => "createLane",
            Op::UpdateLane { .. } => "updateLane",
            Op::MoveLane { .. } => "moveLane",
            Op::UpdateBoard { .. } => "updateBoard",
            Op::SetCover { .. } => "setCover",
            Op::SetKind { .. } => "setKind",
            Op::Batch { .. } => "batch",
            Op::External { .. } => "external",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Applied {
    pub inverse: Op,
    pub changes: Changes,
    pub changes_anything: bool,
    /// Ids created by the op (cards and lanes).
    pub created: Vec<String>,
    /// Trash entries created by the op.
    pub trashed: Vec<String>,
}

#[derive(Default)]
struct Acc {
    ch: Changes,
    created: Vec<String>,
    trashed: Vec<String>,
    touched_anything: bool,
}

impl BoardStore {
    pub fn apply(&mut self, op: Op) -> Result<Applied> {
        if let Some(r) = &self.state.read_only {
            return Err(Error::ReadOnly(r.clone()));
        }
        let mut acc = Acc::default();
        let inverse = self.apply_inner(op, &mut acc)?;
        self.state.version += 1;
        Ok(Applied {
            inverse,
            changes_anything: acc.touched_anything,
            changes: acc.ch,
            created: acc.created,
            trashed: acc.trashed,
        })
    }

    fn apply_inner(&mut self, op: Op, acc: &mut Acc) -> Result<Op> {
        match op {
            Op::CreateCard {
                id,
                parent,
                index,
                content,
            } => self.op_create_card(id, parent, index, content, acc),
            Op::WriteCard { id, content } => self.op_write_card(id, content, acc),
            Op::Move { ids, to, before } => self.op_move(ids, to, before, acc),
            Op::Place { items } => self.relocate(items, acc),
            Op::Trash { nodes, lanes } => self.op_trash(nodes, lanes, acc),
            Op::Restore { entries } => self.op_restore(entries, acc),
            Op::SetArchived { nodes, lanes } => self.op_set_archived(nodes, lanes, acc),
            Op::CreateLane { id, name, index } => self.op_create_lane(id, name, index, acc),
            Op::UpdateLane { id, patch } => self.op_update_lane(id, patch, acc),
            Op::MoveLane { id, index } => self.op_move_lane(id, index, acc),
            Op::UpdateBoard { patch } => self.op_update_board(patch, acc),
            Op::SetCover { id, cover } => self.op_set_cover(id, cover, acc),
            Op::SetKind { kind } => self.op_set_kind(kind, acc),
            Op::External { .. } => Err(Error::invalid(
                "external operation must be handled by the app layer",
            )),
            Op::Batch { ops } => {
                let mut inverses = Vec::new();
                for o in ops {
                    match self.apply_inner(o, acc) {
                        Ok(inv) => inverses.push(inv),
                        Err(e) => {
                            // Best-effort rollback of what already happened.
                            for inv in inverses.into_iter().rev() {
                                let mut scratch = Acc::default();
                                if self.apply_inner(inv, &mut scratch).is_ok() {
                                    acc.ch.merge(scratch.ch);
                                }
                            }
                            return Err(e);
                        }
                    }
                }
                inverses.reverse();
                Ok(Op::Batch { ops: inverses })
            }
        }
    }

    // --- validation --------------------------------------------------------

    /// Existence check only. Board-kind rules (lanes on kanban, root on files)
    /// are enforced by the application layer, because conversions pass through
    /// intermediate states inside a single batch (and its inverse).
    fn validate_parent(&self, p: &Parent) -> Result<()> {
        let s = &self.state;
        match p {
            Parent::Lane(k) => s
                .lane(k)
                .map(|_| ())
                .ok_or_else(|| Error::not_found(k.clone())),
            Parent::Root => Ok(()),
            Parent::Card(c) => s
                .nodes
                .get(c)
                .map(|_| ())
                .ok_or_else(|| Error::not_found(c.clone())),
        }
    }

    /// Drop unknown ids, duplicates, and ids whose ancestor is also listed.
    fn normalize_ids(&self, ids: Vec<String>) -> Vec<String> {
        let mut seen = HashSet::new();
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|i| self.state.nodes.contains_key(i) && seen.insert(i.clone()))
            .collect();
        ids.iter()
            .filter(|id| {
                !ids.iter()
                    .any(|other| other != *id && self.state.is_ancestor(other, id))
            })
            .cloned()
            .collect()
    }

    // --- group conversion ------------------------------------------------------

    pub(crate) fn ensure_group(&mut self, c: &str, ch: &mut Changes) -> Result<()> {
        let n = self.state.nodes.get(c).ok_or_else(|| Error::not_found(c))?;
        if n.is_group {
            return Ok(());
        }
        let dir = self
            .state
            .container_dir(&n.parent.clone())
            .ok_or_else(|| Error::not_found(c))?;
        let gdir = dir.join(c);
        fs::create_dir_all(&gdir).map_err(|e| Error::io(&gdir, e))?;
        let md = dir.join(format!("{c}.md"));
        if md.exists() {
            move_path(&md, &gdir.join(INDEX_MD))?;
        } else {
            atomic_write(&gdir.join(INDEX_MD), b"# \n")?;
        }
        for a in list_attachments(&dir, c) {
            move_path(&dir.join(&a.file), &gdir.join(&a.file))?;
        }
        self.touch(&md);
        self.touch(&gdir);
        self.state.nodes.get_mut(c).unwrap().is_group = true;
        self.save_container(&Parent::Card(c.to_string()))?;
        ch.nodes.insert(c.to_string());
        Ok(())
    }

    pub(crate) fn ensure_plain(&mut self, g: &str, ch: &mut Changes) -> Result<()> {
        let n = self.state.nodes.get(g).ok_or_else(|| Error::not_found(g))?;
        if !n.is_group || !n.children.is_empty() {
            return Ok(());
        }
        let parent = n.parent.clone();
        let dir = self
            .state
            .container_dir(&parent)
            .ok_or_else(|| Error::not_found(g))?;
        let gdir = dir.join(g);
        let md = dir.join(format!("{g}.md"));
        let index_md = gdir.join(INDEX_MD);
        if index_md.exists() {
            move_path(&index_md, &md)?;
        } else {
            atomic_write(&md, b"# \n")?;
        }
        for a in list_attachments(&gdir, g) {
            move_path(&gdir.join(&a.file), &dir.join(&a.file))?;
        }
        remove_path(&gdir.join(INDEX_JSON))?;
        // Anything unknown left inside is preserved in the trash, never deleted.
        let leftovers: Vec<_> = fs::read_dir(&gdir)
            .map(|r| r.flatten().collect())
            .unwrap_or_default();
        if !leftovers.is_empty() {
            let dest = trash::orphans_dir(&self.state.root).join(format!(
                "{g}-{}",
                chrono::Utc::now().format("%Y%m%dT%H%M%S%3f")
            ));
            move_path(&gdir, &dest)?;
        } else {
            let _ = fs::remove_dir(&gdir);
        }
        self.touch(&gdir);
        self.touch(&md);
        self.state.nodes.get_mut(g).unwrap().is_group = false;
        ch.nodes.insert(g.to_string());
        Ok(())
    }

    fn move_node_files(&mut self, id: &str, is_group: bool, from: &Path, to: &Path) -> Result<()> {
        if from == to {
            return Ok(());
        }
        if is_group {
            move_path(&from.join(id), &to.join(id))?;
            self.touch(&from.join(id));
            self.touch(&to.join(id));
        } else {
            let names: Vec<String> = fs::read_dir(from)
                .map_err(|e| Error::io(from, e))?
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| {
                    n == &format!("{id}.md")
                        || parse_attachment_name(n).is_some_and(|(o, _)| o == id)
                })
                .collect();
            for n in names {
                move_path(&from.join(&n), &to.join(&n))?;
                self.touch(&from.join(&n));
                self.touch(&to.join(&n));
            }
        }
        Ok(())
    }

    // --- cards -------------------------------------------------------------------

    fn op_create_card(
        &mut self,
        id: String,
        parent: Parent,
        index: Option<usize>,
        content: String,
        acc: &mut Acc,
    ) -> Result<Op> {
        if !is_id(&id, IdKind::Card) {
            return Err(Error::invalid(format!("bad card id {id}")));
        }
        if self.state.nodes.contains_key(&id) {
            return Err(Error::Conflict(format!("card exists: {id}")));
        }
        self.validate_parent(&parent)?;
        if let Parent::Card(c) = &parent {
            self.ensure_group(c, &mut acc.ch)?;
        }
        let dir = self
            .state
            .container_dir(&parent)
            .ok_or_else(|| Error::invalid("bad parent"))?;
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let path = dir.join(format!("{id}.md"));
        if path.exists() {
            return Err(Error::Conflict(format!("file exists: {}", path.display())));
        }
        let normalized = markdown::normalize(&content);
        atomic_write(&path, normalized.as_bytes())?;
        self.touch(&path);
        self.state.nodes.insert(
            id.clone(),
            Node {
                id: id.clone(),
                parent: parent.clone(),
                is_group: false,
                children: vec![],
                archived: false,
                cover: None,
                meta: markdown::ParsedCard::default(),
                attachments: vec![],
                mtime: 0,
                size: 0,
                hash: String::new(),
                group_extra: Default::default(),
            },
        );
        self.refresh_node_meta(&id, &normalized)?;
        let list = self.state.children_of_mut(&parent).unwrap();
        let at = index.unwrap_or(list.len()).min(list.len());
        list.insert(at, id.clone());
        self.save_container(&parent)?;
        acc.ch.nodes.insert(id.clone());
        acc.ch.touch_parent(&parent);
        acc.created.push(id.clone());
        acc.touched_anything = true;
        Ok(Op::Trash {
            nodes: vec![id],
            lanes: vec![],
        })
    }

    fn op_write_card(&mut self, id: String, content: String, acc: &mut Acc) -> Result<Op> {
        if !self.state.nodes.contains_key(&id) {
            return Err(Error::not_found(id));
        }
        let prev = self.read_content(&id)?;
        let next = markdown::normalize(&content);
        if prev == next {
            return Ok(Op::WriteCard { id, content: prev });
        }
        self.write_node_content(&id, &next)?;
        self.rescan_attachments(&id);
        acc.ch.nodes.insert(id.clone());
        acc.touched_anything = true;
        Ok(Op::WriteCard { id, content: prev })
    }

    fn op_move(
        &mut self,
        ids: Vec<String>,
        to: Parent,
        before: Option<String>,
        acc: &mut Acc,
    ) -> Result<Op> {
        let ids = self.normalize_ids(ids);
        if ids.is_empty() {
            return Ok(Op::Place { items: vec![] });
        }
        self.validate_parent(&to)?;
        let siblings: Vec<String> = self
            .state
            .children_of(&to)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| !ids.contains(c))
            .collect();
        let base = before
            .as_ref()
            .and_then(|b| siblings.iter().position(|c| c == b))
            .unwrap_or(siblings.len());
        let items = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Placement {
                id: id.clone(),
                parent: to.clone(),
                index: base + i,
            })
            .collect();
        self.relocate(items, acc)
    }

    /// Place each item at `(parent, index)`. Returns the inverse `Place`.
    fn relocate(&mut self, items: Vec<Placement>, acc: &mut Acc) -> Result<Op> {
        let keep: HashSet<String> = self
            .normalize_ids(items.iter().map(|i| i.id.clone()).collect())
            .into_iter()
            .collect();
        let items: Vec<Placement> = items.into_iter().filter(|i| keep.contains(&i.id)).collect();
        if items.is_empty() {
            return Ok(Op::Place { items: vec![] });
        }
        // Validate targets and cycles up front.
        for it in &items {
            self.validate_parent(&it.parent)?;
            if let Parent::Card(c) = &it.parent {
                for other in &items {
                    if self.state.is_ancestor(&other.id, c) {
                        return Err(Error::invalid(format!(
                            "cannot move {} into itself",
                            other.id
                        )));
                    }
                }
            }
        }
        let original: Vec<Placement> = items
            .iter()
            .filter_map(|it| {
                self.state.position_of(&it.id).map(|(p, i)| Placement {
                    id: it.id.clone(),
                    parent: p,
                    index: i,
                })
            })
            .collect();
        // No-op detection.
        if original.len() == items.len()
            && items
                .iter()
                .zip(&original)
                .all(|(a, b)| a.parent == b.parent && a.index == b.index)
        {
            return Ok(Op::Place { items: original });
        }
        let targets: BTreeSet<String> = items
            .iter()
            .filter_map(|i| {
                if let Parent::Card(c) = &i.parent {
                    Some(c.clone())
                } else {
                    None
                }
            })
            .collect();
        for c in &targets {
            self.ensure_group(c, &mut acc.ch)?;
        }
        // Detach everything (compute old dirs before mutating parents).
        let mut old_parents: Vec<Parent> = Vec::new();
        let mut moves = Vec::new();
        for it in &items {
            let n = self.state.nodes.get(&it.id).unwrap();
            let old_parent = n.parent.clone();
            let is_group = n.is_group;
            let old_dir = self
                .state
                .container_dir(&old_parent)
                .ok_or_else(|| Error::not_found(it.id.clone()))?;
            if let Some(list) = self.state.children_of_mut(&old_parent) {
                list.retain(|c| c != &it.id);
            }
            if !old_parents.contains(&old_parent) {
                old_parents.push(old_parent);
            }
            moves.push((it.id.clone(), is_group, old_dir));
        }
        for ((id, is_group, old_dir), it) in moves.into_iter().zip(&items) {
            self.state.nodes.get_mut(&id).unwrap().parent = it.parent.clone();
            let new_dir = self
                .state
                .container_dir(&it.parent)
                .ok_or_else(|| Error::not_found(id.clone()))?;
            self.move_node_files(&id, is_group, &old_dir, &new_dir)?;
            acc.ch.nodes.insert(id);
        }
        // Insert in ascending index order per parent so indices are stable.
        let mut sorted: Vec<&Placement> = items.iter().collect();
        sorted.sort_by_key(|p| p.index);
        for it in sorted {
            let list = self.state.children_of_mut(&it.parent).unwrap();
            let at = it.index.min(list.len());
            list.insert(at, it.id.clone());
        }
        let mut to_save: Vec<Parent> = Vec::new();
        for p in old_parents.iter().chain(items.iter().map(|i| &i.parent)) {
            if !to_save.contains(p) {
                to_save.push(p.clone());
            }
        }
        for p in &old_parents {
            if let Parent::Card(g) = p
                && !targets.contains(g)
                && self
                    .state
                    .nodes
                    .get(g)
                    .is_some_and(|n| n.children.is_empty())
            {
                self.ensure_plain(g, &mut acc.ch)?;
            }
        }
        for p in &to_save {
            self.save_container(p)?;
            acc.ch.touch_parent(p);
        }
        acc.touched_anything = true;
        Ok(Op::Place { items: original })
    }

    // --- trash -------------------------------------------------------------------

    fn op_trash(&mut self, nodes: Vec<String>, lanes: Vec<String>, acc: &mut Acc) -> Result<Op> {
        let mut entries = Vec::new();
        for id in self.normalize_ids(nodes) {
            let Some((parent, _)) = self.state.position_of(&id) else {
                continue;
            };
            let removed = self.state.descendants(&id);
            let tid = trash::trash_node(self, &id)?;
            acc.ch.removed.insert(id.clone());
            acc.ch.removed.extend(removed);
            acc.ch.touch_parent(&parent);
            if let Parent::Card(g) = &parent
                && self
                    .state
                    .nodes
                    .get(g)
                    .is_some_and(|n| n.children.is_empty())
            {
                self.ensure_plain(g, &mut acc.ch)?;
            }
            self.save_container(&parent)?;
            entries.push(tid);
        }
        for k in lanes {
            if self.state.lane(&k).is_none() {
                continue;
            }
            let removed: Vec<String> = self
                .state
                .lane(&k)
                .unwrap()
                .order
                .iter()
                .flat_map(|c| {
                    let mut v = self.state.descendants(c);
                    v.push(c.clone());
                    v
                })
                .collect();
            let tid = trash::trash_lane(self, &k)?;
            acc.ch.removed.extend(removed);
            acc.ch.lanes = true;
            self.save_manifest()?;
            entries.push(tid);
        }
        if !entries.is_empty() {
            acc.touched_anything = true;
        }
        acc.trashed.extend(entries.iter().cloned());
        entries.reverse();
        Ok(Op::Restore { entries })
    }

    fn op_restore(&mut self, entries: Vec<String>, acc: &mut Acc) -> Result<Op> {
        let mut nodes = Vec::new();
        let mut lanes = Vec::new();
        for tid in entries {
            match trash::restore(self, &tid, &mut acc.ch)? {
                trash::Restored::Node(id) => nodes.push(id),
                trash::Restored::Lane(k) => lanes.push(k),
            }
        }
        acc.touched_anything |= !nodes.is_empty() || !lanes.is_empty();
        nodes.reverse();
        lanes.reverse();
        Ok(Op::Trash { nodes, lanes })
    }

    // --- flags & lanes -------------------------------------------------------------

    fn op_set_archived(
        &mut self,
        nodes: Vec<(String, bool)>,
        lanes: Vec<(String, bool)>,
        acc: &mut Acc,
    ) -> Result<Op> {
        let mut inv_nodes = Vec::new();
        let mut inv_lanes = Vec::new();
        let mut parents: Vec<Parent> = Vec::new();
        for (id, flag) in nodes {
            let Some(n) = self.state.nodes.get_mut(&id) else {
                continue;
            };
            if n.archived != flag {
                inv_nodes.push((id.clone(), n.archived));
                n.archived = flag;
                if !parents.contains(&n.parent) {
                    parents.push(n.parent.clone());
                }
                acc.ch.nodes.insert(id);
            }
        }
        for (k, flag) in lanes {
            let Some(l) = self.state.lane_mut(&k) else {
                continue;
            };
            if l.archived != flag {
                inv_lanes.push((k, l.archived));
                l.archived = flag;
                acc.ch.lanes = true;
                acc.ch.reindex_lanes = true;
            }
        }
        for p in &parents {
            self.save_container(p)?;
        }
        if !inv_lanes.is_empty() {
            self.save_manifest()?;
        }
        acc.touched_anything |= !inv_nodes.is_empty() || !inv_lanes.is_empty();
        Ok(Op::SetArchived {
            nodes: inv_nodes,
            lanes: inv_lanes,
        })
    }

    fn op_create_lane(
        &mut self,
        id: String,
        name: String,
        index: Option<usize>,
        acc: &mut Acc,
    ) -> Result<Op> {
        if self.state.manifest.kind != BoardKind::Kanban {
            return Err(Error::invalid("lanes only exist on kanban boards"));
        }
        if !is_id(&id, IdKind::Lane) || self.state.lane(&id).is_some() {
            return Err(Error::invalid(format!("bad lane id {id}")));
        }
        let dir = self.state.root.join(&id);
        if dir.exists() {
            return Err(Error::Conflict(format!(
                "lane dir exists: {}",
                dir.display()
            )));
        }
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        self.touch(&dir);
        let lane = Lane {
            id: id.clone(),
            name: name.trim().to_string(),
            order: vec![],
            color: None,
            width: None,
            wip: None,
            collapsed: false,
            archived: false,
            extra: Default::default(),
        };
        let at = index
            .unwrap_or(self.state.lanes.len())
            .min(self.state.lanes.len());
        self.state.lanes.insert(at, lane);
        self.save_container(&Parent::Lane(id.clone()))?;
        self.save_manifest()?;
        acc.ch.lanes = true;
        acc.created.push(id.clone());
        acc.touched_anything = true;
        Ok(Op::Trash {
            nodes: vec![],
            lanes: vec![id],
        })
    }

    fn op_update_lane(&mut self, id: String, patch: LanePatch, acc: &mut Acc) -> Result<Op> {
        let l = self
            .state
            .lane_mut(&id)
            .ok_or_else(|| Error::not_found(id.clone()))?;
        let mut inv = LanePatch::default();
        if let Some(n) = patch.name {
            let n = n.trim().to_string();
            if n != l.name {
                inv.name = Some(std::mem::replace(&mut l.name, n));
            }
        }
        if let Some(c) = patch.color {
            let next = (!c.is_empty()).then_some(c);
            if next != l.color {
                inv.color = Some(l.color.clone().unwrap_or_default());
                l.color = next;
            }
        }
        if let Some(w) = patch.width {
            let next = (w > 0).then_some(w);
            if next != l.width {
                inv.width = Some(l.width.unwrap_or(0));
                l.width = next;
            }
        }
        if let Some(w) = patch.wip {
            let next = (w > 0).then_some(w);
            if next != l.wip {
                inv.wip = Some(l.wip.unwrap_or(0));
                l.wip = next;
            }
        }
        if let Some(c) = patch.collapsed
            && c != l.collapsed
        {
            inv.collapsed = Some(l.collapsed);
            l.collapsed = c;
        }
        if inv != LanePatch::default() {
            self.save_container(&Parent::Lane(id.clone()))?;
            acc.ch.lanes = true;
            acc.ch.reindex_lanes = true;
            acc.touched_anything = true;
        }
        Ok(Op::UpdateLane { id, patch: inv })
    }

    fn op_move_lane(&mut self, id: String, index: usize, acc: &mut Acc) -> Result<Op> {
        let from = self
            .state
            .lanes
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| Error::not_found(id.clone()))?;
        let to = index.min(self.state.lanes.len() - 1);
        if from != to {
            let l = self.state.lanes.remove(from);
            self.state.lanes.insert(to, l);
            self.save_manifest()?;
            acc.ch.lanes = true;
            acc.touched_anything = true;
        }
        Ok(Op::MoveLane { id, index: from })
    }

    fn op_update_board(&mut self, patch: BoardPatch, acc: &mut Acc) -> Result<Op> {
        let m = &mut self.state.manifest;
        let mut inv = BoardPatch::default();
        if let Some(n) = patch.name {
            let n = n.trim().to_string();
            if !n.is_empty() && n != m.name {
                inv.name = Some(std::mem::replace(&mut m.name, n));
            }
        }
        if let Some(v) = patch.view
            && v != m.view
        {
            inv.view = Some(std::mem::replace(&mut m.view, v));
        }
        if let Some(t) = patch.tag_colors
            && t != m.tag_colors
        {
            inv.tag_colors = Some(std::mem::replace(&mut m.tag_colors, t));
        }
        if inv != BoardPatch::default() {
            self.save_manifest()?;
            acc.ch.header = true;
            acc.touched_anything = true;
        }
        Ok(Op::UpdateBoard { patch: inv })
    }

    fn op_set_cover(&mut self, id: String, cover: Option<Cover>, acc: &mut Acc) -> Result<Op> {
        let n = self
            .state
            .nodes
            .get_mut(&id)
            .ok_or_else(|| Error::not_found(id.clone()))?;
        let prev = n.cover.clone();
        if prev != cover {
            n.cover = cover;
            let p = n.parent.clone();
            self.save_container(&p)?;
            acc.ch.nodes.insert(id.clone());
            acc.touched_anything = true;
        }
        Ok(Op::SetCover { id, cover: prev })
    }

    fn op_set_kind(&mut self, kind: BoardKind, acc: &mut Acc) -> Result<Op> {
        let prev = self.state.manifest.kind;
        if prev != kind {
            self.state.manifest.kind = kind;
            self.save_manifest()?;
            acc.ch.header = true;
            acc.ch.lanes = true;
            acc.ch.reindex_lanes = true;
            acc.ch.root = true;
            acc.touched_anything = true;
        }
        Ok(Op::SetKind { kind: prev })
    }
}
