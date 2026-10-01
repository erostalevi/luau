//! Loose adapter: any folder of Markdown (an Obsidian vault, plain `.md` notes)
//! → a [`VaultTree`] → the standard [`Interchange`].
//!
//! Mapping rules (QUESTIONNAIRE L.85):
//! - top-level folders become **lanes**; notes in the root go to an inbox lane.
//!   Without any folder the result is a **files** board.
//! - deeper folders become **group cards**; a folder note (`index.md`,
//!   `README.md` or `<Folder>.md`) becomes the group's body.
//! - each `.md` file becomes a card; its first heading (when it is the first
//!   line) is the title, otherwise the file name is prepended as `# Title`.
//! - YAML front-matter: `tags` → `#tags`, scalar keys → property footer.
//! - `[[Wiki links]]` and `[text](Note.md)` links → card links (resolved by
//!   path, file name or title); local images/files → copied attachments.
//!
//! The source folder is only ever read.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::interchange::{Interchange, IxAttachment, IxBoard, IxCard, IxLane};
use super::{is_md, md_stem, natural_cmp, normalize_rel, skip_name};
use crate::error::{Error, Result};
use crate::fsutil::read_to_string;
use crate::model::{BoardKind, Parent};

pub const MAX_NOTES: usize = 20_000;
pub const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq)]
pub struct VaultNode {
    /// Path relative to the vault root (forward slashes): the note file, or the folder of a group.
    pub rel: String,
    /// File stem or folder name.
    pub name: String,
    /// Markdown file (the folder note for groups, when present).
    pub file: Option<PathBuf>,
    /// Folder of a group.
    pub dir: Option<PathBuf>,
    pub children: Vec<VaultNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultLane {
    pub rel: String,
    pub name: String,
    pub dir: PathBuf,
    pub items: Vec<VaultNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VaultTree {
    pub root: PathBuf,
    pub name: String,
    pub kind: BoardKind,
    pub lanes: Vec<VaultLane>,
    /// Notes (and groups, on files boards) directly in the root.
    pub root_items: Vec<VaultNode>,
    pub notes: usize,
    pub truncated: bool,
}

struct Ctx {
    notes: usize,
    truncated: bool,
}

fn is_folder_note(file: &str, folder: &str) -> bool {
    let stem = md_stem(file).to_lowercase();
    is_md(file) && (stem == "index" || stem == "readme" || stem == folder.to_lowercase())
}

/// Scan a folder. Symlinks, hidden entries and nested Luau boards are skipped.
pub fn scan(root: &Path) -> Result<VaultTree> {
    if !root.is_dir() {
        return Err(Error::invalid(format!("not a folder: {}", root.display())));
    }
    let mut ctx = Ctx {
        notes: 0,
        truncated: false,
    };
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Imported".into());
    let (files, dirs) = list(root);
    let mut root_items = Vec::new();
    for f in files {
        if let Some(n) = note(&mut ctx, root, "", &f) {
            root_items.push(n);
        }
    }
    let mut lanes = Vec::new();
    for d in dirs {
        let dir = root.join(&d);
        let items = scan_dir(&mut ctx, &dir, &d, 1, None);
        if !items.is_empty() {
            lanes.push(VaultLane {
                rel: d.clone(),
                name: d,
                dir,
                items,
            });
        }
    }
    let kind = if lanes.is_empty() {
        BoardKind::Files
    } else {
        BoardKind::Kanban
    };
    Ok(VaultTree {
        root: root.to_path_buf(),
        name,
        kind,
        lanes,
        root_items,
        notes: ctx.notes,
        truncated: ctx.truncated,
    })
}

/// (markdown files, folders) of `dir`, naturally sorted.
fn list(dir: &Path) -> (Vec<String>, Vec<String>) {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let Ok(rd) = fs::read_dir(dir) else {
        return (files, dirs);
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if skip_name(&name) {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            if !crate::store::is_board(&e.path()) {
                dirs.push(name);
            }
        } else if ft.is_file() && is_md(&name) {
            files.push(name);
        }
    }
    files.sort_by(|a, b| natural_cmp(a, b));
    dirs.sort_by(|a, b| natural_cmp(a, b));
    (files, dirs)
}

fn note(ctx: &mut Ctx, dir: &Path, rel_dir: &str, file: &str) -> Option<VaultNode> {
    if ctx.notes >= MAX_NOTES {
        ctx.truncated = true;
        return None;
    }
    ctx.notes += 1;
    Some(VaultNode {
        rel: join_rel(rel_dir, file),
        name: md_stem(file).to_string(),
        file: Some(dir.join(file)),
        dir: None,
        children: vec![],
    })
}

fn join_rel(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        format!("{a}/{b}")
    }
}

/// Items of a folder; `own` is the folder name when it is a group (its folder note is skipped).
fn scan_dir(
    ctx: &mut Ctx,
    dir: &Path,
    rel: &str,
    depth: usize,
    own: Option<&str>,
) -> Vec<VaultNode> {
    if depth > MAX_DEPTH {
        ctx.truncated = true;
        return vec![];
    }
    let (files, dirs) = list(dir);
    // Files and folders are interleaved by name, like a file manager sorted by name.
    let mut entries: Vec<(String, bool)> = files
        .into_iter()
        .map(|f| (f, false))
        .chain(dirs.into_iter().map(|d| (d, true)))
        .collect();
    entries.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    let mut out = Vec::new();
    for (name, is_dir) in entries {
        if is_dir {
            let sub = dir.join(&name);
            let sub_rel = join_rel(rel, &name);
            let (sub_files, _) = list(&sub);
            let folder_note = sub_files.iter().find(|f| is_folder_note(f, &name)).cloned();
            let children = scan_dir(ctx, &sub, &sub_rel, depth + 1, Some(&name));
            if children.is_empty() && folder_note.is_none() {
                continue; // e.g. an attachments folder
            }
            if folder_note.is_some() {
                ctx.notes += 1;
            }
            out.push(VaultNode {
                rel: sub_rel,
                name: name.clone(),
                file: folder_note.map(|f| sub.join(f)),
                dir: Some(sub),
                children,
            });
        } else {
            if own.is_some_and(|o| is_folder_note(&name, o)) {
                continue;
            }
            if let Some(n) = note(ctx, dir, rel, &name) {
                out.push(n);
            }
        }
    }
    out
}

impl VaultTree {
    /// All nodes in pre-order with their parent key (lane rel for lanes).
    pub fn walk(&self) -> Vec<(&VaultNode, Option<&str>, Option<&str>)> {
        fn rec<'a>(
            n: &'a VaultNode,
            lane: Option<&'a str>,
            parent: Option<&'a str>,
            out: &mut Vec<(&'a VaultNode, Option<&'a str>, Option<&'a str>)>,
        ) {
            out.push((n, lane, parent));
            for c in &n.children {
                rec(c, lane, Some(&n.rel), out);
            }
        }
        let mut out = Vec::new();
        for l in &self.lanes {
            for n in &l.items {
                rec(n, Some(&l.rel), None, &mut out);
            }
        }
        for n in &self.root_items {
            rec(n, None, None, &mut out);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Note conversion
// ---------------------------------------------------------------------------

static WIKI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!?)\[\[([^\[\]\n|#]*)(#[^\[\]\n|]*)?(\|[^\[\]\n]*)?\]\]").unwrap()
});
static MDLINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!?)\[([^\]\n]*)\]\(\s*(<[^>\n]+>|[^)\s]+)(\s+\x22[^\x22\n]*\x22)?\s*\)").unwrap()
});

/// Result of converting one note.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub title: String,
    pub body: String,
    pub attachments: Vec<IxAttachment>,
}

/// Split YAML front-matter into (tags, scalar fields, rest of the document).
pub fn split_front_matter(text: &str) -> (Vec<String>, Vec<(String, String)>, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(after) = t
        .strip_prefix("---\n")
        .or_else(|| t.strip_prefix("---\r\n"))
    else {
        return (vec![], vec![], t);
    };
    let mut end = None;
    let mut pos = 0;
    for line in after.split_inclusive('\n') {
        let l = line.trim_end();
        if l == "---" || l == "..." {
            end = Some(pos + line.len());
            break;
        }
        pos += line.len();
    }
    let Some(end) = end else {
        return (vec![], vec![], t);
    };
    let yaml = &after[..pos];
    let rest = &after[end..];
    let (mut tags, mut fields) = (Vec::new(), Vec::new());
    let mut cur_list: Option<String> = None;
    let mut list_vals: Vec<String> = Vec::new();
    let flush = |key: &Option<String>,
                 vals: &mut Vec<String>,
                 tags: &mut Vec<String>,
                 fields: &mut Vec<(String, String)>| {
        if let Some(k) = key {
            if k == "tags" || k == "tag" {
                tags.append(vals);
            } else if !vals.is_empty() {
                fields.push((k.clone(), vals.join(", ")));
                vals.clear();
            }
        }
    };
    for line in yaml.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if cur_list.is_some() {
                list_vals.push(unquote(item));
            }
            continue;
        }
        flush(&cur_list, &mut list_vals, &mut tags, &mut fields);
        cur_list = None;
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_lowercase();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let v = v.trim();
        if v.is_empty() {
            cur_list = Some(key);
        } else if let Some(inner) = v.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            let vals: Vec<String> = inner
                .split(',')
                .map(unquote)
                .filter(|s| !s.is_empty())
                .collect();
            if key == "tags" || key == "tag" {
                tags.extend(vals);
            } else if !vals.is_empty() {
                fields.push((key, vals.join(", ")));
            }
        } else if key == "tags" || key == "tag" {
            tags.extend(v.split([',', ' ']).map(unquote).filter(|s| !s.is_empty()));
        } else {
            fields.push((key, unquote(v)));
        }
    }
    flush(&cur_list, &mut list_vals, &mut tags, &mut fields);
    let tags = tags
        .into_iter()
        .map(|t| t.trim_start_matches('#').replace(' ', "-"))
        .filter(|t| !t.is_empty())
        .collect();
    (tags, fields, rest)
}

fn unquote(s: &str) -> String {
    s.trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_string()
}

/// Title from the first line when it is a heading (any level), else `None`.
fn heading_title(first: &str) -> Option<String> {
    let t = first.trim_start();
    let hashes = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
        let s = t[hashes..].trim().trim_end_matches('#').trim();
        (!s.is_empty()).then(|| s.to_string())
    } else {
        None
    }
}

/// Resolves link targets and attachment files inside the vault.
pub struct Resolver {
    /// lowercase rel path without extension → key
    by_path: HashMap<String, String>,
    /// lowercase file stem / title → keys
    by_name: HashMap<String, Vec<String>>,
    /// lowercase file name → rel paths of non-Markdown files
    files: HashMap<String, Vec<String>>,
    root: PathBuf,
}

impl Resolver {
    pub fn new(tree: &VaultTree) -> Self {
        let mut r = Resolver {
            by_path: HashMap::new(),
            by_name: HashMap::new(),
            files: HashMap::new(),
            root: tree.root.clone(),
        };
        for (n, _, _) in tree.walk() {
            let key = n.rel.clone();
            let no_ext = if n.dir.is_some() {
                n.rel.clone()
            } else {
                md_stem(&n.rel).to_string()
            };
            r.by_path.insert(no_ext.to_lowercase(), key.clone());
            r.by_name
                .entry(n.name.to_lowercase())
                .or_default()
                .push(key);
        }
        index_files(&tree.root, "", 0, &mut r.files);
        r
    }

    /// Card key for a link target (path, file name or title).
    pub fn note(&self, from_dir: &str, target: &str) -> Option<String> {
        let t = target.trim().replace('\\', "/");
        if t.is_empty() {
            return None;
        }
        let t = if is_md(&t) {
            md_stem(&t).to_string()
        } else {
            t
        };
        let lower = t.to_lowercase();
        if let Some(k) =
            normalize_rel(from_dir, &t).and_then(|p| self.by_path.get(&p.to_lowercase()))
        {
            return Some(k.clone());
        }
        if let Some(k) = self.by_path.get(&lower) {
            return Some(k.clone());
        }
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        match self.by_name.get(name) {
            Some(v) if !v.is_empty() => Some(v[0].clone()),
            _ => None,
        }
    }

    /// Relative path of a local non-Markdown file referenced from `from_dir`.
    pub fn file(&self, from_dir: &str, target: &str) -> Option<String> {
        let t = percent_encoding::percent_decode_str(target.trim())
            .decode_utf8_lossy()
            .replace('\\', "/");
        if t.is_empty()
            || t.contains("://")
            || t.starts_with("data:")
            || t.starts_with('#')
            || is_md(&t)
        {
            return None;
        }
        let t = t.split(['?', '#']).next().unwrap_or(&t).to_string();
        if let Some(p) = normalize_rel(from_dir, &t)
            && self.root.join(&p).is_file()
        {
            return Some(p);
        }
        if let Some(p) = normalize_rel("", &t)
            && self.root.join(&p).is_file()
        {
            return Some(p);
        }
        let name = t.rsplit('/').next()?.to_lowercase();
        self.files.get(&name).and_then(|v| v.first().cloned())
    }
}

fn index_files(dir: &Path, rel: &str, depth: usize, out: &mut HashMap<String, Vec<String>>) {
    if depth > MAX_DEPTH || out.len() > MAX_NOTES * 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if skip_name(&name) {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        let r = join_rel(rel, &name);
        if ft.is_dir() && !crate::store::is_board(&e.path()) {
            index_files(&e.path(), &r, depth + 1, out);
        } else if ft.is_file() && !is_md(&name) {
            out.entry(name.to_lowercase()).or_default().push(r);
        }
    }
}

/// Convert a note's text: title, links → keys, local files → attachment placeholders.
pub fn convert_note(text: &str, fallback_title: &str, rel_dir: &str, res: &Resolver) -> Converted {
    let (tags, fields, rest) = split_front_matter(text);
    let rest = rest.trim_start_matches(['\n', '\r']);
    let (first, tail) = match rest.split_once('\n') {
        Some((a, b)) => (a, b),
        None => (rest, ""),
    };
    let (title, mut body) = match heading_title(first) {
        Some(t) => (t.clone(), format!("# {t}\n{tail}")),
        None => {
            let t = fallback_title.trim().to_string();
            let t = if t.is_empty() {
                "Untitled".to_string()
            } else {
                t
            };
            if rest.trim().is_empty() {
                (t.clone(), format!("# {t}\n"))
            } else {
                (t.clone(), format!("# {t}\n\n{rest}"))
            }
        }
    };
    let mut attachments: Vec<IxAttachment> = Vec::new();
    let add_att = |rel: String, attachments: &mut Vec<IxAttachment>| -> String {
        if let Some(a) = attachments.iter().find(|a| a.path == rel) {
            return a.file.clone();
        }
        let display = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let file = format!("luau-attachment-{:04}", attachments.len());
        let size = fs::metadata(res.root.join(&rel))
            .map(|m| m.len())
            .unwrap_or(0);
        attachments.push(IxAttachment {
            file: file.clone(),
            display,
            path: rel,
            size,
        });
        file
    };
    // Wiki links and embeds.
    body = WIKI_RE
        .replace_all(&body, |c: &regex::Captures| {
            let embed = &c[1] == "!";
            let target = c[2].trim();
            let heading = c.get(3).map(|m| m.as_str()).unwrap_or("");
            let alias = c.get(4).map(|m| m.as_str()).unwrap_or("");
            if target.is_empty() {
                return c[0].to_string();
            }
            if !is_md(target)
                && target.contains('.')
                && let Some(rel) = res.file(rel_dir, target)
            {
                let ph = add_att(rel, &mut attachments);
                let label = alias.trim_start_matches('|');
                let label = if label.is_empty() { target } else { label };
                return if embed {
                    format!("![{label}]({ph})")
                } else {
                    format!("[{label}]({ph})")
                };
            }
            match res.note(rel_dir, target) {
                Some(key) => format!("{}[[{key}{heading}{alias}]]", &c[1]),
                None => c[0].to_string(),
            }
        })
        .into_owned();
    // Markdown links to notes and local files.
    body = MDLINK_RE
        .replace_all(&body, |c: &regex::Captures| {
            let embed = &c[1] == "!";
            let text = &c[2];
            let raw = c[3].trim_start_matches('<').trim_end_matches('>');
            let title = c.get(4).map(|m| m.as_str()).unwrap_or("");
            if raw.contains("://") || raw.starts_with("mailto:") || raw.starts_with('#') {
                return c[0].to_string();
            }
            let decoded = percent_encoding::percent_decode_str(raw)
                .decode_utf8_lossy()
                .into_owned();
            let path_part = decoded.split('#').next().unwrap_or(&decoded);
            if is_md(path_part) && !embed {
                if let Some(key) = res.note(rel_dir, path_part) {
                    let t = text.trim();
                    return if t.is_empty() {
                        format!("[[{key}]]")
                    } else {
                        format!("[[{key}|{t}]]")
                    };
                }
                return c[0].to_string();
            }
            match res.file(rel_dir, raw) {
                Some(rel) => {
                    let ph = add_att(rel, &mut attachments);
                    format!("{}[{text}]({ph}{title})", &c[1])
                }
                None => c[0].to_string(),
            }
        })
        .into_owned();
    if !tags.is_empty() {
        let line: Vec<String> = tags.iter().map(|t| format!("#{t}")).collect();
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push('\n');
        body.push_str(&line.join(" "));
        body.push('\n');
    }
    if !fields.is_empty() {
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("\n---\n");
        for (k, v) in &fields {
            body.push_str(&format!("{k}: {v}\n"));
        }
    }
    Converted {
        title,
        body,
        attachments,
    }
}

fn parent_dir(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(a, _)| a).unwrap_or("")
}

/// Build the interchange for a scanned folder. Keys are vault-relative paths.
pub fn to_interchange(tree: &VaultTree) -> Interchange {
    let res = Resolver::new(tree);
    let mut ix = Interchange::new(IxBoard {
        key: String::new(),
        name: tree.name.clone(),
        kind: tree.kind,
        tag_colors: Default::default(),
        view: Default::default(),
        extra: Default::default(),
    });
    ix.source_root = Some(tree.root.to_string_lossy().into_owned());
    for l in &tree.lanes {
        ix.lanes.push(IxLane {
            key: format!("lane:{}", l.rel),
            name: l.name.clone(),
            color: None,
            width: None,
            wip: None,
            collapsed: false,
            archived: false,
        });
    }
    let mut order: HashMap<String, usize> = HashMap::new();
    for (n, lane, parent) in tree.walk() {
        let parent = match (parent, lane) {
            (Some(p), _) => Parent::Card(p.to_string()),
            (None, Some(l)) => Parent::Lane(format!("lane:{l}")),
            (None, None) => Parent::Root,
        };
        let slot = format!("{parent:?}");
        let i = order.entry(slot).or_insert(0);
        let idx = *i;
        *i += 1;
        let (text, dir) = match (&n.file, &n.dir) {
            (Some(f), Some(_)) => (read_to_string(f).unwrap_or_default(), n.rel.clone()),
            (Some(f), None) => (
                read_to_string(f).unwrap_or_default(),
                parent_dir(&n.rel).to_string(),
            ),
            (None, _) => (String::new(), n.rel.clone()),
        };
        let conv = convert_note(&text, &n.name, &dir, &res);
        ix.cards.push(IxCard {
            key: n.rel.clone(),
            parent,
            order: idx,
            title: conv.title,
            body: conv.body,
            tags: vec![],
            links: vec![],
            attachments: conv.attachments,
            archived: false,
            cover: None,
            extra: Default::default(),
        });
    }
    ix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn vault() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        write(r, "Welcome.md", "Hello [[Plan]] and ![[pic.png]]\n");
        write(
            r,
            "Projects/Plan.md",
            "---\ntags: [work, q4]\nstatus: active\n---\n# The plan\n\nSee [home](../Welcome.md).\n",
        );
        write(r, "Projects/Ideas/index.md", "Folder note body\n");
        write(r, "Projects/Ideas/Idea 1.md", "## Idea one\ntext\n");
        write(r, "Projects/Ideas/Idea 10.md", "ten\n");
        write(r, "Projects/Ideas/Idea 2.md", "two\n");
        write(r, "Done/Old.markdown", "old\n");
        write(r, "attachments/pic.png", "PNG");
        write(r, ".obsidian/app.json", "{}");
        d
    }

    #[test]
    fn scans_folders_into_lanes_groups_and_cards() {
        let d = vault();
        let t = scan(d.path()).unwrap();
        assert_eq!(t.kind, BoardKind::Kanban);
        let lanes: Vec<&str> = t.lanes.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(lanes, vec!["Done", "Projects"]); // attachments/ has no notes
        let projects = &t.lanes[1];
        let names: Vec<&str> = projects.items.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["Ideas", "Plan"]);
        let ideas = &projects.items[0];
        assert!(ideas.file.as_ref().unwrap().ends_with("index.md"));
        let kids: Vec<&str> = ideas.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(kids, vec!["Idea 1", "Idea 2", "Idea 10"]);
        assert_eq!(t.root_items.len(), 1);
        assert_eq!(t.notes, 7);
    }

    #[test]
    fn flat_folder_is_a_files_board() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "a.md", "# A\n");
        write(d.path(), "b.md", "B\n");
        let t = scan(d.path()).unwrap();
        assert_eq!(t.kind, BoardKind::Files);
        assert_eq!(t.root_items.len(), 2);
    }

    #[test]
    fn converts_titles_links_front_matter_and_attachments() {
        let d = vault();
        let ix = to_interchange(&scan(d.path()).unwrap());
        let plan = ix.card("Projects/Plan.md").unwrap();
        assert_eq!(plan.title, "The plan");
        assert!(plan.body.starts_with("# The plan\n"));
        assert!(plan.body.contains("[[Welcome.md|home]]"), "{}", plan.body);
        assert!(plan.body.contains("#work #q4"));
        assert!(plan.body.ends_with("\n---\nstatus: active\n"));
        let w = ix.card("Welcome.md").unwrap();
        assert_eq!(w.title, "Welcome");
        assert!(
            w.body
                .starts_with("# Welcome\n\nHello [[Projects/Plan.md]]"),
            "{}",
            w.body
        );
        assert_eq!(w.attachments.len(), 1);
        assert_eq!(w.attachments[0].path, "attachments/pic.png");
        assert!(
            w.body
                .contains(&format!("![pic.png]({})", w.attachments[0].file))
        );
        let idea = ix.card("Projects/Ideas/Idea 1.md").unwrap();
        assert_eq!(idea.title, "Idea one");
        assert_eq!(idea.parent, Parent::Card("Projects/Ideas".into()));
        let group = ix.card("Projects/Ideas").unwrap();
        assert!(group.body.starts_with("# Ideas\n\nFolder note body"));
        assert_eq!(ix.lanes.len(), 2);
    }

    #[test]
    fn front_matter_lists() {
        let (tags, fields, rest) = split_front_matter(
            "---\ntags:\n  - a b\n  - c\naliases:\n  - x\ntitle: \"T\"\n---\nbody",
        );
        assert_eq!(tags, vec!["a-b", "c"]);
        assert_eq!(
            fields,
            vec![("aliases".into(), "x".into()), ("title".into(), "T".into())]
        );
        assert_eq!(rest, "body");
        let (t, f, r) = split_front_matter("---\nno end");
        assert!(t.is_empty() && f.is_empty() && r == "---\nno end");
    }
}
