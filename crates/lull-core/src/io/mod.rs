//! Import / export, templates, board upgrades, loose (as-is) folders.
//!
//! Every importer or exporter is a *Source ↔ Interchange* mapper (SPEC §15):
//! - [`interchange`]: the normalized JSON model and the import planner
//!   (interchange → one journaled `Op::Batch`).
//! - [`vault`]: folder of arbitrary Markdown (e.g. an Obsidian vault) → tree → interchange.
//! - [`loose`]: read-only adapter for folders opened "as is" (no `.lull`).
//! - [`export`]: interchange → Markdown file, Markdown bundle, HTML; board `.zip`.
//! - [`html`]: Markdown → safe, self-contained HTML (tables, callouts, code).
//! - [`archive`]: zip writing and safe extraction.
//! - [`templates`], [`upgrade`], [`settings`]: templates, schema upgrades, settings bundles.
//! - [`service`]: the `impl Core` entry points used by the RPC layer.

pub mod archive;
pub mod export;
pub mod html;
pub mod interchange;
pub mod loose;
pub mod service;
pub mod settings;
pub mod templates;
pub mod upgrade;
pub mod vault;

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Names never imported / exported from user folders.
pub(crate) fn skip_name(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "__MACOSX"
}

pub(crate) fn is_md(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".md") || l.ends_with(".markdown")
}

pub(crate) fn md_stem(name: &str) -> &str {
    let l = name.to_ascii_lowercase();
    if l.ends_with(".markdown") {
        &name[..name.len() - 9]
    } else if l.ends_with(".md") {
        &name[..name.len() - 3]
    } else {
        name
    }
}

/// Normalize `base/rel` (both forward-slash, relative) resolving `.` and `..`.
/// Returns `None` when the result escapes the root or is absolute.
pub(crate) fn normalize_rel(base: &str, rel: &str) -> Option<String> {
    let rel = rel.replace('\\', "/");
    if rel.starts_with('/') || rel.contains(':') {
        return None;
    }
    let mut parts: Vec<String> = base
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    for c in Path::new(&rel).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// Make a file-system friendly name from a title (keeps case and spaces).
pub(crate) fn file_stem_from_title(title: &str, max: usize) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if matches!(
                c,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '[' | ']'
            ) || c.is_control()
            {
                '-'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_matches('.').trim();
    let s: String = s.chars().take(max).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "Untitled".to_string()
    } else {
        s
    }
}

/// Copy a regular file (never a symlink) creating parent folders.
pub(crate) fn copy_regular(src: &Path, dest: &Path) -> Result<u64> {
    let meta = std::fs::symlink_metadata(src).map_err(|e| Error::io(src, e))?;
    if !meta.is_file() {
        return Err(Error::invalid(format!(
            "not a regular file: {}",
            src.display()
        )));
    }
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| Error::io(p, e))?;
    }
    std::fs::copy(src, dest).map_err(|e| Error::io(dest, e))
}

/// True when `inner` is `outer` or below it (canonicalizing the nearest
/// existing ancestor, so not-yet-created export paths compare correctly).
pub(crate) fn is_within(inner: &Path, outer: &Path) -> bool {
    fn canon(p: &Path) -> PathBuf {
        let mut tail = Vec::new();
        let mut cur = p;
        loop {
            if let Ok(c) = cur.canonicalize() {
                let mut out = c;
                for t in tail.iter().rev() {
                    out.push(t);
                }
                return out;
            }
            match (cur.parent(), cur.file_name()) {
                (Some(parent), Some(name)) => {
                    tail.push(name.to_os_string());
                    cur = parent;
                }
                _ => return p.to_path_buf(),
            }
        }
    }
    canon(inner).starts_with(canon(outer))
}

/// Case-insensitive "natural" order: `2 Notes` < `10 Notes`.
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.to_lowercase().chars() {
            let d = c.is_ascii_digit();
            match out.last_mut() {
                Some((is_d, buf)) if *is_d == d => buf.push(c),
                _ => out.push((d, c.to_string())),
            }
        }
        out
    }
    let (x, y) = (chunks(a), chunks(b));
    for (p, q) in x.iter().zip(y.iter()) {
        let o = match (p.0, q.0) {
            (true, true) => {
                let (pa, qa) = (p.1.trim_start_matches('0'), q.1.trim_start_matches('0'));
                pa.len().cmp(&qa.len()).then_with(|| pa.cmp(qa))
            }
            _ => p.1.cmp(&q.1),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    x.len().cmp(&y.len()).then_with(|| a.cmp(b))
}

/// Escape text for HTML element content and attribute values.
pub(crate) fn esc_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_relative_paths() {
        assert_eq!(
            normalize_rel("a/b", "../img/x.png").as_deref(),
            Some("a/img/x.png")
        );
        assert_eq!(normalize_rel("", "./x.png").as_deref(), Some("x.png"));
        assert_eq!(normalize_rel("a", "../../x"), None);
        assert_eq!(normalize_rel("", "/etc/passwd"), None);
        assert_eq!(normalize_rel("", "C:/x"), None);
    }

    #[test]
    fn titles_to_file_names() {
        assert_eq!(file_stem_from_title("Plan: Q4 / H1?", 80), "Plan- Q4 - H1-");
        assert_eq!(file_stem_from_title("  ", 80), "Untitled");
        assert_eq!(file_stem_from_title("../..", 80), "-");
        assert_eq!(md_stem("Note.MD"), "Note");
        assert!(is_md("a.markdown") && !is_md("a.txt"));
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["10 b", "2 a", "Alpha", "alpha 2", "1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["1", "2 a", "10 b", "Alpha", "alpha 2"]);
    }

    #[test]
    fn escapes_html() {
        assert_eq!(
            esc_html("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }
}
