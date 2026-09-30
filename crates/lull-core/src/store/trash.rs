//! Trash: deleted cards and lanes move to `.lull/trash/<tid>/payload/` with an
//! `entry.json` describing where they came from. Nothing is hard-deleted until
//! the retention window passes (default 7 days).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    BoardStore, Changes, list_attachments, load_node_tree, marker_dir, parse_attachment_name,
    write_json,
};
use crate::brand::INDEX_JSON;
use crate::error::{Error, Result};
use crate::fsutil::{move_path, remove_path};
use crate::ids::{IdKind, new_id};
use crate::model::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrashKind {
    Node,
    Lane,
    Orphan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub kind: TrashKind,
    pub item_id: String,
    pub title: String,
    pub parent: Option<Parent>,
    pub index: usize,
    pub is_group: bool,
    /// Number of cards inside (including the item itself for nodes).
    pub count: usize,
    pub deleted_at: String,
    pub board_id: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub cover: Option<Cover>,
}

pub enum Restored {
    Node(String),
    Lane(String),
}

pub fn trash_root(root: &Path) -> PathBuf {
    marker_dir(root).join("trash")
}

pub fn orphans_dir(root: &Path) -> PathBuf {
    trash_root(root).join("orphans")
}

fn entry_dir(root: &Path, tid: &str) -> PathBuf {
    trash_root(root).join(tid)
}

fn new_tid(root: &Path) -> String {
    new_id(IdKind::Trash, |t| entry_dir(root, t).exists())
}

pub(crate) fn trash_node(store: &mut BoardStore, id: &str) -> Result<String> {
    let s = &store.state;
    let n = s.nodes.get(id).ok_or_else(|| Error::not_found(id))?.clone();
    let (parent, index) = s.position_of(id).ok_or_else(|| Error::not_found(id))?;
    let dir = s
        .container_dir(&parent)
        .ok_or_else(|| Error::not_found(id))?;
    let descendants = s.descendants(id);
    let root = s.root.clone();
    let tid = new_tid(&root);
    let payload = entry_dir(&root, &tid).join("payload");
    fs::create_dir_all(&payload).map_err(|e| Error::io(&payload, e))?;
    if n.is_group {
        move_path(&dir.join(id), &payload.join(id))?;
    } else {
        move_path(
            &dir.join(format!("{id}.md")),
            &payload.join(format!("{id}.md")),
        )?;
        for a in list_attachments(&dir, id) {
            move_path(&dir.join(&a.file), &payload.join(&a.file))?;
        }
    }
    store.touch(&dir.join(id));
    store.touch(&dir.join(format!("{id}.md")));
    let entry = TrashEntry {
        id: tid.clone(),
        kind: TrashKind::Node,
        item_id: id.to_string(),
        title: n.meta.title.clone(),
        parent: Some(parent.clone()),
        index,
        is_group: n.is_group,
        count: descendants.len() + 1,
        deleted_at: chrono::Utc::now().to_rfc3339(),
        board_id: store.state.manifest.id.clone(),
        archived: n.archived,
        cover: n.cover.clone(),
    };
    write_json(&entry_dir(&root, &tid).join("entry.json"), &entry)?;
    let st = &mut store.state;
    if let Some(list) = st.children_of_mut(&parent) {
        list.retain(|c| c != id);
    }
    st.nodes.remove(id);
    for d in descendants {
        st.nodes.remove(&d);
    }
    Ok(tid)
}

pub(crate) fn trash_lane(store: &mut BoardStore, k: &str) -> Result<String> {
    let s = &store.state;
    let index = s
        .lanes
        .iter()
        .position(|l| l.id == k)
        .ok_or_else(|| Error::not_found(k))?;
    let lane = s.lanes[index].clone();
    let count: usize = lane.order.iter().map(|c| s.descendants(c).len() + 1).sum();
    let root = s.root.clone();
    let tid = new_tid(&root);
    let payload = entry_dir(&root, &tid).join("payload");
    fs::create_dir_all(&payload).map_err(|e| Error::io(&payload, e))?;
    move_path(&root.join(k), &payload.join(k))?;
    store.touch(&root.join(k));
    let entry = TrashEntry {
        id: tid.clone(),
        kind: TrashKind::Lane,
        item_id: k.to_string(),
        title: lane.name.clone(),
        parent: None,
        index,
        is_group: false,
        count,
        deleted_at: chrono::Utc::now().to_rfc3339(),
        board_id: store.state.manifest.id.clone(),
        archived: lane.archived,
        cover: None,
    };
    write_json(&entry_dir(&root, &tid).join("entry.json"), &entry)?;
    let st = &mut store.state;
    let mut removed = Vec::new();
    for c in &lane.order {
        removed.push(c.clone());
        removed.extend(st.descendants(c));
    }
    for r in removed {
        st.nodes.remove(&r);
    }
    st.lanes.remove(index);
    Ok(tid)
}

pub fn read_entry(root: &Path, tid: &str) -> Result<TrashEntry> {
    let p = entry_dir(root, tid).join("entry.json");
    let text = fs::read_to_string(&p).map_err(|e| Error::io(&p, e))?;
    serde_json::from_str(&text).map_err(|e| Error::Json { path: p, source: e })
}

pub(crate) fn restore(store: &mut BoardStore, tid: &str, ch: &mut Changes) -> Result<Restored> {
    let root = store.state.root.clone();
    let entry = read_entry(&root, tid)?;
    let payload = entry_dir(&root, tid).join("payload");
    match entry.kind {
        TrashKind::Node => {
            let id = entry.item_id.clone();
            if store.state.nodes.contains_key(&id) {
                return Err(Error::Conflict(format!("already exists: {id}")));
            }
            let parent = pick_restore_parent(store, entry.parent.as_ref())?;
            if let Parent::Card(c) = &parent {
                store.ensure_group(c, ch)?;
            }
            let dir = store
                .state
                .container_dir(&parent)
                .ok_or_else(|| Error::not_found("restore parent"))?;
            fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            if entry.is_group {
                move_path(&payload.join(&id), &dir.join(&id))?;
            } else {
                move_path(
                    &payload.join(format!("{id}.md")),
                    &dir.join(format!("{id}.md")),
                )?;
                for e in fs::read_dir(&payload)
                    .map(|r| r.flatten().collect::<Vec<_>>())
                    .unwrap_or_default()
                {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if parse_attachment_name(&name).is_some_and(|(o, _)| o == id) {
                        move_path(&e.path(), &dir.join(&name))?;
                    }
                }
            }
            store.touch(&dir.join(&id));
            store.touch(&dir.join(format!("{id}.md")));
            let loaded = load_node_tree(&root, &dir, parent.clone(), &id)?;
            for k in loaded.keys() {
                if store.state.nodes.contains_key(k) {
                    return Err(Error::Conflict(format!("id collision on restore: {k}")));
                }
            }
            for (k, mut n) in loaded {
                if k == id {
                    n.archived = entry.archived;
                    n.cover = entry.cover.clone();
                }
                ch.nodes.insert(k.clone());
                store.state.nodes.insert(k, n);
            }
            let list = store.state.children_of_mut(&parent).unwrap();
            let at = entry.index.min(list.len());
            list.insert(at, id.clone());
            store.save_container(&parent)?;
            ch.touch_parent(&parent);
            remove_path(&entry_dir(&root, tid))?;
            Ok(Restored::Node(id))
        }
        TrashKind::Lane => {
            let k = entry.item_id.clone();
            if store.state.lane(&k).is_some() {
                return Err(Error::Conflict(format!("lane exists: {k}")));
            }
            move_path(&payload.join(&k), &root.join(&k))?;
            store.touch(&root.join(&k));
            let idx_path = root.join(&k).join(INDEX_JSON);
            let idx: ContainerIndex = fs::read_to_string(&idx_path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let (order, nodes) = super::load_lane_children(&root, &k, &idx);
            for (id, n) in nodes {
                ch.nodes.insert(id.clone());
                store.state.nodes.insert(id, n);
            }
            let lane = Lane {
                id: k.clone(),
                name: idx.name.clone().unwrap_or_else(|| entry.title.clone()),
                order,
                color: idx.color,
                width: idx.width,
                wip: idx.wip,
                collapsed: idx.collapsed,
                archived: entry.archived,
                extra: idx.extra,
            };
            let at = entry.index.min(store.state.lanes.len());
            store.state.lanes.insert(at, lane);
            store.save_manifest()?;
            ch.lanes = true;
            remove_path(&entry_dir(&root, tid))?;
            Ok(Restored::Lane(k))
        }
        TrashKind::Orphan => Err(Error::invalid("orphans cannot be restored automatically")),
    }
}

fn pick_restore_parent(store: &BoardStore, wanted: Option<&Parent>) -> Result<Parent> {
    let s = &store.state;
    if let Some(p) = wanted {
        let ok = match p {
            Parent::Lane(k) => s.manifest.kind == BoardKind::Kanban && s.lane(k).is_some(),
            Parent::Card(c) => s.nodes.contains_key(c),
            Parent::Root => s.manifest.kind == BoardKind::Files,
        };
        if ok {
            return Ok(p.clone());
        }
    }
    match s.manifest.kind {
        BoardKind::Files => Ok(Parent::Root),
        BoardKind::Kanban => s
            .lanes
            .iter()
            .find(|l| !l.archived)
            .or(s.lanes.first())
            .map(|l| Parent::Lane(l.id.clone()))
            .ok_or_else(|| Error::invalid("no lane to restore into")),
    }
}

/// All trash entries, newest first.
pub fn list(root: &Path) -> Vec<TrashEntry> {
    let mut out: Vec<TrashEntry> = fs::read_dir(trash_root(root))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| read_entry(root, &e.file_name().to_string_lossy()).ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    out
}

/// Permanently delete entries older than `ttl_days` (or all when `ttl_days == 0`
/// and `all` is set). Returns number of removed entries.
pub fn purge(root: &Path, ttl_days: u32, all: bool) -> Result<usize> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(ttl_days as i64);
    let mut n = 0;
    for e in list(root) {
        let old = chrono::DateTime::parse_from_rfc3339(&e.deleted_at)
            .map(|d| d < cutoff)
            .unwrap_or(true);
        if all || old {
            remove_path(&entry_dir(root, &e.id))?;
            n += 1;
        }
    }
    // Orphans follow the same retention using directory mtime.
    if let Ok(rd) = fs::read_dir(orphans_dir(root)) {
        for e in rd.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t) < cutoff)
                .unwrap_or(false);
            if all || old {
                remove_path(&e.path())?;
                n += 1;
            }
        }
    }
    Ok(n)
}
