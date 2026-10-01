//! Search query syntax.
//!
//! ```text
//! login "exact phrase" -draft tag:backend -tag:wip board:"Project Alpha"
//! lane:Doing is:open is:archived is:remote is:group has:image has:tasks
//! priority:high assignee:ana due:<2026-10-10 due:overdue updated:>2026-09-01
//! links:c1a2b3c linkedfrom:c1a2b3c case:yes in:title
//! ```
//! Unknown `key:value` pairs are treated as plain text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cmp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    pub value: String,
    pub negate: bool,
    pub phrase: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub key: String,
    pub cmp: Cmp,
    pub value: String,
    pub negate: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub terms: Vec<Term>,
    pub filters: Vec<Filter>,
    pub case_sensitive: bool,
    pub title_only: bool,
}

pub const KEYS: &[&str] = &[
    "tag",
    "label",
    "board",
    "lane",
    "is",
    "has",
    "status",
    "priority",
    "assignee",
    "mention",
    "due",
    "updated",
    "created",
    "links",
    "linkedfrom",
    "type",
    "in",
    "case",
    "id",
    "key",
];

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unquote(s: &str) -> (String, bool) {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        (s[1..s.len() - 1].to_string(), true)
    } else {
        (s.trim_matches('"').to_string(), false)
    }
}

pub fn parse(input: &str) -> Query {
    let mut q = Query::default();
    for tok in tokenize(input) {
        let (negate, body) = match tok.strip_prefix('-') {
            Some(rest) if !rest.is_empty() => (true, rest.to_string()),
            _ => (false, tok.clone()),
        };
        if let Some((k, v)) = body.split_once(':') {
            let key = k.to_lowercase();
            if KEYS.contains(&key.as_str()) && !v.is_empty() && !k.starts_with('"') {
                let (cmp, rest) = if let Some(r) = v.strip_prefix("<=") {
                    (Cmp::Le, r)
                } else if let Some(r) = v.strip_prefix(">=") {
                    (Cmp::Ge, r)
                } else if let Some(r) = v.strip_prefix('<') {
                    (Cmp::Lt, r)
                } else if let Some(r) = v.strip_prefix('>') {
                    (Cmp::Gt, r)
                } else {
                    (Cmp::Eq, v)
                };
                let (value, _) = unquote(rest);
                match key.as_str() {
                    "case" => {
                        q.case_sensitive =
                            matches!(value.as_str(), "yes" | "true" | "sensitive" | "1")
                    }
                    "in" if value == "title" => q.title_only = true,
                    _ => q.filters.push(Filter {
                        key,
                        cmp,
                        value: value.trim_start_matches(['@', '#']).to_string(),
                        negate,
                    }),
                }
                continue;
            }
        }
        let (value, phrase) = unquote(&body);
        if !value.is_empty() {
            q.terms.push(Term {
                value,
                negate,
                phrase,
            });
        }
    }
    q
}

impl Query {
    /// Serialize back to the canonical text form.
    pub fn to_text(&self) -> String {
        let mut parts = Vec::new();
        for t in &self.terms {
            let v = if t.phrase || t.value.contains(' ') {
                format!("\"{}\"", t.value)
            } else {
                t.value.clone()
            };
            parts.push(if t.negate { format!("-{v}") } else { v });
        }
        for f in &self.filters {
            let op = match f.cmp {
                Cmp::Eq => "",
                Cmp::Lt => "<",
                Cmp::Le => "<=",
                Cmp::Gt => ">",
                Cmp::Ge => ">=",
            };
            let v = if f.value.contains(' ') {
                format!("\"{}\"", f.value)
            } else {
                f.value.clone()
            };
            parts.push(format!(
                "{}{}:{op}{v}",
                if f.negate { "-" } else { "" },
                f.key
            ));
        }
        if self.case_sensitive {
            parts.push("case:yes".into());
        }
        if self.title_only {
            parts.push("in:title".into());
        }
        parts.join(" ")
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.filters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mixed_query() {
        let q = parse(
            r#"login "exact phrase" -draft tag:backend -tag:wip board:"Project Alpha" due:<2026-10-10 case:yes"#,
        );
        assert_eq!(q.terms.len(), 3);
        assert_eq!(
            q.terms[1],
            Term {
                value: "exact phrase".into(),
                negate: false,
                phrase: true
            }
        );
        assert!(q.terms[2].negate);
        assert_eq!(
            q.filters[0],
            Filter {
                key: "tag".into(),
                cmp: Cmp::Eq,
                value: "backend".into(),
                negate: false
            }
        );
        assert!(q.filters[1].negate);
        assert_eq!(q.filters[2].value, "Project Alpha");
        assert_eq!(q.filters[3].cmp, Cmp::Lt);
        assert!(q.case_sensitive);
    }

    #[test]
    fn unknown_keys_are_text_and_roundtrip() {
        let q = parse("http://x.com foo:bar assignee:@ana");
        assert_eq!(q.terms.len(), 2);
        assert_eq!(q.filters[0].value, "ana");
        let text = parse("a -b tag:x \"c d\" in:title").to_text();
        assert_eq!(parse(&text), parse("a -b tag:x \"c d\" in:title"));
    }
}
