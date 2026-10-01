//! Jira wiki markup (Server / Data Center descriptions and comments) ↔ Markdown.
//!
//! Uses the same Markdown conventions as [`super::adf`]: callouts for panels,
//! `[@Name](mention:user)` for mentions, `jira-attachment:` for embedded files.

use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use regex::Regex;

use super::adf::{ATTACHMENT_SCHEME, MENTION_SCHEME};
use super::md_util::{escape_inline, normalize_md, table_md};

// ---------------------------------------------------------------------------
// Wiki → Markdown
// ---------------------------------------------------------------------------

static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^h([1-6])\.\s+(.*)$").unwrap());
static LIST_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([*#-]+)\s+(.*)$").unwrap());
static BLOCK_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\{(code|noformat|quote|panel|info|note|warning|tip)(?::([^}]*))?\}(.*)$").unwrap()
});

/// Convert Jira wiki markup to Markdown.
pub fn to_markdown(wiki: &str) -> String {
    let text = wiki.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut Vec<String>| {
        if !para.is_empty() {
            out.push(para.join("\n"));
            out.push(String::new());
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        if let Some(c) = BLOCK_OPEN_RE.captures(t) {
            flush(&mut para, &mut out);
            let kind = c[1].to_string();
            let params = c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let close = format!("{{{kind}}}");
            // Body: rest of the opening line, then lines until the closing tag.
            let mut body: Vec<String> = Vec::new();
            let first_rest = c.get(3).map(|m| m.as_str()).unwrap_or("");
            let mut closed = false;
            if let Some(pos) = first_rest.find(&close) {
                body.push(first_rest[..pos].to_string());
                closed = true;
            } else if !first_rest.is_empty() {
                body.push(first_rest.to_string());
            }
            i += 1;
            while !closed && i < lines.len() {
                let l = lines[i];
                if let Some(pos) = l.find(&close) {
                    let before = &l[..pos];
                    if !before.is_empty() {
                        body.push(before.to_string());
                    }
                    closed = true;
                } else {
                    body.push(l.to_string());
                }
                i += 1;
            }
            let inner = body.join("\n");
            match kind.as_str() {
                "code" | "noformat" => {
                    let lang = if kind == "code" {
                        params
                            .split('|')
                            .find(|p| !p.contains('='))
                            .unwrap_or("")
                            .trim()
                            .to_string()
                    } else {
                        String::new()
                    };
                    let inner = inner.trim_matches('\n');
                    let fence = if inner.contains("```") { "~~~~" } else { "```" };
                    out.push(format!("{fence}{lang}\n{inner}\n{fence}"));
                }
                "quote" => out.push(quote(&to_markdown(&inner))),
                _ => {
                    let callout = match kind.as_str() {
                        "info" => "info",
                        "note" => "note",
                        "warning" => "warning",
                        "tip" => "success",
                        _ => "note",
                    };
                    let title = params
                        .split('|')
                        .find_map(|p| p.trim().strip_prefix("title="))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let head = if title.is_empty() {
                        format!("> [!{callout}]")
                    } else {
                        format!("> [!{callout}] {title}")
                    };
                    let body = to_markdown(&inner);
                    out.push(if body.is_empty() {
                        head
                    } else {
                        format!("{head}\n{}", quote(&body))
                    });
                }
            }
            out.push(String::new());
            continue;
        }
        if let Some(c) = HEADING_RE.captures(t) {
            flush(&mut para, &mut out);
            let level: usize = c[1].parse().unwrap_or(1);
            out.push(format!("{} {}", "#".repeat(level), inline(&c[2])));
            out.push(String::new());
            i += 1;
            continue;
        }
        if t == "----" {
            flush(&mut para, &mut out);
            out.push("---".into());
            out.push(String::new());
            i += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("bq. ") {
            flush(&mut para, &mut out);
            out.push(format!("> {}", inline(rest)));
            out.push(String::new());
            i += 1;
            continue;
        }
        if t.starts_with('|') {
            flush(&mut para, &mut out);
            let mut rows: Vec<Vec<String>> = Vec::new();
            let mut header = false;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let row = lines[i].trim();
                if rows.is_empty() && row.starts_with("||") {
                    header = true;
                }
                rows.push(
                    split_row(row)
                        .into_iter()
                        .map(|c| inline(c.trim()))
                        .collect(),
                );
                i += 1;
            }
            out.push(table_md(&rows, header));
            out.push(String::new());
            continue;
        }
        if let Some(c) = LIST_RE.captures(t)
            && !(c[1].starts_with('-') && c[1].len() > 1)
        {
            flush(&mut para, &mut out);
            let mut items = Vec::new();
            while i < lines.len() {
                let l = lines[i].trim();
                let Some(c) = LIST_RE.captures(l) else { break };
                let marks = &c[1];
                let depth = marks.len() - 1;
                let ordered = marks.ends_with('#');
                let body = inline_task(&c[2]);
                let bullet = if ordered { "1." } else { "-" };
                items.push(format!("{}{bullet} {body}", "  ".repeat(depth)));
                i += 1;
            }
            out.push(renumber(&items.join("\n")));
            out.push(String::new());
            continue;
        }
        para.push(inline(t));
        i += 1;
    }
    flush(&mut para, &mut out);
    normalize_md(&out.join("\n"))
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

/// Ordered list items rendered as `1.` get sequential numbers per level.
fn renumber(list: &str) -> String {
    let mut counters: Vec<usize> = Vec::new();
    list.lines()
        .map(|l| {
            let indent = l.len() - l.trim_start().len();
            let depth = indent / 2;
            counters.truncate(depth + 1);
            while counters.len() <= depth {
                counters.push(0);
            }
            let t = l.trim_start();
            if let Some(rest) = t.strip_prefix("1. ") {
                counters[depth] += 1;
                format!("{}{}. {rest}", " ".repeat(indent), counters[depth])
            } else {
                counters[depth] = 0;
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inline_task(s: &str) -> String {
    if let Some(r) = s.strip_prefix("(/) ") {
        return format!("[x] {}", inline(r));
    }
    if let Some(r) = s.strip_prefix("( ) ") {
        return format!("[ ] {}", inline(r));
    }
    inline(s)
}

fn split_row(row: &str) -> Vec<&str> {
    let r = row.trim_start_matches('|').trim_end_matches('|');
    r.split('|').filter(|c| !c.is_empty()).collect()
}

static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\[\]|]*?)(?:\|([^\[\]]*?))?\]").unwrap());
static IMG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!([^!\s|][^!|]*?)(?:\|[^!]*)?!").unwrap());
static MONO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(.+?)\}\}").unwrap());
static STRONG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^\w*])\*(\S(?:[^*]*?\S)?)\*($|[^\w*])").unwrap());
static EM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^\w_])_(\S(?:[^_]*?\S)?)_($|[^\w_])").unwrap());
static STRIKE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[\s(])-(\S(?:[^-]*?\S)?)-($|[\s).,;:!?])").unwrap());
static EMOTICON: &[(&str, &str)] = &[
    ("(/)", "✅"),
    ("(x)", "❌"),
    ("(!)", "⚠️"),
    ("(i)", "ℹ️"),
    ("(?)", "❓"),
    ("(y)", "👍"),
    ("(n)", "👎"),
    (":)", "🙂"),
    (":(", "🙁"),
];

static ESC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\([\[\]{}|*_!\-+^~?#()])").unwrap());
static PH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\u{0}(\\d+)\u{0}|\u{1}(\\d+)\u{1}|\u{2}").unwrap());

/// Inline wiki → Markdown. Code spans and escapes are protected from other rules.
fn inline(s: &str) -> String {
    let s = s.replace("\\\\", "\u{2}");
    let mut escaped: Vec<String> = Vec::new();
    let s = ESC_RE.replace_all(&s, |c: &regex::Captures| {
        escaped.push(c[1].to_string());
        format!("\u{1}{}\u{1}", escaped.len() - 1)
    });
    // Already-rendered Markdown fragments, frozen until the end.
    let mut frozen: Vec<String> = Vec::new();
    let mut freeze = |s: String| {
        frozen.push(s);
        format!("\u{0}{}\u{0}", frozen.len() - 1)
    };
    let s = MONO_RE.replace_all(&s, |c: &regex::Captures| {
        let code = &c[1];
        let tick = if code.contains('`') { "``" } else { "`" };
        freeze(format!("{tick}{code}{tick}"))
    });
    // Images before links: `!file.png|thumbnail!`.
    let s = IMG_RE.replace_all(&s, |c: &regex::Captures| {
        let target = c[1].trim();
        freeze(
            if target.starts_with("http://") || target.starts_with("https://") {
                format!("![]({target})")
            } else {
                format!(
                    "![{}]({ATTACHMENT_SCHEME}{})",
                    target,
                    target.replace(' ', "%20")
                )
            },
        )
    });
    let s = LINK_RE.replace_all(&s, |c: &regex::Captures| {
        let a = c[1].trim();
        let md = match c.get(2).map(|m| m.as_str().trim()) {
            Some(url) if url.starts_with('~') => format!("[@{a}]({MENTION_SCHEME}{})", &url[1..]),
            Some(url) => format!("[{a}]({url})"),
            None if a.starts_with('~') => format!("[@{}]({MENTION_SCHEME}{})", &a[1..], &a[1..]),
            None if a.starts_with("http://")
                || a.starts_with("https://")
                || a.starts_with("mailto:") =>
            {
                format!("<{a}>")
            }
            None if a.starts_with('^') => format!(
                "[{}]({ATTACHMENT_SCHEME}{})",
                &a[1..],
                a[1..].replace(' ', "%20")
            ),
            None => format!("\\[{a}\\]"),
        };
        freeze(md)
    });
    let s = STRONG_RE.replace_all(&s, "$1**$2**$3");
    let s = EM_RE.replace_all(&s, "$1*$2*$3");
    let s = STRIKE_RE.replace_all(&s, "$1~~$2~~$3");
    let mut s = s.into_owned();
    for (k, v) in EMOTICON {
        s = s.replace(k, v);
    }
    PH_RE
        .replace_all(&s, |c: &regex::Captures| {
            if let Some(i) = c.get(1).and_then(|m| m.as_str().parse::<usize>().ok()) {
                frozen[i].clone()
            } else if let Some(i) = c.get(2).and_then(|m| m.as_str().parse::<usize>().ok()) {
                escape_inline(&escaped[i])
            } else {
                "\\\n".to_string()
            }
        })
        .into_owned()
}

// ---------------------------------------------------------------------------
// Markdown → Wiki
// ---------------------------------------------------------------------------

#[derive(Default)]
struct W {
    out: String,
    /// List stack: ordered?
    lists: Vec<bool>,
    link: Vec<String>,
    /// Collecting a mention's (or image's) text instead of writing it.
    mention: Option<(String, String)>,
    in_code: bool,
    code_close: &'static str,
    cell_sep: &'static str,
    /// Open blockquotes: (output offset of the body, alert kind).
    quotes: Vec<(usize, Option<&'static str>)>,
    /// Set right after a block opener (`{quote}`…) so the first paragraph
    /// does not get a blank line.
    opened: bool,
}

impl W {
    fn nl(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }
    fn blank(&mut self) {
        self.nl();
        if !self.opened && !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }
    fn push(&mut self, s: &str) {
        if let Some((_, text)) = &mut self.mention {
            text.push_str(s);
            return;
        }
        self.opened = false;
        self.out.push_str(s);
    }
    fn open(&mut self, s: &str) {
        self.push(s);
        self.opened = true;
    }
}

fn escape_wiki(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '[' | ']' | '{' | '}' | '|' | '*' | '_' | '!') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

static CALLOUT_WIKI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\\\[\\!([A-Za-z-]+)\\\][ \t]*([^\n]*)\n?").unwrap());

fn wiki_panel(kind: &str) -> &'static str {
    match kind {
        "warning" | "caution" | "error" | "danger" | "attention" | "bug" => "warning",
        "success" | "tip" | "check" | "done" => "tip",
        "note" => "note",
        _ => "info",
    }
}

/// Convert Markdown to Jira wiki markup.
pub fn from_markdown(md: &str) -> String {
    let mut w = W {
        cell_sep: "|",
        code_close: "{code}",
        ..Default::default()
    };
    for ev in Parser::new_ext(md, crate::markdown::options()) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if w.lists.is_empty() {
                        w.blank();
                    }
                }
                Tag::Heading { level, .. } => {
                    w.blank();
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    w.push(&format!("h{n}. "));
                }
                Tag::BlockQuote(kind) => {
                    w.blank();
                    let k = kind.map(|k| match k {
                        pulldown_cmark::BlockQuoteKind::Note => "note",
                        pulldown_cmark::BlockQuoteKind::Tip => "tip",
                        pulldown_cmark::BlockQuoteKind::Important => "info",
                        pulldown_cmark::BlockQuoteKind::Warning
                        | pulldown_cmark::BlockQuoteKind::Caution => "warning",
                    });
                    match k {
                        Some(k) => w.open(&format!("{{{k}}}\n")),
                        None => w.open("{quote}\n"),
                    }
                    w.quotes.push((w.out.len(), k));
                }
                Tag::CodeBlock(kind) => {
                    w.blank();
                    match kind {
                        CodeBlockKind::Fenced(l) if !l.trim().is_empty() => {
                            let lang = l.split_whitespace().next().unwrap_or("");
                            w.push(&format!("{{code:{lang}}}\n"));
                            w.code_close = "{code}";
                        }
                        _ => {
                            w.push("{noformat}\n");
                            w.code_close = "{noformat}";
                        }
                    }
                    w.in_code = true;
                }
                Tag::List(start) => {
                    if w.lists.is_empty() {
                        w.blank();
                    }
                    w.lists.push(start.is_some());
                }
                Tag::Item => {
                    w.nl();
                    let marks: String =
                        w.lists.iter().map(|o| if *o { '#' } else { '*' }).collect();
                    w.push(&format!("{marks} "));
                }
                Tag::Table(_) => w.blank(),
                Tag::TableHead => w.cell_sep = "||",
                Tag::TableRow => {
                    w.nl();
                    w.cell_sep = "|";
                }
                Tag::TableCell => {
                    let sep = w.cell_sep;
                    w.push(sep);
                }
                Tag::Emphasis => w.push("_"),
                Tag::Strong => w.push("*"),
                Tag::Strikethrough => w.push("-"),
                Tag::Link { dest_url, .. } => {
                    if let Some(id) = dest_url.strip_prefix(MENTION_SCHEME) {
                        w.mention = Some((id.to_string(), String::new()));
                    } else {
                        w.push("[");
                        w.link.push(dest_url.to_string());
                    }
                }
                Tag::Image { dest_url, .. } => {
                    let target = dest_url
                        .strip_prefix(ATTACHMENT_SCHEME)
                        .map(|s| s.replace("%20", " "))
                        .unwrap_or_else(|| dest_url.to_string());
                    w.push(&format!("!{target}!"));
                    // Swallow the alt text.
                    w.mention = Some((String::new(), String::new()));
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if w.lists.is_empty() {
                        w.nl();
                    }
                }
                TagEnd::Heading(_) => w.nl(),
                TagEnd::BlockQuote(_) => {
                    w.nl();
                    let (at, kind) = w.quotes.pop().unwrap_or((w.out.len(), None));
                    match kind {
                        Some(k) => w.push(&format!("{{{k}}}\n")),
                        None => {
                            // Obsidian-style `> [!kind] Title` → wiki panel macro.
                            let body = w.out[at..].to_string();
                            if let Some(c) = CALLOUT_WIKI_RE.captures(&body) {
                                let k = wiki_panel(&c[1].to_ascii_lowercase());
                                let title = c[2].trim().to_string();
                                let rest =
                                    body[c.get(0).map(|m| m.end()).unwrap_or(0)..].to_string();
                                let open_at = at - "{quote}\n".len();
                                w.out.truncate(open_at);
                                let head = if title.is_empty() {
                                    format!("{{{k}}}\n")
                                } else {
                                    format!("{{{k}:title={title}}}\n")
                                };
                                w.push(&head);
                                w.push(&rest);
                                w.nl();
                                w.push(&format!("{{{k}}}\n"));
                            } else {
                                w.push("{quote}\n");
                            }
                        }
                    }
                }
                TagEnd::CodeBlock => {
                    w.in_code = false;
                    w.nl();
                    let close = w.code_close;
                    w.push(&format!("{close}\n"));
                }
                TagEnd::List(_) => {
                    w.lists.pop();
                    if w.lists.is_empty() {
                        w.nl();
                    }
                }
                TagEnd::TableHead | TagEnd::TableRow => {
                    let sep = w.cell_sep;
                    w.push(sep);
                }
                TagEnd::Table => w.nl(),
                TagEnd::Emphasis => w.push("_"),
                TagEnd::Strong => w.push("*"),
                TagEnd::Strikethrough => w.push("-"),
                TagEnd::Link => {
                    if let Some((id, _)) = w.mention.take() {
                        w.push(&format!("[~{id}]"));
                    } else if let Some(url) = w.link.pop() {
                        w.push(&format!("|{url}]"));
                    }
                }
                TagEnd::Image => w.mention = None,
                _ => {}
            },
            Event::Text(t) => {
                if w.in_code {
                    w.push(&t);
                } else {
                    w.push(&escape_wiki(&t));
                }
            }
            Event::Code(t) => w.push(&format!("{{{{{t}}}}}")),
            // Jira renders a newline inside a paragraph as a line break, which is
            // what a Markdown source newline looks like to most writers.
            Event::SoftBreak => w.push("\n"),
            Event::HardBreak => w.push("\\\\\n"),
            Event::Rule => {
                w.blank();
                w.push("----\n");
            }
            Event::TaskListMarker(done) => w.push(if done { "(/) " } else { "( ) " }),
            Event::Html(t) | Event::InlineHtml(t) => w.push(&t),
            Event::FootnoteReference(t) => w.push(&format!("[^{t}]")),
            Event::InlineMath(t) | Event::DisplayMath(t) => w.push(&t),
        }
    }
    let out = w.out.trim_matches('\n').to_string();
    let mut res = String::with_capacity(out.len());
    let mut blank = 0;
    for l in out.split('\n') {
        if l.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        if !res.is_empty() {
            res.push('\n');
        }
        res.push_str(l.trim_end());
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_to_md_basics() {
        let w = "h2. Plan\n\nShip *it* with {{care}} and _style_ -old-.\n\n* one\n** nested\n* two\n\n# first\n# second";
        assert_eq!(
            to_markdown(w),
            "## Plan\n\nShip **it** with `care` and *style* ~~old~~.\n\n- one\n  - nested\n- two\n\n1. first\n2. second"
        );
    }

    #[test]
    fn wiki_links_images_mentions() {
        let w = "See [docs|https://a.io] and [~ana] !shot.png|thumbnail! [https://b.io]";
        assert_eq!(
            to_markdown(w),
            "See [docs](https://a.io) and [@ana](mention:ana) ![shot.png](jira-attachment:shot.png) <https://b.io>"
        );
    }

    #[test]
    fn wiki_blocks() {
        let w = "{code:rust}\nfn a() {}\n{code}\n\n{quote}\nquoted\n{quote}\n\n{warning:title=Careful}\nhot\n{warning}\n\n||A||B||\n|1|2|\n\n----";
        assert_eq!(
            to_markdown(w),
            "```rust\nfn a() {}\n```\n\n> quoted\n\n> [!warning] Careful\n> hot\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n---"
        );
    }

    #[test]
    fn md_to_wiki_basics() {
        let md = "## Plan\n\nShip **it** with `care` and *style* ~~old~~.\n\n- one\n  - nested\n- two\n\n1. first\n2. second";
        assert_eq!(
            from_markdown(md),
            "h2. Plan\n\nShip *it* with {{care}} and _style_ -old-.\n\n* one\n** nested\n* two\n\n# first\n# second"
        );
    }

    #[test]
    fn md_to_wiki_blocks() {
        let md = "```js\nlet a;\n```\n\n> quoted\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n[docs](https://a.io) [@Ana](mention:ana) ![x](jira-attachment:shot.png)\n\n- [x] done";
        assert_eq!(
            from_markdown(md),
            "{code:js}\nlet a;\n{code}\n\n{quote}\nquoted\n{quote}\n\n||A||B||\n|1|2|\n\n[docs|https://a.io] [~ana] !shot.png!\n\n* (/) done"
        );
    }

    #[test]
    fn roundtrip_wiki() {
        let md = "## Plan\n\nShip **it** with `care`.\n\n- one\n  - nested\n- two\n\n1. first\n2. second\n\n> [!warning] Careful\n> hot\n\n- [ ] task\n\n```sh\nmake\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |";
        let wiki = from_markdown(md);
        assert_eq!(to_markdown(&wiki), md);
    }
}
