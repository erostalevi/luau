//! Standard interchange model (SPEC §15) and the import planner.
//!
//! `cards` are listed in pre-order (parents before children, siblings in order)
//! so an import can append them one by one. Attachment `path`s are relative to
//! `sourceRoot`; `file` is the exact name the card body references.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::fsutil::sanitize_name;
use crate::ids::attachment_token;
use crate::model::*;
use crate::store::{LanePatch, Op};

pub const FORMAT: &str = "luau-interchange";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interchange {
    pub format: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
    /// Absolute folder that attachment `path`s are relative to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_root: Option<String>,
    pub board: IxBoard,
    #[serde(default)]
    pub lanes: Vec<IxLane>,
    #[serde(default)]
    pub cards: Vec<IxCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IxBoard {
    #[serde(default)]
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub kind: BoardKind,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_colors: BTreeMap<String, String>,
    #[serde(default)]
    pub view: ViewSettings,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IxLane {
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wip: Option<u32>,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IxAttachment {
    /// Name referenced from the body (replaced on import).
    pub file: String,
    pub display: String,
    /// Path relative to `sourceRoot` (forward slashes).
    pub path: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IxCard {
    pub key: String,
    /// Parent by key: `lane` (lane key), `card` (card key) or `root`.
    pub parent: Parent,
    #[serde(default)]
    pub order: usize,
    #[serde(default)]
    pub title: String,
    /// Full Markdown document (title line included).
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Keys (or ids) of linked cards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<IxAttachment>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Cover>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

impl Interchange {
    pub fn new(board: IxBoard) -> Self {
        Interchange {
            format: FORMAT.into(),
            version: VERSION,
            exported_at: Some(chrono::Utc::now().to_rfc3339()),
            source_root: None,
            board,
            lanes: vec![],
            cards: vec![],
        }
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.format != FORMAT {
            return Err(crate::Error::invalid("not a Luau interchange file"));
        }
        if self.version > VERSION {
            return Err(crate::Error::invalid(format!(
                "interchange version {} is newer than supported",
                self.version
            )));
        }
        Ok(())
    }

    pub fn card(&self, key: &str) -> Option<&IxCard> {
        self.cards.iter().find(|c| c.key == key)
    }

    /// Children of `parent` (by key) in order.
    pub fn children<'a>(&'a self, parent: &'a Parent) -> impl Iterator<Item = &'a IxCard> + 'a {
        self.cards.iter().filter(move |c| &c.parent == parent)
    }
}

/// Build the interchange for a board, or for one card subtree (`only`), whose
/// root is then reported with a `root` parent.
pub fn from_state(
    st: &BoardState,
    read: &dyn Fn(&str) -> String,
    only: Option<&str>,
) -> Interchange {
    let m = &st.manifest;
    let mut ix = Interchange::new(IxBoard {
        key: m.id.clone(),
        name: m.name.clone(),
        kind: m.kind,
        tag_colors: m.tag_colors.clone(),
        view: m.view.clone(),
        extra: Map::new(),
    });
    ix.source_root = Some(st.root.to_string_lossy().into_owned());
    fn walk(
        st: &BoardState,
        read: &dyn Fn(&str) -> String,
        ids: &[String],
        parent: Parent,
        out: &mut Vec<IxCard>,
    ) {
        for (i, id) in ids.iter().enumerate() {
            let Some(n) = st.nodes.get(id) else { continue };
            let dir = st.attachment_dir(id).unwrap_or_else(|| st.root.clone());
            out.push(IxCard {
                key: id.clone(),
                parent: parent.clone(),
                order: i,
                title: n.meta.title.clone(),
                body: read(id),
                tags: n.meta.tags.clone(),
                links: n.meta.links.iter().filter_map(|l| l.id.clone()).collect(),
                attachments: n
                    .attachments
                    .iter()
                    .map(|a| IxAttachment {
                        file: a.file.clone(),
                        display: a.display.clone(),
                        path: st.rel(&dir.join(&a.file)),
                        size: a.size,
                    })
                    .collect(),
                archived: n.archived,
                cover: n.cover.clone(),
                extra: Map::new(),
            });
            walk(st, read, &n.children, Parent::Card(id.clone()), out);
        }
    }
    match only {
        Some(id) => walk(st, read, &[id.to_string()], Parent::Root, &mut ix.cards),
        None => {
            for l in &st.lanes {
                ix.lanes.push(IxLane {
                    key: l.id.clone(),
                    name: l.name.clone(),
                    color: l.color.clone(),
                    width: l.width,
                    wip: l.wip,
                    collapsed: l.collapsed,
                    archived: l.archived,
                });
                walk(
                    st,
                    read,
                    &l.order,
                    Parent::Lane(l.id.clone()),
                    &mut ix.cards,
                );
            }
            walk(st, read, &st.root_order, Parent::Root, &mut ix.cards);
        }
    }
    ix
}

/// A file to copy once the batch has created its card.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedCopy {
    pub card: String,
    pub src: PathBuf,
    pub file: String,
}

#[derive(Debug, Clone, Default)]
pub struct ImportPlan {
    pub ops: Vec<Op>,
    pub copies: Vec<PlannedCopy>,
    /// Interchange key → new id (cards and lanes).
    pub ids: HashMap<String, String>,
    pub cards: usize,
    pub lanes: usize,
    pub warnings: Vec<String>,
}

static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\[\]\n|#]+)([^\[\]\n]*)\]\]").unwrap());

/// Rewrite `[[key…]]` targets through `map` (unknown targets are kept).
pub fn remap_links(body: &str, map: &dyn Fn(&str) -> Option<String>) -> String {
    LINK_RE
        .replace_all(body, |c: &regex::Captures| match map(c[2].trim()) {
            Some(id) => format!("{}[[{}{}]]", &c[1], id, &c[3]),
            None => c[0].to_string(),
        })
        .into_owned()
}

/// New attachment name for card `id` keeping a readable stem.
pub fn attachment_name(id: &str, display: &str) -> String {
    let (stem, ext) = match display.rsplit_once('.') {
        Some((s, e))
            if !e.is_empty() && e.len() <= 12 && e.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            (s, e.to_ascii_lowercase())
        }
        _ => (display, "bin".to_string()),
    };
    let stem = sanitize_name(stem, 40);
    let tok = attachment_token();
    if stem.is_empty() {
        format!("{id}.{tok}.{ext}")
    } else {
        format!("{id}.{tok}-{stem}.{ext}")
    }
}

pub struct PlanOptions<'a> {
    /// Target board kind (cards with an incompatible parent are re-homed).
    pub kind: BoardKind,
    /// Lane name used for root cards on a kanban target.
    pub inbox_lane: &'a str,
    pub new_card_id: &'a mut dyn FnMut() -> String,
    pub new_lane_id: &'a mut dyn FnMut() -> String,
}

/// Plan the ops that recreate `ix` on an (empty) board with fresh ids.
pub fn plan(ix: &Interchange, o: PlanOptions) -> ImportPlan {
    let mut p = ImportPlan::default();
    let root = ix.source_root.as_deref().map(PathBuf::from);
    let card_keys: HashSet<&str> = ix.cards.iter().map(|c| c.key.as_str()).collect();
    for c in &ix.cards {
        if !p.ids.contains_key(&c.key) {
            p.ids.insert(c.key.clone(), (o.new_card_id)());
        }
    }
    let mut lane_ops = Vec::new();
    let mut lane_after = Vec::new();
    let mut archived_lanes = Vec::new();
    if o.kind == BoardKind::Kanban {
        for l in &ix.lanes {
            let k = (o.new_lane_id)();
            p.ids.insert(l.key.clone(), k.clone());
            lane_ops.push(Op::CreateLane {
                id: k.clone(),
                name: l.name.clone(),
                index: None,
            });
            let patch = LanePatch {
                name: None,
                color: l.color.clone(),
                width: l.width,
                wip: l.wip,
                collapsed: l.collapsed.then_some(true),
            };
            if patch != LanePatch::default() {
                lane_after.push(Op::UpdateLane {
                    id: k.clone(),
                    patch,
                });
            }
            if l.archived {
                archived_lanes.push((k, true));
            }
            p.lanes += 1;
        }
    }
    let mut inbox: Option<String> = None;
    let mut card_ops = Vec::new();
    let mut covers = Vec::new();
    let mut archived = Vec::new();
    for c in &ix.cards {
        let id = p.ids[&c.key].clone();
        let parent = match &c.parent {
            Parent::Card(k) if card_keys.contains(k.as_str()) => Parent::Card(p.ids[k].clone()),
            Parent::Lane(k) if o.kind == BoardKind::Kanban && p.ids.contains_key(k) => {
                Parent::Lane(p.ids[k].clone())
            }
            _ if o.kind == BoardKind::Files => Parent::Root,
            _ => {
                let k = inbox.get_or_insert_with(|| {
                    let k = (o.new_lane_id)();
                    lane_ops.push(Op::CreateLane {
                        id: k.clone(),
                        name: o.inbox_lane.to_string(),
                        index: None,
                    });
                    p.lanes += 1;
                    k
                });
                Parent::Lane(k.clone())
            }
        };
        let mut body = remap_links(&c.body, &|t| {
            p.ids.get(t).filter(|_| card_keys.contains(t)).cloned()
        });
        for a in &c.attachments {
            let file = attachment_name(&id, &a.display);
            if !a.file.is_empty() {
                body = body.replace(&a.file, &file);
            }
            let src = match &root {
                Some(r) => match crate::fsutil::safe_join(r, &a.path) {
                    Ok(s) => s,
                    Err(_) => {
                        p.warnings.push(format!("unsafe_attachment:{}", a.path));
                        continue;
                    }
                },
                None => {
                    p.warnings.push(format!("missing_attachment:{}", a.path));
                    continue;
                }
            };
            if let Some(cv) = &c.cover
                && cv.file == a.file
            {
                covers.push(Op::SetCover {
                    id: id.clone(),
                    cover: Some(Cover {
                        file: file.clone(),
                        mode: cv.mode,
                    }),
                });
            }
            p.copies.push(PlannedCopy {
                card: id.clone(),
                src,
                file,
            });
        }
        card_ops.push(Op::CreateCard {
            id: id.clone(),
            parent,
            index: None,
            content: body,
        });
        if c.archived {
            archived.push((id, true));
        }
        p.cards += 1;
    }
    p.ops.extend(lane_ops);
    p.ops.extend(lane_after);
    p.ops.extend(card_ops);
    p.ops.extend(covers);
    if !archived.is_empty() || !archived_lanes.is_empty() {
        p.ops.push(Op::SetArchived {
            nodes: archived,
            lanes: archived_lanes,
        });
    }
    if !ix.board.tag_colors.is_empty() {
        p.ops.push(Op::UpdateBoard {
            patch: crate::store::BoardPatch {
                name: None,
                view: None,
                tag_colors: Some(ix.board.tag_colors.clone()),
            },
        });
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_links_keeping_heading_and_alias() {
        let map = |t: &str| (t == "a").then(|| "c111111".to_string());
        let out = remap_links(
            "see [[a]] and [[a#Plan|the plan]] and ![[a]] but not [[b]]",
            &map,
        );
        assert_eq!(
            out,
            "see [[c111111]] and [[c111111#Plan|the plan]] and ![[c111111]] but not [[b]]"
        );
    }

    #[test]
    fn attachment_names_follow_convention() {
        let n = attachment_name("c123456", "My Photo.PNG");
        assert!(crate::store::parse_attachment_name(&n).is_some(), "{n}");
        assert!(n.starts_with("c123456.") && n.ends_with("-my-photo.png"));
    }
}
