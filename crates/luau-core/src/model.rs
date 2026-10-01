//! Board model: on-disk manifests, in-memory state and UI DTOs.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::brand::SCHEMA;
use crate::markdown::{Face, Footer, Heading, LinkRef, ParsedCard, TaskStats};

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
pub enum BoardKind {
    #[default]
    Kanban,
    Files,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    #[default]
    Columns,
    Rows,
}

/// Which axis keeps a fixed size: `FixedMain` = lanes have fixed width (columns)
/// or fixed height (rows) and cards scroll; `FixedCross` = cards have fixed size and wrap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Spacing {
    #[default]
    FixedMain,
    FixedCross,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewSettings {
    pub orientation: Orientation,
    pub spacing: Spacing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverMode {
    /// Large image at the top of the card.
    #[default]
    Cover,
    /// Small thumbnail next to the title.
    Thumb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cover {
    pub file: String,
    #[serde(default)]
    pub mode: CoverMode,
}

/// `.luau/board.json`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardManifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: BoardKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<String>,
    /// Root order (files boards).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archived: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub archived_lanes: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub covers: BTreeMap<String, Cover>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub view: ViewSettings,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tag_colors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl BoardManifest {
    pub fn new(id: String, name: String, kind: BoardKind) -> Self {
        BoardManifest {
            schema: SCHEMA,
            id,
            name,
            kind,
            lanes: vec![],
            order: vec![],
            archived: vec![],
            archived_lanes: vec![],
            covers: BTreeMap::new(),
            view: ViewSettings::default(),
            tag_colors: BTreeMap::new(),
            created: Some(chrono::Utc::now().to_rfc3339()),
            extra: Map::new(),
        }
    }
}

/// `<lane>/index.json` and `<group>/index.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContainerIndex {
    pub schema: u32,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub order: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub archived: Vec<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub covers: BTreeMap<String, Cover>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wip: Option<u32>,
    #[serde(skip_serializing_if = "is_false")]
    pub collapsed: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub enum Parent {
    Lane(String),
    Card(String),
    Root,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub id: String,
    pub name: String,
    pub order: Vec<String>,
    pub color: Option<String>,
    pub width: Option<u32>,
    pub wip: Option<u32>,
    pub collapsed: bool,
    pub archived: bool,
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    Image,
    Pdf,
    Video,
    Audio,
    Text,
    Other,
}

impl AttachmentKind {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "avif" | "heic" => {
                Self::Image
            }
            "pdf" => Self::Pdf,
            "mp4" | "mov" | "webm" | "mkv" | "m4v" => Self::Video,
            "mp3" | "wav" | "ogg" | "m4a" | "flac" => Self::Audio,
            "txt" | "csv" | "json" | "md" | "log" | "yaml" | "yml" | "toml" | "xml" => Self::Text,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// File name on disk (`<cardId>.<tok>[-name].<ext>`).
    pub file: String,
    /// Human display name (`name.ext` or `tok.ext`).
    pub display: String,
    pub kind: AttachmentKind,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub parent: Parent,
    pub is_group: bool,
    pub children: Vec<String>,
    pub archived: bool,
    pub cover: Option<Cover>,
    pub meta: ParsedCard,
    pub attachments: Vec<Attachment>,
    pub mtime: i64,
    pub size: u64,
    pub hash: String,
    /// Extra keys preserved from a group's index.json.
    pub group_extra: Map<String, Value>,
}

/// Real on-disk layout of a folder opened "as is" (no `.luau`, see `io::loose`).
/// Normal boards derive every path from ids; loose boards map ids to paths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LooseLayout {
    /// Node id → Markdown file (absolute). Group folders may have none.
    pub files: HashMap<String, PathBuf>,
    /// Lane or group id → folder (absolute).
    pub dirs: HashMap<String, PathBuf>,
    /// Content shown for nodes without a file (folder groups).
    pub synthetic: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct BoardState {
    pub root: PathBuf,
    pub manifest: BoardManifest,
    pub lanes: Vec<Lane>,
    pub root_order: Vec<String>,
    pub nodes: HashMap<String, Node>,
    /// Set when opened from a newer schema or otherwise protected.
    pub read_only: Option<String>,
    pub warnings: Vec<String>,
    pub version: u64,
    /// Set for folders opened "as is" (`io::loose`); `None` for normal boards.
    pub loose: Option<Box<LooseLayout>>,
}

impl BoardState {
    pub fn lane(&self, id: &str) -> Option<&Lane> {
        self.lanes.iter().find(|l| l.id == id)
    }
    pub fn lane_mut(&mut self, id: &str) -> Option<&mut Lane> {
        self.lanes.iter_mut().find(|l| l.id == id)
    }

    /// Ordered children of a container.
    pub fn children_of(&self, parent: &Parent) -> Option<&Vec<String>> {
        match parent {
            Parent::Lane(k) => self.lane(k).map(|l| &l.order),
            Parent::Card(c) => self.nodes.get(c).map(|n| &n.children),
            Parent::Root => Some(&self.root_order),
        }
    }

    pub fn children_of_mut(&mut self, parent: &Parent) -> Option<&mut Vec<String>> {
        match parent {
            Parent::Lane(k) => self.lane_mut(k).map(|l| &mut l.order),
            Parent::Card(c) => self.nodes.get_mut(c).map(|n| &mut n.children),
            Parent::Root => Some(&mut self.root_order),
        }
    }

    /// Absolute directory of a container.
    pub fn container_dir(&self, parent: &Parent) -> Option<PathBuf> {
        match parent {
            Parent::Root => Some(self.root.clone()),
            Parent::Lane(k) | Parent::Card(k) if self.loose.is_some() => {
                self.loose.as_ref()?.dirs.get(k).cloned()
            }
            Parent::Lane(k) => Some(self.root.join(k)),
            Parent::Card(c) => {
                let n = self.nodes.get(c)?;
                Some(self.container_dir(&n.parent)?.join(c))
            }
        }
    }

    /// Absolute path of a node's Markdown file.
    pub fn node_file(&self, id: &str) -> Option<PathBuf> {
        if let Some(l) = &self.loose {
            return l.files.get(id).cloned();
        }
        let n = self.nodes.get(id)?;
        let dir = self.container_dir(&n.parent)?;
        Some(if n.is_group {
            dir.join(id).join(crate::brand::INDEX_MD)
        } else {
            dir.join(format!("{id}.md"))
        })
    }

    /// Directory holding the node's attachments.
    pub fn attachment_dir(&self, id: &str) -> Option<PathBuf> {
        if let Some(l) = &self.loose {
            return l
                .dirs
                .get(id)
                .cloned()
                .or_else(|| l.files.get(id)?.parent().map(PathBuf::from));
        }
        let n = self.nodes.get(id)?;
        let dir = self.container_dir(&n.parent)?;
        Some(if n.is_group { dir.join(id) } else { dir })
    }

    /// Path relative to the board root (forward slashes).
    pub fn rel(&self, p: &std::path::Path) -> String {
        p.strip_prefix(&self.root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// True when `ancestor` is `id` or one of its ancestors.
    pub fn is_ancestor(&self, ancestor: &str, id: &str) -> bool {
        let mut cur = Some(id.to_string());
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = match self.nodes.get(&c).map(|n| &n.parent) {
                Some(Parent::Card(p)) => Some(p.clone()),
                _ => None,
            };
        }
        false
    }

    /// Lane containing the node (walking up groups).
    pub fn lane_of(&self, id: &str) -> Option<String> {
        let mut cur = self.nodes.get(id)?;
        loop {
            match &cur.parent {
                Parent::Lane(k) => return Some(k.clone()),
                Parent::Root => return None,
                Parent::Card(c) => cur = self.nodes.get(c)?,
            }
        }
    }

    /// All descendants (depth-first, excluding `id`).
    pub fn descendants(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack: Vec<String> = self
            .nodes
            .get(id)
            .map(|n| n.children.iter().rev().cloned().collect())
            .unwrap_or_default();
        while let Some(c) = stack.pop() {
            if let Some(n) = self.nodes.get(&c) {
                stack.extend(n.children.iter().rev().cloned());
            }
            out.push(c);
        }
        out
    }

    pub fn position_of(&self, id: &str) -> Option<(Parent, usize)> {
        let n = self.nodes.get(id)?;
        let idx = self.children_of(&n.parent)?.iter().position(|c| c == id)?;
        Some((n.parent.clone(), idx))
    }
}

// ---------------------------------------------------------------------------
// DTOs sent to the UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LaneDto {
    pub id: String,
    pub name: String,
    pub order: Vec<String>,
    pub color: Option<String>,
    pub width: Option<u32>,
    pub wip: Option<u32>,
    pub collapsed: bool,
    pub archived: bool,
}

impl From<&Lane> for LaneDto {
    fn from(l: &Lane) -> Self {
        LaneDto {
            id: l.id.clone(),
            name: l.name.clone(),
            order: l.order.clone(),
            color: l.color.clone(),
            width: l.width,
            wip: l.wip,
            collapsed: l.collapsed,
            archived: l.archived,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    pub id: String,
    pub parent: Parent,
    pub is_group: bool,
    pub children: Vec<String>,
    pub archived: bool,
    pub cover: Option<Cover>,
    pub title: String,
    pub has_title_line: bool,
    pub tags: Vec<String>,
    pub links: Vec<LinkRef>,
    pub mentions: Vec<String>,
    pub dates: Vec<String>,
    pub footer: Footer,
    pub tasks: TaskStats,
    pub face: Face,
    pub headings: Vec<Heading>,
    pub attachments: Vec<Attachment>,
    pub mtime: i64,
    pub word_count: usize,
    pub has_code: bool,
}

impl From<&Node> for NodeDto {
    fn from(n: &Node) -> Self {
        NodeDto {
            id: n.id.clone(),
            parent: n.parent.clone(),
            is_group: n.is_group,
            children: n.children.clone(),
            archived: n.archived,
            cover: n.cover.clone(),
            title: n.meta.title.clone(),
            has_title_line: n.meta.has_title_line,
            tags: n.meta.tags.clone(),
            links: n.meta.links.clone(),
            mentions: n.meta.mentions.clone(),
            dates: n.meta.dates.clone(),
            footer: n.meta.footer.clone(),
            tasks: n.meta.tasks.clone(),
            face: n.meta.face.clone(),
            headings: n.meta.headings.clone(),
            attachments: n.attachments.clone(),
            mtime: n.mtime,
            word_count: n.meta.word_count,
            has_code: n.meta.has_code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardHeaderDto {
    pub id: String,
    pub name: String,
    pub kind: BoardKind,
    pub root: String,
    pub view: ViewSettings,
    pub tag_colors: BTreeMap<String, String>,
    pub read_only: Option<String>,
    pub warnings: Vec<String>,
    pub schema: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardSnapshot {
    pub header: BoardHeaderDto,
    pub lanes: Vec<LaneDto>,
    pub root_order: Vec<String>,
    pub nodes: Vec<NodeDto>,
    pub version: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardDelta {
    pub board_id: String,
    pub version: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<BoardHeaderDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lanes: Option<Vec<LaneDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_order: Option<Vec<String>>,
    pub nodes: Vec<NodeDto>,
    pub removed: Vec<String>,
}

impl BoardDelta {
    pub fn is_empty(&self) -> bool {
        self.header.is_none()
            && self.lanes.is_none()
            && self.root_order.is_none()
            && self.nodes.is_empty()
            && self.removed.is_empty()
    }
}

impl BoardState {
    pub fn header_dto(&self) -> BoardHeaderDto {
        BoardHeaderDto {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            kind: self.manifest.kind,
            root: self.root.to_string_lossy().into_owned(),
            view: self.manifest.view.clone(),
            tag_colors: self.manifest.tag_colors.clone(),
            read_only: self.read_only.clone(),
            warnings: self.warnings.clone(),
            schema: self.manifest.schema,
        }
    }

    pub fn snapshot(&self) -> BoardSnapshot {
        BoardSnapshot {
            header: self.header_dto(),
            lanes: self.lanes.iter().map(LaneDto::from).collect(),
            root_order: self.root_order.clone(),
            nodes: self.nodes.values().map(NodeDto::from).collect(),
            version: self.version,
        }
    }
}
