//! Atlassian Document Format (Jira Cloud descriptions/comments) ↔ Markdown.
//!
//! Conventions used on the Markdown side:
//! - panels ↔ callouts `> [!info]` (`info|note|warning|success|error`)
//! - mentions ↔ `[@Name](mention:ACCOUNT_ID)`
//! - media ↔ `![file.png](jira-attachment:file.png)` (rewritten to local files on import)
//! - task lists ↔ `- [ ]` / `- [x]`

use pulldown_cmark::{BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::{Map, Value, json};

use super::md_util::{
    callout_kind, escape_inline, indent_rest, join_blocks, normalize_md, table_md,
};

pub const ATTACHMENT_SCHEME: &str = "jira-attachment:";
pub const MENTION_SCHEME: &str = "mention:";

// ---------------------------------------------------------------------------
// ADF → Markdown
// ---------------------------------------------------------------------------

/// Convert an ADF document (or any ADF node) to Markdown.
pub fn to_markdown(doc: &Value) -> String {
    if doc.is_null() {
        return String::new();
    }
    if let Some(s) = doc.as_str() {
        // Some endpoints return plain strings (e.g. legacy fields).
        return s.to_string();
    }
    let out = if doc.get("type").and_then(Value::as_str) == Some("doc") {
        blocks(children(doc))
    } else {
        block(doc).unwrap_or_default()
    };
    normalize_md(&out)
}

fn children(n: &Value) -> &[Value] {
    n.get("content")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn attr<'a>(n: &'a Value, k: &str) -> Option<&'a Value> {
    n.get("attrs").and_then(|a| a.get(k))
}

fn attr_str<'a>(n: &'a Value, k: &str) -> Option<&'a str> {
    attr(n, k).and_then(Value::as_str)
}

fn blocks(list: &[Value]) -> String {
    join_blocks(list.iter().filter_map(block).collect())
}

fn block(n: &Value) -> Option<String> {
    let ty = n.get("type").and_then(Value::as_str).unwrap_or("");
    Some(match ty {
        "paragraph" => inline(children(n)),
        "heading" => {
            let level = attr(n, "level")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), inline(children(n)))
        }
        "bulletList" => list(children(n), None),
        "orderedList" => list(
            children(n),
            Some(attr(n, "order").and_then(Value::as_u64).unwrap_or(1)),
        ),
        "taskList" | "decisionList" => task_list(children(n)),
        "codeBlock" => {
            let lang = attr_str(n, "language").unwrap_or("");
            let text: String = children(n)
                .iter()
                .filter_map(|t| t.get("text").and_then(Value::as_str))
                .collect();
            let fence = if text.contains("```") { "~~~~" } else { "```" };
            format!("{fence}{lang}\n{text}\n{fence}")
        }
        "blockquote" => quote(&blocks(children(n))),
        "rule" => "---".into(),
        "panel" => {
            let kind = match attr_str(n, "panelType").unwrap_or("info") {
                "note" => "note",
                "warning" => "warning",
                "success" => "success",
                "error" => "error",
                _ => "info",
            };
            callout(kind, "", &blocks(children(n)))
        }
        "expand" | "nestedExpand" => callout(
            "note",
            attr_str(n, "title").unwrap_or(""),
            &blocks(children(n)),
        ),
        "mediaSingle" | "mediaGroup" => children(n).iter().map(media).collect::<Vec<_>>().join(" "),
        "media" => media(n),
        "table" => table(n),
        "bodiedExtension" | "layoutSection" | "layoutColumn" => blocks(children(n)),
        "extension" | "inlineExtension" => return None,
        "blockCard" | "embedCard" => attr_str(n, "url")
            .map(|u| format!("<{u}>"))
            .unwrap_or_default(),
        _ if !children(n).is_empty() => {
            // Unknown container: inline if it holds text, otherwise blocks.
            if children(n)
                .iter()
                .any(|c| c.get("type").and_then(Value::as_str) == Some("text"))
            {
                inline(children(n))
            } else {
                blocks(children(n))
            }
        }
        _ => return None,
    })
}

fn quote(inner: &str) -> String {
    inner
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn callout(kind: &str, title: &str, inner: &str) -> String {
    let head = if title.is_empty() {
        format!("> [!{kind}]")
    } else {
        format!("> [!{kind}] {title}")
    };
    if inner.is_empty() {
        head
    } else {
        format!("{head}\n{}", quote(inner))
    }
}

fn list(items: &[Value], ordered: Option<u64>) -> String {
    let mut out = Vec::new();
    for (i, it) in items.iter().enumerate() {
        let marker = match ordered {
            Some(start) => format!("{}. ", start + i as u64),
            None => "- ".to_string(),
        };
        let body = list_item_body(children(it));
        out.push(format!("{marker}{}", indent_rest(&body, marker.len())));
    }
    out.join("\n")
}

/// List item content: blocks separated by single newlines (tight lists).
fn list_item_body(content: &[Value]) -> String {
    content
        .iter()
        .filter_map(block)
        .collect::<Vec<_>>()
        .join("\n")
}

fn task_list(items: &[Value]) -> String {
    let mut out = Vec::new();
    for it in items {
        match it.get("type").and_then(Value::as_str) {
            Some("taskItem") | Some("decisionItem") => {
                let done = attr_str(it, "state").is_some_and(|s| s == "DONE" || s == "DECIDED");
                let mark = if done { "- [x] " } else { "- [ ] " };
                out.push(format!("{mark}{}", inline(children(it))));
            }
            Some("taskList") | Some("decisionList") => {
                let nested = task_list(children(it));
                out.push(
                    nested
                        .lines()
                        .map(|l| format!("  {l}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            _ => {
                if let Some(b) = block(it) {
                    out.push(b);
                }
            }
        }
    }
    out.join("\n")
}

fn media(n: &Value) -> String {
    let alt = attr_str(n, "alt").filter(|s| !s.is_empty());
    match attr_str(n, "type") {
        Some("external") => {
            let url = attr_str(n, "url").unwrap_or("");
            format!("![{}]({url})", alt.unwrap_or("image"))
        }
        _ => {
            let name = alt.or_else(|| attr_str(n, "id")).unwrap_or("attachment");
            format!(
                "![{}]({ATTACHMENT_SCHEME}{})",
                name.replace(['[', ']'], ""),
                encode_target(name)
            )
        }
    }
}

fn encode_target(s: &str) -> String {
    s.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

fn table(n: &Value) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_first = false;
    for (i, row) in children(n).iter().enumerate() {
        let cells: Vec<String> = children(row)
            .iter()
            .map(|c| {
                if i == 0 && c.get("type").and_then(Value::as_str) == Some("tableHeader") {
                    header_first = true;
                }
                children(c)
                    .iter()
                    .filter_map(block)
                    .collect::<Vec<_>>()
                    .join("<br>")
                    .replace('\n', "<br>")
            })
            .collect();
        rows.push(cells);
    }
    table_md(&rows, header_first)
}

#[derive(Clone, PartialEq)]
enum Mark {
    Strong,
    Em,
    Code,
    Strike,
    Link(String),
}

fn marks_of(n: &Value) -> Vec<Mark> {
    let mut v = Vec::new();
    for m in n
        .get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match m.get("type").and_then(Value::as_str) {
            Some("strong") => v.push(Mark::Strong),
            Some("em") => v.push(Mark::Em),
            Some("code") => v.push(Mark::Code),
            Some("strike") => v.push(Mark::Strike),
            Some("link") => v.push(Mark::Link(attr_str(m, "href").unwrap_or("").to_string())),
            _ => {}
        }
    }
    v
}

fn wrap(text: &str, marks: &[Mark]) -> String {
    if text.is_empty() {
        return String::new();
    }
    if text.trim().is_empty() && !marks.contains(&Mark::Code) {
        return text.to_string();
    }
    let code = marks.contains(&Mark::Code);
    let mut s = if code {
        let tick = if text.contains('`') { "``" } else { "`" };
        format!("{tick}{text}{tick}")
    } else {
        escape_inline(text)
    };
    // Keep surrounding whitespace outside of emphasis markers.
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    let (pre, core, post) = (
        s[..lead].to_string(),
        s.trim().to_string(),
        s[s.len() - trail..].to_string(),
    );
    s = core;
    if marks.contains(&Mark::Strike) {
        s = format!("~~{s}~~");
    }
    if marks.contains(&Mark::Em) {
        s = format!("*{s}*");
    }
    if marks.contains(&Mark::Strong) {
        s = format!("**{s}**");
    }
    if let Some(Mark::Link(href)) = marks.iter().find(|m| matches!(m, Mark::Link(_))) {
        s = format!("[{s}]({href})");
    }
    format!("{pre}{s}{post}")
}

fn inline(list: &[Value]) -> String {
    let mut out = String::new();
    // Merge adjacent text nodes sharing the same marks.
    let mut pending: Option<(String, Vec<Mark>)> = None;
    let flush = |out: &mut String, p: &mut Option<(String, Vec<Mark>)>| {
        if let Some((t, m)) = p.take() {
            out.push_str(&wrap(&t, &m));
        }
    };
    for n in list {
        match n.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" => {
                let t = n.get("text").and_then(Value::as_str).unwrap_or("");
                let m = marks_of(n);
                match &mut pending {
                    Some((pt, pm)) if *pm == m => pt.push_str(t),
                    _ => {
                        flush(&mut out, &mut pending);
                        pending = Some((t.to_string(), m));
                    }
                }
            }
            other => {
                flush(&mut out, &mut pending);
                match other {
                    "hardBreak" => out.push_str("\\\n"),
                    "mention" => {
                        let id = attr_str(n, "id").unwrap_or("");
                        let text = attr_str(n, "text").unwrap_or("").trim_start_matches('@');
                        let text = if text.is_empty() { id } else { text };
                        out.push_str(&format!("[@{text}]({MENTION_SCHEME}{id})"));
                    }
                    "emoji" => out.push_str(
                        attr_str(n, "text")
                            .or_else(|| attr_str(n, "shortName"))
                            .unwrap_or(""),
                    ),
                    "inlineCard" => {
                        if let Some(u) = attr_str(n, "url") {
                            out.push_str(&format!("<{u}>"));
                        }
                    }
                    "date" => {
                        let ts = attr(n, "timestamp").and_then(|v| {
                            v.as_str()
                                .and_then(|s| s.parse::<i64>().ok())
                                .or_else(|| v.as_i64())
                        });
                        if let Some(d) = ts.and_then(chrono::DateTime::from_timestamp_millis) {
                            out.push_str(&d.format("%Y-%m-%d").to_string());
                        }
                    }
                    "status" => out.push_str(&format!("`{}`", attr_str(n, "text").unwrap_or(""))),
                    "mediaInline" | "media" => out.push_str(&media(n)),
                    "placeholder" => {}
                    _ => out.push_str(&inline(children(n))),
                }
            }
        }
    }
    flush(&mut out, &mut pending);
    out
}

// ---------------------------------------------------------------------------
// Markdown → ADF
// ---------------------------------------------------------------------------

struct Frame {
    node: Map<String, Value>,
    content: Vec<Value>,
    /// Paragraph opened automatically (tight list items, table cells).
    implicit: bool,
    task: Option<bool>,
    /// GFM alert kind of a blockquote (`> [!NOTE]`), already mapped to a panel type.
    gfm: Option<&'static str>,
}

impl Frame {
    fn new(ty: &str, attrs: Option<Value>) -> Self {
        let mut node = Map::new();
        node.insert("type".into(), json!(ty));
        if let Some(a) = attrs {
            node.insert("attrs".into(), a);
        }
        Frame {
            node,
            content: vec![],
            implicit: false,
            task: None,
            gfm: None,
        }
    }
    fn ty(&self) -> &str {
        self.node.get("type").and_then(Value::as_str).unwrap_or("")
    }
    fn finish(mut self) -> Value {
        let is_leaf = matches!(self.ty(), "rule");
        if !is_leaf {
            self.node
                .insert("content".into(), Value::Array(self.content));
        }
        Value::Object(self.node)
    }
}

struct Builder {
    stack: Vec<Frame>,
    marks: Vec<Value>,
    /// Collecting a mention link's text.
    mention: Option<(String, String)>,
    in_code_block: bool,
    local_id: u32,
}

const INLINE_PARENTS: &[&str] = &["paragraph", "heading", "codeBlock"];

impl Builder {
    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("doc frame")
    }
    fn push(&mut self, f: Frame) {
        self.stack.push(f);
    }
    fn pop_into_parent(&mut self) {
        let f = self.stack.pop().expect("frame");
        let v = f.finish();
        if self.stack.is_empty() {
            self.stack.push(Frame::new("doc", None));
        }
        // Empty paragraphs are dropped (ADF allows them but they add noise).
        if v["type"] == "paragraph" && v["content"].as_array().is_some_and(|c| c.is_empty()) {
            return;
        }
        self.top().content.push(v);
    }
    fn close_implicit(&mut self) {
        while self.stack.last().is_some_and(|f| f.implicit) {
            self.pop_into_parent();
        }
    }
    fn ensure_inline(&mut self) {
        let ty = self
            .stack
            .last()
            .map(|f| f.ty().to_string())
            .unwrap_or_default();
        if !INLINE_PARENTS.contains(&ty.as_str()) {
            let mut p = Frame::new("paragraph", None);
            p.implicit = true;
            self.push(p);
        }
    }
    fn text(&mut self, t: &str) {
        if t.is_empty() {
            return;
        }
        if let Some((_, text)) = &mut self.mention {
            text.push_str(t);
            return;
        }
        if self.in_code_block {
            let top = self.top();
            if let Some(last) = top.content.last_mut()
                && let Some(s) = last.get("text").and_then(Value::as_str)
            {
                let joined = format!("{s}{t}");
                last["text"] = json!(joined);
                return;
            }
            top.content.push(json!({ "type": "text", "text": t }));
            return;
        }
        self.ensure_inline();
        let marks = self.marks.clone();
        let top = self.top();
        // Merge with the previous text node when marks are identical.
        if let Some(last) = top.content.last_mut()
            && last["type"] == "text"
            && last.get("marks").cloned().unwrap_or(json!([])) == json!(marks)
        {
            let joined = format!("{}{t}", last["text"].as_str().unwrap_or(""));
            last["text"] = json!(joined);
            return;
        }
        let mut node = json!({ "type": "text", "text": t });
        if !marks.is_empty() {
            node["marks"] = json!(marks);
        }
        top.content.push(node);
    }
    fn inline_node(&mut self, v: Value) {
        self.ensure_inline();
        self.top().content.push(v);
    }
    fn next_local_id(&mut self) -> String {
        self.local_id += 1;
        format!("t{}", self.local_id)
    }
}

/// Convert Markdown to an ADF document.
pub fn from_markdown(md: &str) -> Value {
    let mut b = Builder {
        stack: vec![Frame::new("doc", None)],
        marks: vec![],
        mention: None,
        in_code_block: false,
        local_id: 0,
    };
    let parser = Parser::new_ext(md, crate::markdown::options());
    for ev in parser {
        match ev {
            Event::Start(tag) => start(&mut b, tag),
            Event::End(tag) => end(&mut b, tag),
            Event::Text(t) => b.text(&t),
            Event::Code(t) => {
                let mut marks: Vec<Value> = b
                    .marks
                    .iter()
                    .filter(|m| m["type"] == "link")
                    .cloned()
                    .collect();
                marks.insert(0, json!({ "type": "code" }));
                b.inline_node(json!({ "type": "text", "text": t.to_string(), "marks": marks }));
            }
            // Kept as "\n" so callout titles can be told apart; turned into
            // spaces by `postprocess`.
            Event::SoftBreak => b.text("\n"),
            Event::HardBreak => b.inline_node(json!({ "type": "hardBreak" })),
            Event::Rule => {
                b.close_implicit();
                b.top().content.push(json!({ "type": "rule" }));
            }
            Event::TaskListMarker(done) => {
                if let Some(item) = b.stack.iter_mut().rev().find(|f| f.ty() == "listItem") {
                    item.task = Some(done);
                }
            }
            Event::Html(t) | Event::InlineHtml(t) => b.text(t.trim_end_matches('\n')),
            Event::FootnoteReference(t) => b.text(&format!("[^{t}]")),
            Event::InlineMath(t) | Event::DisplayMath(t) => b.text(&t),
        }
    }
    while b.stack.len() > 1 {
        b.pop_into_parent();
    }
    let mut doc = b.stack.pop().unwrap().finish();
    doc["version"] = json!(1);
    postprocess(&mut doc);
    doc
}

fn start(b: &mut Builder, tag: Tag) {
    match tag {
        Tag::Paragraph => {
            b.close_implicit();
            b.push(Frame::new("paragraph", None));
        }
        Tag::Heading { level, .. } => {
            b.close_implicit();
            let l = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
            };
            b.push(Frame::new("heading", Some(json!({ "level": l }))));
        }
        Tag::BlockQuote(kind) => {
            b.close_implicit();
            let mut f = Frame::new("blockquote", None);
            f.gfm = kind.map(|k| match k {
                BlockQuoteKind::Note => "note",
                BlockQuoteKind::Tip => "success",
                BlockQuoteKind::Important => "info",
                BlockQuoteKind::Warning => "warning",
                BlockQuoteKind::Caution => "error",
            });
            b.push(f);
        }
        Tag::CodeBlock(kind) => {
            b.close_implicit();
            let lang = match kind {
                CodeBlockKind::Fenced(l) => l.split_whitespace().next().unwrap_or("").to_string(),
                CodeBlockKind::Indented => String::new(),
            };
            let attrs = if lang.is_empty() {
                None
            } else {
                Some(json!({ "language": lang }))
            };
            b.push(Frame::new("codeBlock", attrs));
            b.in_code_block = true;
        }
        Tag::List(start) => {
            b.close_implicit();
            match start {
                Some(n) => b.push(Frame::new("orderedList", Some(json!({ "order": n })))),
                None => b.push(Frame::new("bulletList", None)),
            }
        }
        Tag::Item => {
            b.close_implicit();
            b.push(Frame::new("listItem", None));
        }
        Tag::Table(_) => {
            b.close_implicit();
            b.push(Frame::new(
                "table",
                Some(json!({ "isNumberColumnEnabled": false, "layout": "default" })),
            ));
        }
        Tag::TableHead => {
            let mut f = Frame::new("tableRow", None);
            f.task = Some(true); // marks header row
            b.push(f);
        }
        Tag::TableRow => b.push(Frame::new("tableRow", None)),
        Tag::TableCell => {
            let header = b.stack.last().is_some_and(|f| f.task == Some(true));
            b.push(Frame::new(
                if header { "tableHeader" } else { "tableCell" },
                None,
            ));
        }
        Tag::Emphasis => b.marks.push(json!({ "type": "em" })),
        Tag::Strong => b.marks.push(json!({ "type": "strong" })),
        Tag::Strikethrough => b.marks.push(json!({ "type": "strike" })),
        Tag::Link { dest_url, .. } => {
            if let Some(id) = dest_url.strip_prefix(MENTION_SCHEME) {
                b.mention = Some((id.to_string(), String::new()));
            } else {
                b.marks
                    .push(json!({ "type": "link", "attrs": { "href": dest_url.to_string() } }));
            }
        }
        Tag::Image { dest_url, .. } => {
            // Images need an uploaded media id; external URLs become links.
            let url = dest_url.to_string();
            b.marks
                .push(json!({ "type": "link", "attrs": { "href": url } }));
        }
        _ => {}
    }
}

fn end(b: &mut Builder, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph | TagEnd::Heading(_) => {
            b.close_implicit();
            b.pop_into_parent();
        }
        TagEnd::CodeBlock => {
            b.in_code_block = false;
            let top = b.top();
            if let Some(last) = top.content.last_mut() {
                let t = last["text"]
                    .as_str()
                    .unwrap_or("")
                    .trim_end_matches('\n')
                    .to_string();
                last["text"] = json!(t);
            }
            top.content
                .retain(|c| c["text"].as_str().is_some_and(|s| !s.is_empty()));
            b.pop_into_parent();
        }
        TagEnd::BlockQuote(_) => {
            b.close_implicit();
            let f = b.stack.pop().expect("blockquote");
            let gfm = f.gfm;
            let v = f.finish();
            let v = match gfm {
                Some(kind) => panel(kind, v["content"].as_array().cloned().unwrap_or_default()),
                None => to_panel_if_callout(v),
            };
            b.top().content.push(v);
        }
        TagEnd::Item => {
            b.close_implicit();
            let f = b.stack.pop().expect("item");
            let task = f.task;
            let mut v = f.finish();
            if let Some(done) = task {
                v["attrs"] = json!({ "state": if done { "DONE" } else { "TODO" } });
                v["__task"] = json!(true);
            }
            b.top().content.push(v);
        }
        TagEnd::List(_) => {
            b.close_implicit();
            let f = b.stack.pop().expect("list");
            let is_task =
                !f.content.is_empty() && f.content.iter().all(|i| i.get("__task").is_some());
            let v = if is_task {
                task_list_from(b, f.content)
            } else {
                strip_task_flags(f.finish())
            };
            b.top().content.push(v);
        }
        TagEnd::Table => {
            let f = b.stack.pop().expect("table");
            let v = f.finish();
            b.top().content.push(v);
        }
        TagEnd::TableHead | TagEnd::TableRow => b.pop_into_parent(),
        TagEnd::TableCell => {
            b.close_implicit();
            let f = b.stack.pop().expect("cell");
            let mut v = f.finish();
            // Cells need block content: wrap loose inline nodes in a paragraph.
            let content = v["content"].as_array().cloned().unwrap_or_default();
            if content
                .iter()
                .any(|c| c["type"] == "text" || c["type"] == "hardBreak")
                || content.is_empty()
            {
                v["content"] = json!([{ "type": "paragraph", "content": content }]);
            }
            b.top().content.push(v);
        }
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
            b.marks.pop();
        }
        TagEnd::Link => {
            if let Some((id, text)) = b.mention.take() {
                b.inline_node(json!({ "type": "mention", "attrs": { "id": id, "text": format!("@{}", text.trim_start_matches('@')) } }));
            } else {
                b.marks.pop();
            }
        }
        TagEnd::Image => {
            b.marks.pop();
        }
        _ => {}
    }
}

fn strip_task_flags(mut v: Value) -> Value {
    if let Some(items) = v.get_mut("content").and_then(Value::as_array_mut) {
        for it in items {
            if let Some(o) = it.as_object_mut() {
                o.remove("__task");
                if o.get("type").and_then(Value::as_str) == Some("listItem") {
                    o.remove("attrs");
                }
            }
        }
    }
    v
}

/// listItems (all tasks) → taskList with inline taskItems; nested lists become siblings.
fn task_list_from(b: &mut Builder, items: Vec<Value>) -> Value {
    let mut content = Vec::new();
    for it in items {
        let state = it["attrs"]["state"].as_str().unwrap_or("TODO").to_string();
        let blocks = it["content"].as_array().cloned().unwrap_or_default();
        let mut inline_nodes = Vec::new();
        let mut extra = Vec::new();
        for (i, blk) in blocks.into_iter().enumerate() {
            if i == 0 && blk["type"] == "paragraph" {
                inline_nodes = blk["content"].as_array().cloned().unwrap_or_default();
            } else if blk["type"] == "taskList" {
                extra.push(blk);
            } else if blk["type"] == "paragraph" {
                inline_nodes.push(json!({ "type": "hardBreak" }));
                inline_nodes.extend(blk["content"].as_array().cloned().unwrap_or_default());
            }
        }
        content.push(json!({ "type": "taskItem", "attrs": { "localId": b.next_local_id(), "state": state }, "content": inline_nodes }));
        content.extend(extra);
    }
    json!({ "type": "taskList", "attrs": { "localId": b.next_local_id() }, "content": content })
}

/// `> [!warning] Title` blockquotes become ADF panels.
fn to_panel_if_callout(v: Value) -> Value {
    let Some(first) = v["content"].as_array().and_then(|c| c.first()).cloned() else {
        return v;
    };
    if first["type"] != "paragraph" {
        return v;
    }
    let Some(t) = first["content"]
        .as_array()
        .and_then(|c| c.first())
        .and_then(|n| n["text"].as_str())
        .map(str::to_string)
    else {
        return v;
    };
    let Some((kind, rest)) = callout_kind(&t) else {
        return v;
    };
    let panel_type = match kind.as_str() {
        "note" => "note",
        "warning" | "caution" | "attention" => "warning",
        "success" | "tip" | "check" | "done" => "success",
        "error" | "danger" | "failure" | "bug" => "error",
        _ => "info",
    };
    // The title runs until the first soft break.
    let (title, remainder) = match rest.split_once('\n') {
        Some((a, b)) => (a.trim().to_string(), b.trim_start().to_string()),
        None => (rest.trim().to_string(), String::new()),
    };
    let mut blocks = v["content"].as_array().cloned().unwrap_or_default();
    let mut para = blocks.remove(0);
    let mut inl = para["content"].as_array().cloned().unwrap_or_default();
    if remainder.is_empty() {
        inl.remove(0);
        // A marker line followed by a hard break: drop the break too.
        if inl.first().is_some_and(|n| n["type"] == "hardBreak") {
            inl.remove(0);
        }
    } else {
        inl[0]["text"] = json!(remainder);
    }
    if !title.is_empty() {
        let mut head =
            vec![json!({ "type": "text", "text": title, "marks": [{ "type": "strong" }] })];
        if !inl.is_empty() {
            head.push(json!({ "type": "hardBreak" }));
        }
        head.extend(inl);
        inl = head;
    }
    if !inl.is_empty() {
        para["content"] = json!(inl);
        blocks.insert(0, para);
    }
    panel(panel_type, blocks)
}

fn panel(panel_type: &str, mut blocks: Vec<Value>) -> Value {
    // Panels only allow a subset of blocks.
    blocks.retain(|b| {
        matches!(
            b["type"].as_str(),
            Some(
                "paragraph"
                    | "heading"
                    | "bulletList"
                    | "orderedList"
                    | "taskList"
                    | "codeBlock"
                    | "rule"
            )
        )
    });
    json!({ "type": "panel", "attrs": { "panelType": panel_type }, "content": blocks })
}

/// Remove internal markers left in the tree; soft breaks become spaces
/// (outside code blocks).
fn postprocess(v: &mut Value) {
    fn walk(v: &mut Value, in_code: bool) {
        match v {
            Value::Object(o) => {
                o.remove("__task");
                let code = in_code || o.get("type").and_then(Value::as_str) == Some("codeBlock");
                if !code
                    && o.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(Value::String(t)) = o.get_mut("text")
                    && t.contains('\n')
                {
                    *t = t.replace('\n', " ");
                }
                for (_, x) in o.iter_mut() {
                    walk(x, code);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, in_code)),
            _ => {}
        }
    }
    walk(v, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(content: Value) -> Value {
        json!({ "type": "doc", "version": 1, "content": content })
    }
    fn p(content: Value) -> Value {
        json!({ "type": "paragraph", "content": content })
    }
    fn t(s: &str) -> Value {
        json!({ "type": "text", "text": s })
    }
    fn tm(s: &str, marks: &[&str]) -> Value {
        json!({ "type": "text", "text": s, "marks": marks.iter().map(|m| json!({"type": m})).collect::<Vec<_>>() })
    }

    #[test]
    fn headings_and_paragraphs() {
        let d = doc(json!([
            { "type": "heading", "attrs": { "level": 2 }, "content": [t("Title")] },
            p(json!([t("Hello "), tm("bold", &["strong"]), t(" and "), tm("it", &["em"])])),
        ]));
        assert_eq!(to_markdown(&d), "## Title\n\nHello **bold** and *it*");
    }

    #[test]
    fn emphasis_keeps_spaces_outside() {
        let d = doc(json!([p(json!([tm("bold ", &["strong"]), t("x")]))]));
        assert_eq!(to_markdown(&d), "**bold** x");
    }

    #[test]
    fn code_link_strike() {
        let d = doc(json!([p(json!([
            tm("x()", &["code"]),
            t(" "),
            { "type": "text", "text": "site", "marks": [{ "type": "link", "attrs": { "href": "https://a.io" } }] },
            t(" "),
            tm("old", &["strike"]),
        ]))]));
        assert_eq!(to_markdown(&d), "`x()` [site](https://a.io) ~~old~~");
    }

    #[test]
    fn nested_lists() {
        let d = doc(json!([{ "type": "bulletList", "content": [
            { "type": "listItem", "content": [p(json!([t("one")])), { "type": "orderedList", "attrs": {"order": 1}, "content": [
                { "type": "listItem", "content": [p(json!([t("a")]))] },
                { "type": "listItem", "content": [p(json!([t("b")]))] },
            ]}]},
            { "type": "listItem", "content": [p(json!([t("two")]))] },
        ]}]));
        assert_eq!(to_markdown(&d), "- one\n  1. a\n  2. b\n- two");
    }

    #[test]
    fn code_block_and_rule_and_quote() {
        let d = doc(json!([
            { "type": "codeBlock", "attrs": { "language": "rust" }, "content": [t("fn main() {}")] },
            { "type": "rule" },
            { "type": "blockquote", "content": [p(json!([t("quoted")]))] },
        ]));
        assert_eq!(
            to_markdown(&d),
            "```rust\nfn main() {}\n```\n\n---\n\n> quoted"
        );
    }

    #[test]
    fn panels_become_callouts() {
        let d = doc(
            json!([{ "type": "panel", "attrs": { "panelType": "warning" }, "content": [p(json!([t("Careful")]))] }]),
        );
        assert_eq!(to_markdown(&d), "> [!warning]\n> Careful");
    }

    #[test]
    fn mentions_emoji_dates_status() {
        let d = doc(json!([p(json!([
            { "type": "mention", "attrs": { "id": "557058:abc", "text": "@Ana Pérez" } },
            t(" "),
            { "type": "emoji", "attrs": { "shortName": ":smile:", "text": "😄" } },
            t(" "),
            { "type": "date", "attrs": { "timestamp": "1767225600000" } },
            t(" "),
            { "type": "status", "attrs": { "text": "BLOCKED" } },
        ]))]));
        assert_eq!(
            to_markdown(&d),
            "[@Ana Pérez](mention:557058:abc) 😄 2026-01-01 `BLOCKED`"
        );
    }

    #[test]
    fn tables() {
        let d = doc(json!([{ "type": "table", "content": [
            { "type": "tableRow", "content": [
                { "type": "tableHeader", "content": [p(json!([t("A")]))] },
                { "type": "tableHeader", "content": [p(json!([t("B")]))] },
            ]},
            { "type": "tableRow", "content": [
                { "type": "tableCell", "content": [p(json!([t("1")]))] },
                { "type": "tableCell", "content": [p(json!([t("x|y")]))] },
            ]},
        ]}]));
        assert_eq!(to_markdown(&d), "| A | B |\n| --- | --- |\n| 1 | x\\|y |");
    }

    #[test]
    fn tasks_and_media() {
        let d = doc(json!([
            { "type": "taskList", "attrs": {"localId": "x"}, "content": [
                { "type": "taskItem", "attrs": { "localId": "1", "state": "DONE" }, "content": [t("done")] },
                { "type": "taskItem", "attrs": { "localId": "2", "state": "TODO" }, "content": [t("todo")] },
            ]},
            { "type": "mediaSingle", "content": [{ "type": "media", "attrs": { "id": "abc", "type": "file", "alt": "shot.png" } }] },
        ]));
        assert_eq!(
            to_markdown(&d),
            "- [x] done\n- [ ] todo\n\n![shot.png](jira-attachment:shot.png)"
        );
    }

    #[test]
    fn hard_break() {
        let d = doc(json!([p(json!([t("a"), { "type": "hardBreak" }, t("b")]))]));
        assert_eq!(to_markdown(&d), "a\\\nb");
    }

    #[test]
    fn md_to_adf_basic() {
        let v = from_markdown("# Hi\n\nSome **bold** and *em* `code` [l](https://x.io)");
        assert_eq!(v["type"], "doc");
        assert_eq!(v["version"], 1);
        assert_eq!(v["content"][0]["type"], "heading");
        assert_eq!(v["content"][0]["attrs"]["level"], 1);
        let inl = v["content"][1]["content"].as_array().unwrap();
        assert_eq!(inl[0]["text"], "Some ");
        assert_eq!(inl[1]["marks"][0]["type"], "strong");
        assert_eq!(inl[3]["marks"][0]["type"], "em");
        assert_eq!(inl[5]["marks"][0]["type"], "code");
        assert_eq!(inl[7]["marks"][0]["attrs"]["href"], "https://x.io");
    }

    #[test]
    fn md_to_adf_lists_wrap_paragraphs() {
        let v = from_markdown("- a\n- b\n  1. c\n");
        let list = &v["content"][0];
        assert_eq!(list["type"], "bulletList");
        assert_eq!(list["content"][0]["type"], "listItem");
        assert_eq!(list["content"][0]["content"][0]["type"], "paragraph");
        assert_eq!(list["content"][1]["content"][1]["type"], "orderedList");
        assert!(list["content"][0].get("attrs").is_none());
    }

    #[test]
    fn md_to_adf_tasks_callouts_mentions_tables() {
        let v = from_markdown(
            "- [x] shipped\n- [ ] later\n\n> [!warning] Heads up\n> be careful\n\nhi [@Ana](mention:123)\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        );
        let c = v["content"].as_array().unwrap();
        assert_eq!(c[0]["type"], "taskList");
        assert_eq!(c[0]["content"][0]["attrs"]["state"], "DONE");
        assert_eq!(c[0]["content"][0]["content"][0]["text"], "shipped");
        assert_eq!(c[1]["type"], "panel");
        assert_eq!(c[1]["attrs"]["panelType"], "warning");
        assert_eq!(c[1]["content"][0]["content"][0]["text"], "Heads up");
        assert_eq!(c[2]["content"][1]["type"], "mention");
        assert_eq!(c[2]["content"][1]["attrs"]["id"], "123");
        assert_eq!(c[3]["type"], "table");
        assert_eq!(c[3]["content"][0]["content"][0]["type"], "tableHeader");
        assert_eq!(
            c[3]["content"][1]["content"][1]["content"][0]["content"][0]["text"],
            "2"
        );
        assert!(!v.to_string().contains("__task"));
    }

    #[test]
    fn md_to_adf_code_block() {
        let v = from_markdown("```js\nlet a = 1;\nlet b = 2;\n```\n");
        assert_eq!(v["content"][0]["type"], "codeBlock");
        assert_eq!(v["content"][0]["attrs"]["language"], "js");
        assert_eq!(
            v["content"][0]["content"][0]["text"],
            "let a = 1;\nlet b = 2;"
        );
    }

    #[test]
    fn roundtrip_is_stable() {
        let md = "## Plan\n\nShip **it** with `care`.\n\n- one\n  - nested\n- two\n\n1. first\n2. second\n\n> [!info]\n> note this\n\n- [ ] task\n\n```sh\nmake\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |";
        let back = to_markdown(&from_markdown(md));
        assert_eq!(back, md);
        assert_eq!(to_markdown(&from_markdown(&back)), md);
    }
}
