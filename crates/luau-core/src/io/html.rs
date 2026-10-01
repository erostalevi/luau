//! Markdown → safe HTML for exports (HTML file, PDF via the print dialog).
//!
//! - Raw HTML in cards is rendered as text (never as markup), so an exported
//!   file can't run scripts from imported or synced content.
//! - Only `http(s)`, `mailto`, in-page anchors and relative links survive;
//!   `javascript:` and other schemes become `#`.
//! - `[[card]]` links become the card title (an in-page anchor when the card
//!   is part of the export), `#tags` become chips, callouts (`> [!note]`)
//!   and the property footer get their own styling.

use std::sync::LazyLock;

use pulldown_cmark::{CowStr, Event, Parser, Tag, TextMergeStream};
use regex::Regex;

use super::esc_html;
use crate::markdown;

pub struct HtmlCtx<'a> {
    /// Card id → title (for `[[id]]` links).
    pub title_of: &'a dyn Fn(&str) -> Option<String>,
    /// Card id → in-page anchor (`#card-…`) when the card is in this export.
    pub anchor_of: &'a dyn Fn(&str) -> Option<String>,
    /// Local file reference → URL to embed (usually a `data:` URI), when available.
    pub resolve_src: &'a dyn Fn(&str) -> Option<String>,
}

static WIKI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!?)\[\[([^\[\]\n|#]+)(#[^\[\]\n|]*)?(?:\|([^\[\]\n]*))?\]\]").unwrap()
});
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[\s(\[,;])#([\p{L}\p{N}_/\-]+)").unwrap());
static CALLOUT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<blockquote>\s*<p>\[!([A-Za-z][\w-]*)\]([+-]?)\s*([^<\n]*)").unwrap()
});
static ALERT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<blockquote class="markdown-alert-(\w+)">"#).unwrap());

fn safe_href(url: &str) -> String {
    let u = url.trim();
    let lower = u.to_ascii_lowercase();
    let has_scheme = lower.split_once(':').is_some_and(|(s, _)| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && !s.contains('/')
    });
    if !has_scheme
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
    {
        u.to_string()
    } else {
        "#".to_string()
    }
}

/// Split plain text into escaped HTML with link and tag decorations.
fn decorate_text(text: &str, ctx: &HtmlCtx) -> String {
    let mut out = String::new();
    let mut last = 0;
    for c in WIKI_RE.captures_iter(text) {
        let m = c.get(0).unwrap();
        out.push_str(&decorate_tags(&text[last..m.start()]));
        let target = c[2].trim();
        let alias = c
            .get(4)
            .map(|a| a.as_str().trim())
            .filter(|a| !a.is_empty());
        let heading = c
            .get(3)
            .map(|h| h.as_str().trim_start_matches('#'))
            .filter(|h| !h.is_empty());
        let title = (ctx.title_of)(target).unwrap_or_else(|| target.to_string());
        let mut label = alias.map(str::to_string).unwrap_or(title);
        if let (Some(h), None) = (heading, alias) {
            label = format!("{label} › {h}");
        }
        match (ctx.anchor_of)(target) {
            Some(a) => out.push_str(&format!(
                "<a class=\"card-link\" href=\"{}\">{}</a>",
                esc_html(&a),
                esc_html(&label)
            )),
            None => out.push_str(&format!(
                "<span class=\"card-link\">{}</span>",
                esc_html(&label)
            )),
        }
        last = m.end();
    }
    out.push_str(&decorate_tags(&text[last..]));
    out
}

fn decorate_tags(text: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    for c in TAG_RE.captures_iter(text) {
        let m = c.get(0).unwrap();
        let pre = c.get(1).unwrap();
        out.push_str(&esc_html(&text[last..pre.end()]));
        out.push_str(&format!("<span class=\"tag\">#{}</span>", esc_html(&c[2])));
        last = m.end();
    }
    out.push_str(&esc_html(&text[last..]));
    out
}

/// Render a card body (title line and property footer handled separately).
pub fn render_body(md: &str, ctx: &HtmlCtx) -> String {
    let parsed = markdown::parse(md);
    let mut lines: Vec<&str> = md.lines().collect();
    if let Some(start) = parsed.footer.start_line
        && start <= lines.len()
    {
        lines.truncate(start);
    }
    if parsed.has_title_line && !lines.is_empty() {
        lines.remove(0);
    }
    let text = lines.join("\n");
    let mut events: Vec<Event> = Vec::new();
    let mut in_code = false;
    for ev in TextMergeStream::new(Parser::new_ext(&text, markdown::options())) {
        match ev {
            Event::Start(Tag::CodeBlock(k)) => {
                in_code = true;
                events.push(Event::Start(Tag::CodeBlock(k)));
            }
            Event::End(e @ pulldown_cmark::TagEnd::CodeBlock) => {
                in_code = false;
                events.push(Event::End(e));
            }
            Event::Html(h) | Event::InlineHtml(h) => events.push(Event::Text(h)),
            Event::Text(t) if !in_code && (t.contains("[[") || t.contains('#')) => {
                events.push(Event::InlineHtml(CowStr::from(decorate_text(&t, ctx))));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = (ctx.resolve_src)(&dest_url).unwrap_or_else(|| safe_href(&dest_url));
                events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::from(dest),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = (ctx.resolve_src)(&dest_url).unwrap_or_else(|| safe_href(&dest_url));
                events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: CowStr::from(dest),
                    title,
                    id,
                }));
            }
            other => events.push(other),
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    let html = ALERT_RE.replace_all(&html, |c: &regex::Captures| {
        format!(
            "<blockquote class=\"callout callout-{0}\"><div class=\"callout-title\">{1}</div>",
            &c[1],
            capitalize(&c[1])
        )
    });
    let html = CALLOUT_RE.replace_all(&html, |c: &regex::Captures| {
        let kind = c[1].to_ascii_lowercase();
        let title = c[3].trim();
        let title = if title.is_empty() { capitalize(&kind) } else { title.to_string() };
        format!(
            "<blockquote class=\"callout callout-{kind}\"><div class=\"callout-title\">{title}</div><p>"
        )
    });
    let mut out = html.into_owned();
    if !parsed.footer.fields.is_empty() {
        out.push_str("<table class=\"props\">");
        for (k, v) in &parsed.footer.fields {
            out.push_str(&format!(
                "<tr><th>{}</th><td>{}</td></tr>",
                esc_html(&capitalize(k)),
                decorate_text(v, ctx)
            ));
        }
        out.push_str("</table>");
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub const CSS: &str = r#"
:root{--bg:#fbfaf7;--surface:#ffffff;--text:#2b2d33;--muted:#6b7080;--line:#e7e4dc;--accent:#5b5bd6;--accent-soft:#eeeefc;--code:#f5f3ee;--tag:#eef3ff;--tag-text:#3d5bb5;
--note:#e8f1ff;--note-b:#8fb3f5;--tip:#e7f7ee;--tip-b:#7cc79b;--warn:#fff4e0;--warn-b:#f0b95a;--danger:#fdecec;--danger-b:#ec9a9a;--important:#f3ecff;--important-b:#b89af0}
@media (prefers-color-scheme:dark){:root{--bg:#18191c;--surface:#202226;--text:#e6e6e9;--muted:#9a9ea8;--line:#2f3238;--accent:#9d9dfa;--accent-soft:#2a2a44;--code:#26282d;--tag:#242b40;--tag-text:#a9bdf5;
--note:#1e2a3d;--note-b:#4d6fa8;--tip:#1c3027;--tip-b:#4d9a6d;--warn:#372d1c;--warn-b:#b98a3c;--danger:#3a2224;--danger-b:#b86a6a;--important:#2c2440;--important-b:#8b6fc4}}
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.6 Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif}
main{max-width:980px;margin:0 auto;padding:48px 24px 80px}
header.doc{margin-bottom:32px}
header.doc h1{font-size:30px;line-height:1.2;margin:0 0 6px;letter-spacing:-.01em}
header.doc .meta{color:var(--muted);font-size:13px}
nav.overview{display:grid;grid-template-columns:repeat(auto-fill,minmax(200px,1fr));gap:12px;margin:0 0 40px}
nav.overview .lane{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:12px 14px}
nav.overview .lane h3{margin:0 0 8px;font-size:13px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted)}
nav.overview ol{margin:0;padding:0;list-style:none}
nav.overview li{margin:4px 0;font-size:14px}
nav.overview a{color:var(--text);text-decoration:none}
nav.overview a:hover{color:var(--accent)}
section.lane-section>h2{font-size:20px;margin:44px 0 16px;padding-bottom:6px;border-bottom:1px solid var(--line)}
article.card{background:var(--surface);border:1px solid var(--line);border-radius:16px;padding:20px 24px;margin:0 0 16px;box-shadow:0 1px 2px rgba(0,0,0,.03)}
article.card article.card{margin:16px 0 0;border-radius:12px;box-shadow:none}
article.card>h2,article.card>h3,article.card>h4{margin:0 0 10px;line-height:1.3}
article.card.archived{opacity:.7}
.badge{display:inline-block;font-size:11px;color:var(--muted);border:1px solid var(--line);border-radius:999px;padding:0 8px;margin-left:8px;vertical-align:middle}
.cover{display:block;max-width:100%;border-radius:10px;margin:0 0 12px}
a{color:var(--accent)}
img{max-width:100%;height:auto;border-radius:8px}
h1,h2,h3,h4,h5,h6{line-height:1.3}
p,ul,ol,table,pre,blockquote{margin:0 0 12px}
code{font-family:"JetBrains Mono",ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.88em;background:var(--code);border-radius:6px;padding:.1em .35em}
pre{background:var(--code);border-radius:10px;padding:12px 14px;overflow:auto}
pre code{background:none;padding:0;font-size:13px;line-height:1.55}
table{border-collapse:collapse;width:100%;font-size:14px}
th,td{border:1px solid var(--line);padding:6px 10px;text-align:left;vertical-align:top}
thead th{background:var(--code)}
blockquote{border-left:3px solid var(--line);padding:4px 14px;color:var(--muted);margin-left:0}
blockquote.callout{color:var(--text);border-radius:10px;border-left-width:4px;padding:10px 14px;background:var(--note);border-color:var(--note-b)}
blockquote.callout>*:last-child{margin-bottom:0}
.callout-title{font-weight:600;margin-bottom:4px}
.callout-tip,.callout-success,.callout-check,.callout-done{background:var(--tip)!important;border-color:var(--tip-b)!important}
.callout-warning,.callout-caution,.callout-attention,.callout-question,.callout-help,.callout-faq{background:var(--warn)!important;border-color:var(--warn-b)!important}
.callout-danger,.callout-error,.callout-bug,.callout-failure,.callout-fail{background:var(--danger)!important;border-color:var(--danger-b)!important}
.callout-important,.callout-example,.callout-quote,.callout-abstract,.callout-summary{background:var(--important)!important;border-color:var(--important-b)!important}
.tag{display:inline-block;background:var(--tag);color:var(--tag-text);border-radius:999px;padding:0 8px;font-size:.85em;line-height:1.6}
.card-link{color:var(--accent);background:var(--accent-soft);border-radius:6px;padding:0 5px;text-decoration:none}
span.card-link{color:var(--muted);background:none}
ul.contains-task-list{list-style:none;padding-left:4px}
li.task-list-item input,li input[type=checkbox]{margin-right:8px}
table.props{width:auto;margin-top:12px;font-size:13px}
table.props th{background:none;color:var(--muted);font-weight:500;border:none;padding:2px 16px 2px 0}
table.props td{border:none;padding:2px 0}
hr{border:none;border-top:1px solid var(--line);margin:20px 0}
footer.doc{margin-top:48px;color:var(--muted);font-size:12px;text-align:center}
@media print{body{background:#fff;color:#000;font-size:11pt}main{max-width:none;padding:0}nav.overview{display:none}
article.card{box-shadow:none;break-inside:avoid-page;border-color:#ddd}section.lane-section>h2{break-after:avoid}a{color:inherit}}
@media (max-width:640px){main{padding:28px 16px 60px}article.card{padding:16px}}
"#;

/// Wrap rendered content into a complete, self-contained HTML document.
pub fn page(title: &str, meta: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"generator\" content=\"{app}\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data: https:; media-src data: https:; style-src 'unsafe-inline'\">\n<title>{t}</title>\n<style>{css}</style>\n</head>\n<body>\n<main>\n<header class=\"doc\"><h1>{t}</h1><div class=\"meta\">{m}</div></header>\n{body}\n<footer class=\"doc\">{app}</footer>\n</main>\n</body>\n</html>\n",
        app = crate::brand::APP_NAME,
        t = esc_html(title),
        m = esc_html(meta),
        css = CSS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_render(md: &str) -> String {
        let title_of = |id: &str| (id == "c111111").then(|| "Other <card>".to_string());
        let anchor_of = |id: &str| (id == "c111111").then(|| "#card-c111111".to_string());
        let resolve =
            |r: &str| (r == "c1.abcd.png").then(|| "data:image/png;base64,AAAA".to_string());
        render_body(
            md,
            &HtmlCtx {
                title_of: &title_of,
                anchor_of: &anchor_of,
                resolve_src: &resolve,
            },
        )
    }

    #[test]
    fn renders_tables_callouts_code_and_escapes_html() {
        let md = "# Title\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> [!warning] Careful\n> text\n\n```rust\nlet x = \"#no-tag\";\n```\n\n<script>alert(1)</script>\n\n[x](javascript:alert(1)) [y](https://example.com)\n";
        let h = ctx_render(md);
        assert!(!h.contains("<h1>"), "title line is rendered separately");
        assert!(h.contains("<table>") && h.contains("<td>1</td>"));
        assert!(
            h.contains("callout callout-warning") && h.contains("Careful"),
            "{h}"
        );
        assert!(h.contains("<code class=\"language-rust\">") && h.contains("#no-tag"));
        assert!(!h.contains("<span class=\"tag\">#no-tag"));
        assert!(!h.contains("<script>") && h.contains("&lt;script&gt;"));
        assert!(h.contains("href=\"#\"") && h.contains("href=\"https://example.com\""));
    }

    #[test]
    fn decorates_links_tags_images_and_footer() {
        let md = "# T\n\nSee [[c111111]] and [[c999999|alias]] #todo\n\n![img](c1.abcd.png)\n\n---\npriority: high\n";
        let h = ctx_render(md);
        assert!(
            h.contains("<a class=\"card-link\" href=\"#card-c111111\">Other &lt;card&gt;</a>"),
            "{h}"
        );
        assert!(h.contains("<span class=\"card-link\">alias</span>"));
        assert!(h.contains("<span class=\"tag\">#todo</span>"));
        assert!(h.contains("src=\"data:image/png;base64,AAAA\""));
        assert!(
            h.contains("<table class=\"props\"><tr><th>Priority</th><td>high</td></tr></table>")
        );
    }

    #[test]
    fn github_alerts_get_titles() {
        let h = ctx_render("> [!NOTE]\n> hello\n");
        assert!(
            h.contains("callout callout-note") && h.contains("callout-title\">Note<"),
            "{h}"
        );
    }

    #[test]
    fn page_is_self_contained() {
        let p = page("A & B", "meta", "<p>x</p>");
        assert!(p.starts_with("<!doctype html>") && p.contains("<title>A &amp; B</title>"));
        assert!(p.contains("Content-Security-Policy") && !p.contains("<script"));
    }
}
