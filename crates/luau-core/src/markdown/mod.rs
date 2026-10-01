//! Markdown analysis for cards: title, tags, links, mentions, dates, footer
//! properties, task stats, card-face preview and extractive summary.
//!
//! Pure functions, no I/O. Everything the board view needs is derived here so
//! the UI never has to parse full documents.

pub mod footer;
pub mod summary;

use std::ops::Range;
use std::sync::LazyLock;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::ids::{IdKind, is_id};
pub use footer::Footer;

pub const FACE_MAX_ITEMS: usize = 8;
pub const FACE_MAX_ROWS: usize = 5;
pub const SUMMARY_MAX_CHARS: usize = 220;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRef {
    /// Card id when the target is an id; otherwise `None` (unresolved title link).
    pub id: Option<String>,
    /// Raw target text (id or title).
    pub target: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    pub embed: bool,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStats {
    pub total: usize,
    pub done: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceItem {
    pub text: String,
    /// `Some(checked)` for task items.
    pub task: Option<bool>,
    /// 0-based line of the item in the file (used to toggle checkboxes).
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Face {
    #[default]
    None,
    Checklist {
        items: Vec<FaceItem>,
        total: usize,
    },
    List {
        items: Vec<FaceItem>,
        total: usize,
        ordered: bool,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        total: usize,
    },
    Summary {
        text: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCard {
    pub title: String,
    /// True when line 0 is a `# ` heading.
    pub has_title_line: bool,
    pub tags: Vec<String>,
    pub links: Vec<LinkRef>,
    pub mentions: Vec<String>,
    pub dates: Vec<String>,
    pub footer: Footer,
    pub tasks: TaskStats,
    pub face: Face,
    pub headings: Vec<Heading>,
    /// Relative local file references (images, attachments).
    pub file_refs: Vec<String>,
    pub word_count: usize,
    pub has_code: bool,
    /// Plain text used by search indexing (never sent to the UI).
    #[serde(skip)]
    pub plain: String,
}

static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)(?:^|[\s(\[,;])#([\p{L}\p{N}_/\-]+)").unwrap());
static MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)(?:^|[\s(\[,;])@([\p{L}\p{N}][\p{L}\p{N}_.\-]*)").unwrap());
static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\[\]\n]+?)\]\]").unwrap());
static DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d{4}-\d{2}-\d{2})(?:[ T](\d{1,2}:\d{2}))?\]").unwrap());
static TASK_LINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\].*)$").unwrap());

pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

/// Normalize content for writing: strip BOM, LF line endings, no backslash
/// hard breaks, single trailing newline.
pub fn normalize(content: &str) -> String {
    let s = content.strip_prefix('\u{feff}').unwrap_or(content);
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut s = clean_hard_breaks(&s).into_owned();
    while s.ends_with("\n\n") {
        s.pop();
    }
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Drop backslash hard breaks (`text\` at the end of a line). Luau renders
/// every line break as a break, so they only clutter the editor; older Jira
/// imports wrote them for each Shift+Enter. A line holding just `\` becomes
/// blank (a paragraph break). Fenced/indented code, math blocks and table
/// rows are left alone, as is a `\` that ends a paragraph (a literal there).
/// Idempotent; expects LF line endings.
pub fn clean_hard_breaks(content: &str) -> std::borrow::Cow<'_, str> {
    if !content.contains("\\\n") {
        return content.into();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out: Vec<std::borrow::Cow<str>> = Vec::with_capacity(lines.len());
    let mut fence: Option<(char, usize)> = None;
    let mut math = false;
    let mut indented_code = false;
    for (i, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some((ch, n)) = fence {
            if indent < 4
                && trimmed.starts_with(&ch.to_string().repeat(n))
                && trimmed.trim_start_matches(ch).trim().is_empty()
            {
                fence = None;
            }
            out.push(line.into());
            continue;
        }
        if indent < 4 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            let ch = trimmed.chars().next().unwrap_or('`');
            fence = Some((ch, trimmed.chars().take_while(|&c| c == ch).count()));
            out.push(line.into());
            continue;
        }
        if trimmed.trim_end() == "$$" {
            math = !math;
            out.push(line.into());
            continue;
        }
        let prev_blank = i == 0 || lines[i - 1].trim().is_empty();
        let code_indent = line.starts_with('\t') || indent >= 4;
        indented_code =
            code_indent && (prev_blank || indented_code) || indented_code && line.trim().is_empty();
        let next_text = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
        let slashes = line.len() - line.trim_end_matches('\\').len();
        let keep =
            math || slashes % 2 == 0 || !next_text || trimmed.starts_with('|') || indented_code;
        if keep {
            out.push(line.into());
            continue;
        }
        let cut = &line[..line.len() - 1];
        // A lone `\` (possibly after `>` quote markers) ends the paragraph.
        if cut.trim_end().trim_end_matches(['>', ' ']).is_empty() {
            out.push(cut.trim_end().into());
        } else {
            out.push(cut.into());
        }
    }
    out.join("\n").into()
}

/// Title from line 0 if it is an H1 (`# Title`).
pub fn title_line(first_line: &str) -> Option<String> {
    let t = first_line.trim_start();
    if t == "#" {
        return Some(String::new());
    }
    t.strip_prefix("# ")
        .map(|r| r.trim().trim_end_matches('#').trim().to_string())
}

/// Replace (or insert) the title line of `content`.
pub fn set_title(content: &str, title: &str) -> String {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let title = title.replace(['\n', '\r'], " ");
    let (first, rest) = content.split_once('\n').unwrap_or((content, ""));
    if title_line(first).is_some() {
        format!("# {}\n{}", title.trim(), rest)
    } else {
        format!("# {}\n\n{}", title.trim(), content)
    }
}

/// Flip the checkbox on `line` (0-based). Returns `None` when the line is not a task.
pub fn toggle_task(content: &str, line: usize) -> Option<String> {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    let l = lines.get(line)?;
    let caps = TASK_LINE_RE.captures(l.trim_end_matches('\r'))?;
    let next = if &caps[2] == " " { "x" } else { " " };
    lines[line] = format!("{}{}{}", &caps[1], next, &caps[3]);
    Some(lines.join("\n"))
}

struct LineIndex(Vec<usize>);

impl LineIndex {
    fn new(s: &str) -> Self {
        let mut v = vec![0];
        v.extend(s.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex(v)
    }
    fn line_of(&self, offset: usize) -> usize {
        match self.0.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

fn is_local_ref(url: &str) -> bool {
    !(url.is_empty()
        || url.starts_with('#')
        || url.contains("://")
        || url.starts_with("mailto:")
        || url.starts_with("tel:")
        || url.starts_with("data:")
        || url.starts_with('/'))
}

fn in_ranges(pos: usize, ranges: &[Range<usize>]) -> bool {
    ranges.iter().any(|r| r.contains(&pos))
}

fn push_unique_ci(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x.eq_ignore_ascii_case(s)) {
        v.push(s.to_string());
    }
}

/// Parse a full card document.
#[allow(clippy::type_complexity)]
pub fn parse(content: &str) -> ParsedCard {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let src: std::borrow::Cow<str> = if content.contains('\r') {
        content.replace("\r\n", "\n").into()
    } else {
        content.into()
    };
    let lines: Vec<&str> = src.split('\n').collect();
    let footer = footer::parse(&lines);
    let body_end_line = footer.start_line.unwrap_or(lines.len());
    let li = LineIndex::new(&src);
    let body_end =
        li.0.get(body_end_line)
            .copied()
            .unwrap_or(src.len())
            .min(src.len());
    let body = &src[..body_end];

    let mut out = ParsedCard {
        footer,
        ..Default::default()
    };
    if let Some(t) = lines.first().and_then(|l| title_line(l)) {
        out.title = t;
        out.has_title_line = true;
    }

    // --- structural pass -------------------------------------------------
    let mut masks: Vec<Range<usize>> = Vec::new();
    let mut code_block_start: Option<usize> = None;
    let mut heading: Option<(u8, usize, String)> = None;
    let mut list_depth = 0usize;
    let mut para_text = String::new();
    let mut in_para = false;
    let mut plain = String::new();

    // face capture state
    let mut face: Option<Face> = None;
    let mut cap_list: Option<(bool, Vec<FaceItem>, usize)> = None; // (ordered, items, total)
    let mut cur_item: Option<FaceItem> = None;
    let mut cap_table: Option<(Vec<String>, Vec<Vec<String>>, usize, bool)> = None; // header, rows, total, in_head
    let mut cur_row: Vec<String> = Vec::new();
    let mut cur_cell: Option<String> = None;
    let mut paragraphs: Vec<String> = Vec::new();

    for (ev, range) in Parser::new_ext(body, options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::CodeBlock(_)) => {
                code_block_start = Some(range.start);
                out.has_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(s) = code_block_start.take() {
                    masks.push(s..range.end.max(s + 1));
                }
            }
            Event::Code(t) => {
                masks.push(range.clone());
                plain.push_str(&t);
                plain.push(' ');
                if let Some(c) = cur_cell.as_mut() {
                    c.push_str(&t);
                } else if let Some(i) = cur_item.as_mut() {
                    i.text.push_str(&t);
                } else if in_para {
                    para_text.push_str(&t);
                }
            }
            Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::Html(_)
            | Event::InlineHtml(_) => {
                masks.push(range.clone());
            }
            Event::Start(Tag::Link { dest_url, .. })
            | Event::Start(Tag::Image { dest_url, .. }) => {
                masks.push(range.clone());
                let url = dest_url.split(['#', '?']).next().unwrap_or("");
                if is_local_ref(url) {
                    let decoded = percent_encoding::percent_decode_str(url)
                        .decode_utf8_lossy()
                        .into_owned();
                    if !out.file_refs.contains(&decoded) {
                        out.file_refs.push(decoded);
                    }
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level as u8, li.line_of(range.start), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, line, text)) = heading.take() {
                    let text = text.trim().to_string();
                    if out.title.is_empty() && level == 1 && !out.has_title_line {
                        out.title = text.clone();
                    }
                    plain.push_str(&text);
                    plain.push('\n');
                    out.headings.push(Heading { level, text, line });
                }
            }
            Event::Start(Tag::Paragraph) => {
                in_para = true;
                para_text.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                in_para = false;
                if list_depth == 0 && cap_table.is_none() {
                    let t = para_text.trim().to_string();
                    if !t.is_empty() {
                        paragraphs.push(t);
                    }
                }
                plain.push('\n');
            }
            Event::Start(Tag::List(first)) => {
                list_depth += 1;
                if list_depth == 1 && face.is_none() && cap_list.is_none() && cap_table.is_none() {
                    cap_list = Some((first.is_some(), Vec::new(), 0));
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0
                    && let Some((ordered, items, total)) = cap_list.take()
                    && !items.is_empty()
                {
                    let is_tasks = items.iter().any(|i| i.task.is_some());
                    face = Some(if is_tasks {
                        Face::Checklist { items, total }
                    } else {
                        Face::List {
                            items,
                            total,
                            ordered,
                        }
                    });
                }
            }
            Event::Start(Tag::Item) => {
                if list_depth == 1 && cap_list.is_some() {
                    cur_item = Some(FaceItem {
                        text: String::new(),
                        task: None,
                        line: li.line_of(range.start),
                    });
                }
            }
            Event::End(TagEnd::Item) => {
                if list_depth == 1
                    && let (Some(item), Some((_, items, total))) =
                        (cur_item.take(), cap_list.as_mut())
                {
                    *total += 1;
                    if items.len() < FACE_MAX_ITEMS {
                        items.push(FaceItem {
                            text: item.text.trim().to_string(),
                            ..item
                        });
                    }
                }
                plain.push('\n');
            }
            Event::TaskListMarker(checked) => {
                out.tasks.total += 1;
                if checked {
                    out.tasks.done += 1;
                }
                if list_depth == 1
                    && let Some(item) = cur_item.as_mut()
                {
                    item.task = Some(checked);
                }
            }
            Event::Start(Tag::Table(_)) => {
                if face.is_none() && cap_list.is_none() {
                    cap_table = Some((Vec::new(), Vec::new(), 0, false));
                }
            }
            Event::Start(Tag::TableHead) => {
                if let Some(t) = cap_table.as_mut() {
                    t.3 = true;
                }
                cur_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = cap_table.as_mut() {
                    t.0 = std::mem::take(&mut cur_row);
                    t.3 = false;
                }
            }
            Event::Start(Tag::TableRow) => cur_row.clear(),
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = cap_table.as_mut() {
                    t.2 += 1;
                    if t.1.len() < FACE_MAX_ROWS {
                        t.1.push(std::mem::take(&mut cur_row));
                    }
                }
            }
            Event::Start(Tag::TableCell) => cur_cell = Some(String::new()),
            Event::End(TagEnd::TableCell) => {
                if let Some(c) = cur_cell.take() {
                    plain.push_str(&c);
                    plain.push(' ');
                    cur_row.push(c.trim().to_string());
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some((header, rows, total, _)) = cap_table.take() {
                    face = Some(Face::Table {
                        header,
                        rows,
                        total,
                    });
                }
                plain.push('\n');
            }
            Event::Text(t) => {
                if code_block_start.is_some() {
                    plain.push_str(&t);
                    continue;
                }
                if let Some((_, _, h)) = heading.as_mut() {
                    h.push_str(&t);
                    continue;
                }
                plain.push_str(&t);
                if let Some(c) = cur_cell.as_mut() {
                    c.push_str(&t);
                } else if let Some(i) = cur_item.as_mut() {
                    if list_depth == 1 {
                        i.text.push_str(&t);
                    }
                } else if in_para {
                    para_text.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                plain.push(' ');
                if in_para && cur_item.is_none() {
                    para_text.push(' ');
                } else if let Some(i) = cur_item.as_mut()
                    && list_depth == 1
                {
                    i.text.push(' ');
                }
            }
            _ => {}
        }
    }

    // --- token pass over raw source (tags, links, mentions, dates) -------
    let line_of = |pos: usize| li.line_of(pos);
    for m in LINK_RE.captures_iter(body) {
        let whole = m.get(0).unwrap();
        if in_ranges(whole.start(), &masks) {
            continue;
        }
        let inner = m[2].trim();
        let (target_part, alias) = match inner.split_once('|') {
            Some((t, a)) => (t.trim(), Some(a.trim().to_string())),
            None => (inner, None),
        };
        let (target, heading) = match target_part.split_once('#') {
            Some((t, h)) => (t.trim(), Some(h.trim().to_string())),
            None => (target_part, None),
        };
        if target.is_empty() {
            continue;
        }
        let id = is_id(target, IdKind::Card).then(|| target.to_string());
        out.links.push(LinkRef {
            id,
            target: target.to_string(),
            heading,
            alias,
            embed: &m[1] == "!",
            line: line_of(whole.start()),
        });
        masks.push(whole.range());
    }
    for m in TAG_RE.captures_iter(body) {
        let g = m.get(1).unwrap();
        let hash = g.start() - 1;
        if in_ranges(hash, &masks) {
            continue;
        }
        let tag = g.as_str().trim_end_matches(['/', '-']);
        if tag.is_empty() || tag.starts_with('/') || tag.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        push_unique_ci(&mut out.tags, tag);
    }
    for m in MENTION_RE.captures_iter(body) {
        let g = m.get(1).unwrap();
        if in_ranges(g.start(), &masks) {
            continue;
        }
        let name = g.as_str().trim_end_matches(['.', '-']);
        if !name.is_empty() {
            push_unique_ci(&mut out.mentions, name);
        }
    }
    for m in DATE_RE.captures_iter(body) {
        let whole = m.get(0).unwrap();
        if in_ranges(whole.start(), &masks) {
            continue;
        }
        let before = body[..whole.start()].chars().last();
        let after = body[whole.end()..].chars().next();
        if before == Some('[') || matches!(after, Some('(') | Some('[') | Some(':') | Some(']')) {
            continue;
        }
        let d = m[1].to_string();
        if chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_ok() && !out.dates.contains(&d) {
            out.dates.push(d);
        }
    }
    for a in &out.footer.assignees {
        push_unique_ci(&mut out.mentions, a);
    }

    // --- face fallback: summary -----------------------------------------
    out.face = match face {
        Some(f) => f,
        None => {
            let text = paragraphs.join("\n");
            let s = summary::summarize(&text, 2, SUMMARY_MAX_CHARS);
            if s.is_empty() {
                Face::None
            } else {
                Face::Summary { text: s }
            }
        }
    };
    out.word_count = plain.split_whitespace().count();
    out.plain = plain;
    out
}

/// Extract the title only (cheap path).
pub fn quick_title(content: &str) -> String {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let first = content
        .split('\n')
        .next()
        .unwrap_or("")
        .trim_end_matches('\r');
    title_line(first).unwrap_or_else(|| parse(content).title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_first_line() {
        let p = parse("# Fix login bug\n\nSome body text.\n");
        assert_eq!(p.title, "Fix login bug");
        assert!(p.has_title_line);
    }

    #[test]
    fn title_fallback_to_first_h1() {
        let p = parse("intro\n\n# Real title\n");
        assert_eq!(p.title, "Real title");
        assert!(!p.has_title_line);
        assert_eq!(parse("no heading at all\n").title, "");
    }

    #[test]
    fn tags_basic_and_exclusions() {
        let src = "# T #notatitletag\n\nHello #urgent and #q4_goals, #work/client-a.\n\
                   `#code`\n\n```\n#fenced\n```\n\n[link](#anchor) https://x.com/#frag #123 ##double\n\
                   $#math$ (#paren)\n";
        let p = parse(src);
        assert!(p.tags.contains(&"urgent".to_string()));
        assert!(p.tags.contains(&"q4_goals".to_string()));
        assert!(p.tags.contains(&"work/client-a".to_string()));
        assert!(p.tags.contains(&"paren".to_string()));
        assert!(p.tags.contains(&"notatitletag".to_string()));
        for bad in ["code", "fenced", "anchor", "frag", "123", "double", "math"] {
            assert!(
                !p.tags.iter().any(|t| t == bad),
                "unexpected tag {bad}: {:?}",
                p.tags
            );
        }
    }

    #[test]
    fn tags_are_deduped_case_insensitively() {
        let p = parse("# T\n\n#Urgent #urgent #URGENT\n");
        assert_eq!(p.tags, vec!["Urgent"]);
    }

    #[test]
    fn unicode_tags() {
        let p = parse("# T\n\n#reunión #açaí #日本\n");
        assert_eq!(p.tags, vec!["reunión", "açaí", "日本"]);
    }

    #[test]
    fn links_and_embeds() {
        let p = parse(
            "# T\n\nSee [[c8x1q0a]] and ![[c2mz7pb#Plan|the plan]] or [[Some Title]].\n`[[c0000000]]`\n",
        );
        assert_eq!(p.links.len(), 3);
        assert_eq!(p.links[0].id.as_deref(), Some("c8x1q0a"));
        assert!(!p.links[0].embed);
        assert_eq!(p.links[1].id.as_deref(), Some("c2mz7pb"));
        assert!(p.links[1].embed);
        assert_eq!(p.links[1].heading.as_deref(), Some("Plan"));
        assert_eq!(p.links[1].alias.as_deref(), Some("the plan"));
        assert_eq!(p.links[2].id, None);
        assert_eq!(p.links[2].target, "Some Title");
        assert_eq!(p.links[0].line, 2);
    }

    #[test]
    fn mentions_and_dates() {
        let p = parse(
            "# T\n\nAsk @ana and @luis.p about [2026-10-03], not a@b.com or [2026-13-40] or [x](y) [2026-01-02](url)\n",
        );
        assert_eq!(p.mentions, vec!["ana", "luis.p"]);
        assert_eq!(p.dates, vec!["2026-10-03"]);
    }

    #[test]
    fn face_checklist() {
        let src = "# T\n\nIntro paragraph.\n\n- [ ] one\n- [x] two\n  - [ ] nested\n- [ ] three\n";
        let p = parse(src);
        assert_eq!(p.tasks, TaskStats { total: 4, done: 1 });
        match p.face {
            Face::Checklist { items, total } => {
                assert_eq!(total, 3);
                assert_eq!(items[0].text, "one");
                assert_eq!(items[1].task, Some(true));
                assert_eq!(items[1].line, 5);
                assert_eq!(items[2].text, "three");
            }
            f => panic!("unexpected face {f:?}"),
        }
    }

    #[test]
    fn face_list_and_table() {
        let p = parse("# T\n\n1. a\n2. b\n");
        assert!(matches!(
            p.face,
            Face::List {
                ordered: true,
                total: 2,
                ..
            }
        ));
        let p = parse("# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        match p.face {
            Face::Table {
                header,
                rows,
                total,
            } => {
                assert_eq!(header, vec!["A", "B"]);
                assert_eq!(rows, vec![vec!["1", "2"], vec!["3", "4"]]);
                assert_eq!(total, 2);
            }
            f => panic!("unexpected face {f:?}"),
        }
    }

    #[test]
    fn face_summary_and_none() {
        let p = parse("# T\n\nThis is the body. It explains things.\n");
        assert!(matches!(p.face, Face::Summary { .. }));
        assert_eq!(parse("# Only title\n").face, Face::None);
    }

    #[test]
    fn footer_excluded_from_body_and_feeds_mentions() {
        let p = parse("# T\n\nbody #tag\n\n---\npriority: high\nassignees: @ana\nlabels: x\n");
        assert_eq!(p.footer.priority.as_deref(), Some("high"));
        assert_eq!(p.tags, vec!["tag"]);
        assert_eq!(p.mentions, vec!["ana"]);
    }

    #[test]
    fn file_refs() {
        let p = parse(
            "# T\n\n![](c8x1q0a.3f9a.png) [doc](c8x1q0a.ab12.pdf) [w](https://x.com) ![](my%20pic.png)\n",
        );
        assert_eq!(
            p.file_refs,
            vec!["c8x1q0a.3f9a.png", "c8x1q0a.ab12.pdf", "my pic.png"]
        );
    }

    #[test]
    fn toggles_tasks() {
        let src = "# T\n- [ ] a\n- [x] b\nnot a task\n";
        assert_eq!(
            toggle_task(src, 1).unwrap(),
            "# T\n- [x] a\n- [x] b\nnot a task\n"
        );
        assert_eq!(
            toggle_task(src, 2).unwrap(),
            "# T\n- [ ] a\n- [ ] b\nnot a task\n"
        );
        assert!(toggle_task(src, 3).is_none());
        assert!(toggle_task(src, 99).is_none());
    }

    #[test]
    fn hard_breaks_are_cleaned() {
        // Old Jira import: Shift+Enter, then an empty line made of a break.
        assert_eq!(
            clean_hard_breaks("a long line.\\\n\\\nPropuesta:\n\n1. x\n"),
            "a long line.\n\nPropuesta:\n\n1. x\n"
        );
        assert_eq!(clean_hard_breaks("a\\\nb\n"), "a\nb\n");
        assert_eq!(clean_hard_breaks("> q\\\n> \\\n> r\n"), "> q\n>\n> r\n");
        // Escaped backslash, paragraph end, code, math and tables stay.
        for keep in [
            "C:\\\\\nnext\n",
            "ends with \\\n\nnext\n",
            "```\na\\\nb\n```\n",
            "~~~~sh\nx \\\n  --y\n~~~~\n",
            "$$\na \\\\\nb\\\nc\n$$\n",
            "| a\\\n| b\n",
            "para\n\n    code\\\n    more\\\n    end\n",
        ] {
            assert_eq!(clean_hard_breaks(keep), keep, "{keep:?}");
        }
        let once = clean_hard_breaks("x\\\\\\\ny\n").into_owned();
        assert_eq!(once, "x\\\\\ny\n");
        assert_eq!(clean_hard_breaks(&once), once);
        assert_eq!(normalize("a\\\r\nb"), "a\nb\n");
    }

    #[test]
    fn set_title_and_normalize() {
        assert_eq!(set_title("# Old\nbody\n", "New"), "# New\nbody\n");
        assert_eq!(set_title("body\n", "New"), "# New\n\nbody\n");
        assert_eq!(normalize("\u{feff}a\r\nb\r\n\n\n"), "a\nb\n");
        assert_eq!(normalize("x"), "x\n");
    }

    #[test]
    fn crlf_input() {
        let p = parse("# Title\r\n\r\n- [ ] a\r\n#tag\r\n");
        assert_eq!(p.title, "Title");
        assert_eq!(p.tags, vec!["tag"]);
    }
}
