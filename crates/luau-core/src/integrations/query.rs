//! Search query resolution for the integrations panel (pure, no I/O).
//!
//! The panel offers two modes for Jira: **JQL** (sent as typed) and **plain
//! text**, which is turned into `text ~ "…"` here. User text is never spliced
//! into JQL raw: it is cleaned (control characters, length), Lucene operators
//! are neutralized and the result is escaped for a JQL string literal.
//! Trello has no query language beyond its own search syntax, so it always
//! receives the text as a single URL-encoded parameter.

use serde::{Deserialize, Serialize};

use super::types::ProviderKind;
use crate::error::{Error, Result};

/// Longest JQL accepted from the UI.
pub const MAX_JQL_LEN: usize = 4000;
/// Longest plain-text search accepted from the UI.
pub const MAX_TEXT_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    #[default]
    Jql,
    Text,
}

/// Characters with a meaning in Lucene query syntax (what Jira's `~` operator
/// parses). They are escaped so the text is matched literally.
const LUCENE_SPECIAL: &[char] = &[
    '+', '-', '&', '|', '!', '(', ')', '{', '}', '[', ']', '^', '~', '*', '?', ':', '/', '"', '\\',
];

/// Collapse whitespace and drop control characters (newlines, NUL, bidi
/// overrides…); `None` when longer than `max` characters afterwards.
fn clean(s: &str, max: usize) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        let bidi = matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
        if c.is_whitespace() || c.is_control() || bidi {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(c);
    }
    (out.chars().count() <= max).then_some(out)
}

/// Escape `text` for use inside a double-quoted JQL string given to `~`.
///
/// Two layers: Lucene (each special character gets a `\`, and the boolean
/// operators `AND`/`OR`/`NOT`, which are case-sensitive in Lucene, are
/// lower-cased so they become plain words) and then JQL string escaping
/// (`\` → `\\`, `"` → `\"`).
pub fn escape_text(text: &str) -> String {
    let mut lucene = String::with_capacity(text.len() + 8);
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            lucene.push(' ');
        }
        let word = match word {
            "AND" | "OR" | "NOT" => word.to_ascii_lowercase(),
            w => w.to_string(),
        };
        for c in word.chars() {
            if LUCENE_SPECIAL.contains(&c) {
                lucene.push('\\');
            }
            lucene.push(c);
        }
    }
    let mut jql = String::with_capacity(lucene.len() + 8);
    for c in lucene.chars() {
        if c == '\\' || c == '"' {
            jql.push('\\');
        }
        jql.push(c);
    }
    jql
}

/// A Jira project key as Jira allows them (letters, digits, `_`; starts with a letter).
fn valid_project_key(k: &str) -> bool {
    let mut chars = k.chars();
    k.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Build the JQL for a plain-text search, optionally scoped to one project.
pub fn text_jql(text: &str, project: Option<&str>) -> Result<String> {
    let text = clean(text, MAX_TEXT_LEN).ok_or_else(|| Error::invalid("query_too_long"))?;
    if text.is_empty() {
        return Err(Error::invalid("query_empty"));
    }
    let mut jql = String::new();
    if let Some(p) = project.map(str::trim).filter(|p| !p.is_empty()) {
        if !valid_project_key(p) {
            return Err(Error::invalid("invalid_project"));
        }
        jql.push_str(&format!("project = \"{p}\" AND "));
    }
    jql.push_str(&format!(
        "text ~ \"{}\" ORDER BY updated DESC",
        escape_text(&text)
    ));
    Ok(jql)
}

/// The query actually sent to the provider. An empty query means the
/// account's default (`default_query`).
pub fn resolve(
    provider: ProviderKind,
    mode: SearchMode,
    raw: &str,
    project: Option<&str>,
    default_query: String,
) -> Result<String> {
    if raw.trim().is_empty() {
        return Ok(default_query);
    }
    match (provider.is_jira(), mode) {
        (true, SearchMode::Text) => text_jql(raw, project),
        (true, SearchMode::Jql) => {
            if raw.chars().count() > MAX_JQL_LEN {
                return Err(Error::invalid("query_too_long"));
            }
            Ok(raw.to_string())
        }
        // Trello: always plain text (URL-encoded as one parameter by the client).
        (false, _) => clean(raw, MAX_TEXT_LEN).ok_or_else(|| Error::invalid("query_too_long")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_are_quoted() {
        assert_eq!(
            text_jql("login fails", None).unwrap(),
            r#"text ~ "login fails" ORDER BY updated DESC"#
        );
    }

    #[test]
    fn quotes_and_backslashes_cannot_break_out() {
        // A classic injection attempt: close the string and add a clause.
        let j = text_jql(r#"x" OR project = SECRET OR text ~ "y"#, None).unwrap();
        assert_eq!(
            j,
            r#"text ~ "x\\\" or project = SECRET or text \\~ \\\"y" ORDER BY updated DESC"#
        );
        // Exactly one unescaped string literal: the opening and closing quote.
        let unescaped = unescaped_quotes(&j);
        assert_eq!(unescaped, 2, "{j}");

        let j = text_jql(r"C:\temp\", None).unwrap();
        assert_eq!(j, r#"text ~ "C\\:\\\\temp\\\\" ORDER BY updated DESC"#);
        assert_eq!(unescaped_quotes(&j), 2);
    }

    /// Count `"` not preceded by an odd number of backslashes.
    fn unescaped_quotes(s: &str) -> usize {
        let b = s.as_bytes();
        (0..b.len())
            .filter(|&i| {
                b[i] == b'"' && {
                    let n = b[..i].iter().rev().take_while(|&&c| c == b'\\').count();
                    n % 2 == 0
                }
            })
            .count()
    }

    #[test]
    fn lucene_operators_are_literal() {
        assert_eq!(escape_text("a+b (c)"), r"a\\+b \\(c\\)");
        assert_eq!(escape_text("wild*card?"), r"wild\\*card\\?");
        assert_eq!(escape_text("fix AND ship NOT now"), "fix and ship not now");
        // Lower-case words are plain terms already.
        assert_eq!(escape_text("and or not"), "and or not");
        assert_eq!(escape_text("LUAU-101"), r"LUAU\\-101");
    }

    #[test]
    fn control_chars_and_whitespace_are_cleaned() {
        assert_eq!(
            text_jql("  a\n\tb\u{0}c \u{202E}d  ", None).unwrap(),
            r#"text ~ "a b c d" ORDER BY updated DESC"#
        );
        assert!(matches!(text_jql(" \n ", None), Err(Error::Invalid(m)) if m == "query_empty"));
    }

    #[test]
    fn unicode_is_kept() {
        assert_eq!(
            text_jql("canción ñandú 日本", None).unwrap(),
            r#"text ~ "canción ñandú 日本" ORDER BY updated DESC"#
        );
    }

    #[test]
    fn length_is_capped() {
        let long = "a".repeat(MAX_TEXT_LEN + 1);
        assert!(matches!(text_jql(&long, None), Err(Error::Invalid(m)) if m == "query_too_long"));
        assert!(text_jql(&"a".repeat(MAX_TEXT_LEN), None).is_ok());
    }

    #[test]
    fn project_scope_is_validated() {
        assert_eq!(
            text_jql("bug", Some("LUAU")).unwrap(),
            r#"project = "LUAU" AND text ~ "bug" ORDER BY updated DESC"#
        );
        for bad in [r#"X" OR "1"="1"#, "1ABC", "A B", "a-b"] {
            assert!(text_jql("bug", Some(bad)).is_err(), "{bad}");
        }
        // Blank project = no scope.
        assert_eq!(
            text_jql("bug", Some(" ")).unwrap(),
            r#"text ~ "bug" ORDER BY updated DESC"#
        );
    }

    #[test]
    fn resolve_by_provider_and_mode() {
        let jira = ProviderKind::JiraCloud;
        let dflt = || "assignee = currentUser()".to_string();
        assert_eq!(
            resolve(jira, SearchMode::Jql, "  ", None, dflt()).unwrap(),
            dflt()
        );
        assert_eq!(
            resolve(jira, SearchMode::Text, "", None, dflt()).unwrap(),
            dflt()
        );
        // JQL mode sends the query as typed.
        assert_eq!(
            resolve(
                jira,
                SearchMode::Jql,
                "project = X AND type = Bug",
                None,
                dflt()
            )
            .unwrap(),
            "project = X AND type = Bug"
        );
        assert!(
            resolve(
                jira,
                SearchMode::Jql,
                &"x".repeat(MAX_JQL_LEN + 1),
                None,
                dflt()
            )
            .is_err()
        );
        assert_eq!(
            resolve(
                ProviderKind::JiraServer,
                SearchMode::Text,
                "a\"b",
                None,
                dflt()
            )
            .unwrap(),
            r#"text ~ "a\\\"b" ORDER BY updated DESC"#
        );
        // Trello ignores the mode: cleaned plain text.
        let trello = ProviderKind::Trello;
        assert_eq!(
            resolve(
                trello,
                SearchMode::Jql,
                " is:open\nlogin ",
                None,
                String::new()
            )
            .unwrap(),
            "is:open login"
        );
    }

    #[test]
    fn mode_serde() {
        assert_eq!(
            serde_json::to_string(&SearchMode::Text).unwrap(),
            "\"text\""
        );
        assert_eq!(
            serde_json::from_str::<SearchMode>("\"jql\"").unwrap(),
            SearchMode::Jql
        );
    }
}
