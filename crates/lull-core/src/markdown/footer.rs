//! Card property footer.
//!
//! A footer is the last block of a card file: a `---` line preceded by a blank
//! line (so it is never a setext heading underline) followed only by
//! `key: value` lines until end of file.
//!
//! ```text
//! ---
//! priority: high
//! due: 2026-10-03
//! assignees: @ana, @luis
//! labels: backend, infra
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Footer {
    /// 0-based line index of the `---` separator; `None` when absent.
    pub start_line: Option<usize>,
    /// All fields in file order (keys lowercased).
    pub fields: Vec<(String, String)>,
    pub priority: Option<String>,
    pub due: Option<String>,
    pub start: Option<String>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
}

/// Keys with dedicated UI; any other `key: value` pair is still kept.
pub const KNOWN_KEYS: &[&str] = &["priority", "due", "start", "assignees", "labels", "estimate", "status"];

pub const PRIORITIES: &[&str] = &["urgent", "high", "medium", "low", "none"];

fn parse_field(line: &str) -> Option<(String, String)> {
    let (k, v) = line.split_once(':')?;
    let key = k.trim();
    if key.is_empty()
        || key.len() > 32
        || !key.chars().next()?.is_alphabetic()
        || !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' ')
    {
        return None;
    }
    Some((key.to_lowercase(), v.trim().to_string()))
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().trim_start_matches('@').trim_start_matches('#').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Locate and parse the footer in `lines` (already LF-split).
pub fn parse(lines: &[&str]) -> Footer {
    // Scan from the end: every trailing non-empty line must be a field.
    let mut idx = lines.len();
    let mut saw_field = false;
    while idx > 0 {
        let line = lines[idx - 1].trim_end();
        if line.trim().is_empty() {
            idx -= 1;
            continue;
        }
        if line.trim() == "---" {
            let preceded_ok = idx < 2 || lines[idx - 2].trim().is_empty();
            if saw_field && preceded_ok {
                return build(lines, idx - 1);
            }
            return Footer::default();
        }
        if parse_field(line).is_some() {
            saw_field = true;
            idx -= 1;
            continue;
        }
        return Footer::default();
    }
    Footer::default()
}

fn build(lines: &[&str], sep: usize) -> Footer {
    let mut f = Footer { start_line: Some(sep), ..Default::default() };
    for line in &lines[sep + 1..] {
        if let Some((k, v)) = parse_field(line) {
            match k.as_str() {
                "priority" => f.priority = (!v.is_empty()).then(|| v.to_lowercase()),
                "due" => f.due = (!v.is_empty()).then(|| v.clone()),
                "start" => f.start = (!v.is_empty()).then(|| v.clone()),
                "assignees" | "assignee" | "owners" => f.assignees = split_list(&v),
                "labels" | "label" => f.labels = split_list(&v),
                _ => {}
            }
            f.fields.push((k, v));
        }
    }
    f
}

/// Render a footer block (without leading blank line) from fields.
pub fn render(fields: &[(String, String)]) -> String {
    let mut s = String::from("---\n");
    for (k, v) in fields {
        if !v.trim().is_empty() {
            s.push_str(k);
            s.push_str(": ");
            s.push_str(v.trim());
            s.push('\n');
        }
    }
    s
}

/// Replace (or insert, or remove when `fields` is empty) the footer of `content`.
pub fn set_fields(content: &str, fields: &[(String, String)]) -> String {
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let current = parse(&lines);
    let body_end = current.start_line.unwrap_or(lines.len());
    let mut body = lines[..body_end].join("\n");
    while body.ends_with('\n') || body.ends_with(' ') {
        body.pop();
    }
    let non_empty: Vec<(String, String)> =
        fields.iter().filter(|(_, v)| !v.trim().is_empty()).cloned().collect();
    if non_empty.is_empty() {
        body.push('\n');
        return body;
    }
    format!("{body}\n\n{}", render(&non_empty))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        s.split('\n').collect()
    }

    #[test]
    fn parses_footer() {
        let src = "# T\n\nbody\n\n---\npriority: High\ndue: 2026-10-03\nassignees: @ana, @luis\nlabels: a, #b\n";
        let f = parse(&lines(src));
        assert_eq!(f.start_line, Some(4));
        assert_eq!(f.priority.as_deref(), Some("high"));
        assert_eq!(f.due.as_deref(), Some("2026-10-03"));
        assert_eq!(f.assignees, vec!["ana", "luis"]);
        assert_eq!(f.labels, vec!["a", "b"]);
    }

    #[test]
    fn setext_heading_is_not_footer() {
        let src = "# T\nSome heading\n---\npriority: high\n";
        assert_eq!(parse(&lines(src)).start_line, None);
    }

    #[test]
    fn plain_rule_is_not_footer() {
        assert_eq!(parse(&lines("# T\n\n---\n\nmore text\n")).start_line, None);
        assert_eq!(parse(&lines("# T\n\n---\n")).start_line, None);
    }

    #[test]
    fn set_fields_roundtrip() {
        let src = "# T\n\nbody\n";
        let with = set_fields(src, &[("priority".into(), "high".into()), ("due".into(), "2026-01-01".into())]);
        assert_eq!(with, "# T\n\nbody\n\n---\npriority: high\ndue: 2026-01-01\n");
        let f = parse(&lines(&with));
        assert_eq!(f.priority.as_deref(), Some("high"));
        let updated = set_fields(&with, &[("priority".into(), "low".into())]);
        assert_eq!(updated, "# T\n\nbody\n\n---\npriority: low\n");
        let removed = set_fields(&updated, &[]);
        assert_eq!(removed, "# T\n\nbody\n");
    }
}
