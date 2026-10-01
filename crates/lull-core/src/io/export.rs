//! Exporters: a board (or one card subtree) → single Markdown file, Markdown
//! bundle (`.zip` of readable folders, re-importable with the loose adapter),
//! self-contained HTML (also used for PDF through the print dialog) and
//! interchange JSON. The board `.zip` lives in [`super::archive`].

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use base64::Engine;
use regex::Regex;

use super::archive::ZipOut;
use super::html::{self, HtmlCtx};
use super::{esc_html, file_stem_from_title};
use crate::error::Result;
use crate::model::*;

/// Images above this size are linked by name instead of embedded in HTML.
pub const MAX_EMBED: u64 = 12 * 1024 * 1024;

static WIKI_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!?)\[\[([^\[\]\n|#]+)(#[^\[\]\n|]*)?(?:\|([^\[\]\n]*))?\]\]").unwrap()
});

/// Replace `[[id…]]` links by a label via `f(id, heading, alias)`.
pub fn replace_links(body: &str, f: &dyn Fn(&str, &str, Option<&str>) -> String) -> String {
    WIKI_RE
        .replace_all(body, |c: &regex::Captures| {
            if &c[1] == "!" {
                return c[0].to_string();
            }
            f(
                c[2].trim(),
                c.get(3).map(|m| m.as_str()).unwrap_or(""),
                c.get(4)
                    .map(|m| m.as_str().trim())
                    .filter(|a| !a.is_empty()),
            )
        })
        .into_owned()
}

/// Shift Markdown headings (outside code fences) by `by` levels (max `######`).
pub fn shift_headings(md: &str, by: usize) -> String {
    let mut out = String::with_capacity(md.len() + 16);
    let mut fence: Option<&str> = None;
    for line in md.split_inclusive('\n') {
        let t = line.trim_start();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(&t[..3]);
            out.push_str(line);
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            let n = (hashes + by).min(6);
            out.push_str(&"#".repeat(n));
            out.push_str(&t[hashes..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn title_of(st: &BoardState, id: &str) -> Option<String> {
    st.nodes.get(id).map(|n| {
        if n.meta.title.trim().is_empty() {
            "Untitled".into()
        } else {
            n.meta.title.clone()
        }
    })
}

fn links_to_titles(st: &BoardState, body: &str) -> String {
    replace_links(body, &|id, heading, alias| {
        let t = alias
            .map(str::to_string)
            .or_else(|| title_of(st, id))
            .unwrap_or_else(|| id.to_string());
        let h = heading.trim_start_matches('#');
        if h.is_empty() || alias.is_some() {
            t
        } else {
            format!("{t} › {h}")
        }
    })
}

/// Single Markdown document (links replaced with titles).
pub fn markdown_doc(st: &BoardState, read: &dyn Fn(&str) -> String, only: Option<&str>) -> String {
    fn card(
        st: &BoardState,
        read: &dyn Fn(&str) -> String,
        id: &str,
        depth: usize,
        out: &mut String,
    ) {
        let Some(n) = st.nodes.get(id) else { return };
        let mut body = links_to_titles(st, &read(id));
        if !n.meta.has_title_line {
            body = format!("# {}\n\n{body}", title_of(st, id).unwrap_or_default());
        }
        out.push_str(shift_headings(&body, depth).trim_end());
        out.push_str("\n\n");
        for c in &n.children {
            card(st, read, c, depth + 1, out);
        }
    }
    let mut out = String::new();
    match only {
        Some(id) => card(st, read, id, 0, &mut out),
        None => {
            out.push_str(&format!("# {}\n\n", st.manifest.name));
            for l in st.lanes.iter().filter(|l| !l.archived) {
                out.push_str(&format!("## {}\n\n", l.name));
                for id in &l.order {
                    card(st, read, id, 2, &mut out);
                }
            }
            for id in &st.root_order {
                card(st, read, id, 1, &mut out);
            }
        }
    }
    format!("{}\n", out.trim_end())
}

/// Markdown bundle: one folder per lane, `<Title>.md` per card, groups as
/// `<Title>/index.md`, attachments next to their card, `[[Title]]` links.
pub fn markdown_bundle(
    st: &BoardState,
    read: &dyn Fn(&str) -> String,
    dest: &Path,
) -> Result<(usize, u64)> {
    let top = file_stem_from_title(&st.manifest.name, 80);
    let mut z = ZipOut::create(dest)?;
    // Unique readable names per folder.
    let mut used: HashMap<String, HashSet<String>> = HashMap::new();
    let mut unique = |dir: &str, stem: &str| -> String {
        let set = used.entry(dir.to_string()).or_default();
        let mut name = stem.to_string();
        let mut i = 2;
        while !set.insert(name.to_lowercase()) {
            name = format!("{stem} {i}");
            i += 1;
        }
        name
    };
    // Link label: the exported card's (unique) name.
    let mut names: HashMap<String, String> = HashMap::new();
    let mut plan: Vec<(String, String, bool)> = Vec::new(); // (id, dir, is_group)
    fn collect(
        st: &BoardState,
        ids: &[String],
        dir: &str,
        unique: &mut dyn FnMut(&str, &str) -> String,
        names: &mut HashMap<String, String>,
        plan: &mut Vec<(String, String, bool)>,
    ) {
        for id in ids {
            let Some(n) = st.nodes.get(id) else { continue };
            let stem = unique(
                dir,
                &file_stem_from_title(&title_of(st, id).unwrap_or_default(), 80),
            );
            names.insert(id.clone(), stem.clone());
            let is_group = !n.children.is_empty();
            plan.push((id.clone(), dir.to_string(), is_group));
            if is_group {
                collect(
                    st,
                    &n.children,
                    &format!("{dir}{stem}/"),
                    unique,
                    names,
                    plan,
                );
            }
        }
    }
    for l in &st.lanes {
        let lane_dir = format!(
            "{top}/{}/",
            unique(&format!("{top}/"), &file_stem_from_title(&l.name, 80))
        );
        collect(st, &l.order, &lane_dir, &mut unique, &mut names, &mut plan);
    }
    collect(
        st,
        &st.root_order,
        &format!("{top}/"),
        &mut unique,
        &mut names,
        &mut plan,
    );
    for (id, dir, is_group) in &plan {
        let n = &st.nodes[id];
        let body = replace_links(
            &read(id),
            &|target, heading, alias| match names.get(target) {
                Some(name) => format!(
                    "[[{name}{heading}{}]]",
                    alias.map(|a| format!("|{a}")).unwrap_or_default()
                ),
                None => alias.unwrap_or(target).to_string(),
            },
        );
        let name = &names[id];
        let (file, att_dir) = if *is_group {
            (format!("{dir}{name}/index.md"), format!("{dir}{name}/"))
        } else {
            (format!("{dir}{name}.md"), dir.clone())
        };
        z.add_bytes(&file, body.as_bytes())?;
        if let Some(src_dir) = st.attachment_dir(id) {
            for a in &n.attachments {
                z.add_file(&format!("{att_dir}{}", a.file), &src_dir.join(&a.file))?;
            }
        }
    }
    let r = (z.files, z.bytes);
    z.finish()?;
    Ok(r)
}

fn mime_of(file: &str) -> Option<&'static str> {
    let ext = file.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => return None, // svg is never embedded (could carry script)
    })
}

/// Self-contained HTML document of a board or card subtree.
pub fn html_doc(st: &BoardState, read: &dyn Fn(&str) -> String, only: Option<&str>) -> String {
    let in_export: HashSet<String> = match only {
        Some(id) => {
            let mut s = HashSet::new();
            let mut stack = vec![id.to_string()];
            while let Some(x) = stack.pop() {
                if let Some(n) = st.nodes.get(&x) {
                    stack.extend(n.children.iter().cloned());
                }
                s.insert(x);
            }
            s
        }
        None => st.nodes.keys().cloned().collect(),
    };
    let data_uri = |id: &str, file: &str| -> Option<String> {
        let mime = mime_of(file)?;
        let dir = st.attachment_dir(id)?;
        let p = crate::fsutil::safe_join(&dir, file).ok()?;
        let meta = fs::symlink_metadata(&p).ok()?;
        if !meta.is_file() || meta.len() > MAX_EMBED {
            return None;
        }
        let bytes = fs::read(&p).ok()?;
        Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    };
    fn card(
        st: &BoardState,
        id: &str,
        depth: usize,
        ctx_for: &dyn Fn(&str) -> String,
        cover_for: &dyn Fn(&str) -> Option<String>,
        out: &mut String,
    ) {
        let Some(n) = st.nodes.get(id) else { return };
        let h = (depth + 2).min(4);
        out.push_str(&format!(
            "<article class=\"card{}\" id=\"card-{}\">",
            if n.archived { " archived" } else { "" },
            esc_html(id)
        ));
        if let Some(src) = cover_for(id) {
            out.push_str(&format!(
                "<img class=\"cover\" src=\"{}\" alt=\"\">",
                esc_html(&src)
            ));
        }
        out.push_str(&format!(
            "<h{h}>{}{}</h{h}>",
            esc_html(&title_of(st, id).unwrap_or_default()),
            if n.archived {
                "<span class=\"badge\">archived</span>"
            } else {
                ""
            }
        ));
        out.push_str(&ctx_for(id));
        for c in &n.children {
            card(st, c, depth + 1, ctx_for, cover_for, out);
        }
        out.push_str("</article>\n");
    }
    let render = |id: &str| -> String {
        let t_of = |x: &str| title_of(st, x);
        let a_of = |x: &str| in_export.contains(x).then(|| format!("#card-{x}"));
        let r_src = |r: &str| {
            let r = percent_encoding::percent_decode_str(r)
                .decode_utf8_lossy()
                .into_owned();
            st.nodes.get(id)?.attachments.iter().find(|a| a.file == r)?;
            data_uri(id, &r)
        };
        html::render_body(
            &read(id),
            &HtmlCtx {
                title_of: &t_of,
                anchor_of: &a_of,
                resolve_src: &r_src,
            },
        )
    };
    let cover = |id: &str| -> Option<String> {
        let c = st.nodes.get(id)?.cover.as_ref()?;
        data_uri(id, &c.file)
    };
    let mut body = String::new();
    let title;
    let meta;
    match only {
        Some(id) => {
            title = title_of(st, id).unwrap_or_default();
            meta = st.manifest.name.clone();
            if let Some(n) = st.nodes.get(id) {
                body.push_str(&render(id));
                for c in &n.children {
                    card(st, c, 0, &render, &cover, &mut body);
                }
            }
        }
        None => {
            title = st.manifest.name.clone();
            let count = st.nodes.len();
            meta = format!(
                "{count} {} · {}",
                if count == 1 { "card" } else { "cards" },
                chrono::Local::now().format("%Y-%m-%d")
            );
            let lanes: Vec<&Lane> = st.lanes.iter().filter(|l| !l.archived).collect();
            if lanes.len() > 1 {
                body.push_str("<nav class=\"overview\">");
                for l in &lanes {
                    body.push_str(&format!(
                        "<div class=\"lane\"><h3>{}</h3><ol>",
                        esc_html(&l.name)
                    ));
                    for id in &l.order {
                        body.push_str(&format!(
                            "<li><a href=\"#card-{}\">{}</a></li>",
                            esc_html(id),
                            esc_html(&title_of(st, id).unwrap_or_default())
                        ));
                    }
                    body.push_str("</ol></div>");
                }
                body.push_str("</nav>\n");
            }
            for l in lanes {
                body.push_str(&format!(
                    "<section class=\"lane-section\"><h2>{}</h2>\n",
                    esc_html(&l.name)
                ));
                for id in &l.order {
                    card(st, id, 0, &render, &cover, &mut body);
                }
                body.push_str("</section>\n");
            }
            for id in &st.root_order {
                card(st, id, 0, &render, &cover, &mut body);
            }
        }
    }
    html::page(&title, &meta, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_headings_outside_code() {
        let s = shift_headings("# A\n```\n# not\n```\n## B\n", 2);
        assert_eq!(s, "### A\n```\n# not\n```\n#### B\n");
        assert_eq!(shift_headings("##### x\n", 3), "###### x\n");
    }

    #[test]
    fn replaces_links_but_not_embeds() {
        let out = replace_links("a [[c1]] b [[c2#H|Alias]] ![[c3]]", &|id, h, a| {
            format!("<{id}{h}{}>", a.unwrap_or(""))
        });
        assert_eq!(out, "a <c1> b <c2#HAlias> ![[c3]]");
    }
}
