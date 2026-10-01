//! Small Markdown writing helpers shared by the ADF and wiki-markup converters.

/// Join rendered blocks with a blank line, skipping empty ones.
pub fn join_blocks(blocks: Vec<String>) -> String {
    blocks
        .into_iter()
        .filter(|b| !b.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Indent every line but the first by `n` spaces (list item continuation).
pub fn indent_rest(body: &str, n: usize) -> String {
    let pad = " ".repeat(n);
    let mut out = String::with_capacity(body.len() + 8);
    for (i, line) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&pad);
            }
        }
        out.push_str(line);
    }
    out
}

/// Escape characters that would otherwise start Markdown syntax inside inline text.
pub fn escape_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        let needs = match c {
            '\\' | '*' | '`' | '[' | ']' => true,
            // Intra-word underscores (snake_case) are literal in CommonMark.
            '_' => {
                !(prev.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric))
            }
            '~' => next == Some('~') || prev == Some('~'),
            '<' => next.is_some_and(|n| n.is_ascii_alphabetic() || n == '/' || n == '!'),
            _ => false,
        };
        if needs {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Tidy converter output: no leading/trailing blank lines, at most one blank
/// line between blocks (outside fenced code), no trailing spaces.
pub fn normalize_md(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut fence: Option<String> = None;
    let mut blank = 0;
    for line in s.split('\n') {
        let t = line.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            out.push(line);
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            let f: String = t.chars().take_while(|c| *c == '`' || *c == '~').collect();
            fence = Some(f);
            blank = 0;
            out.push(line.trim_end());
            continue;
        }
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
            out.push("");
        } else {
            blank = 0;
            // Keep a trailing backslash (hard break) but drop trailing spaces.
            out.push(line.trim_end());
        }
    }
    let joined = out.join("\n");
    joined.trim_matches('\n').to_string()
}

/// Render a GFM table. The first row is always used as the header row
/// (GFM requires one); `header_first` only documents the source intent.
pub fn table_md(rows: &[Vec<String>], header_first: bool) -> String {
    let _ = header_first;
    if rows.is_empty() {
        return String::new();
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', "<br>");
    let line = |r: &Vec<String>| {
        let mut cells: Vec<String> = r.iter().map(|c| cell(c)).collect();
        cells.resize(cols, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0])];
    out.push(format!("| {} |", vec!["---"; cols].join(" | ")));
    for r in &rows[1..] {
        out.push(line(r));
    }
    out.join("\n")
}

/// Recognise an Obsidian-style callout marker at the start of `text`:
/// `[!warning] Title` → `("warning", " Title")`.
pub fn callout_kind(text: &str) -> Option<(String, &str)> {
    let rest = text.trim_start().strip_prefix("[!")?;
    let end = rest.find(']')?;
    let kind = &rest[..end];
    if kind.is_empty()
        || kind.len() > 20
        || !kind.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
    {
        return None;
    }
    let after = &rest[end + 1..];
    // Obsidian fold markers.
    let after = after.strip_prefix(['+', '-']).unwrap_or(after);
    Some((kind.to_ascii_lowercase(), after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(
            escape_inline("a*b [c] snake_case _x_"),
            "a\\*b \\[c\\] snake_case \\_x\\_"
        );
        assert_eq!(escape_inline("a < b <div>"), "a < b \\<div>");
    }

    #[test]
    fn tables_pad_and_escape() {
        let t = table_md(&[vec!["A".into()], vec!["1".into(), "x|y".into()]], true);
        assert_eq!(t, "| A |  |\n| --- | --- |\n| 1 | x\\|y |");
    }

    #[test]
    fn callouts() {
        assert_eq!(
            callout_kind("[!Warning] Hi"),
            Some(("warning".into(), " Hi"))
        );
        assert_eq!(callout_kind("[!note]-\nx"), Some(("note".into(), "\nx")));
        assert_eq!(callout_kind("[x] no"), None);
    }

    #[test]
    fn normalize_keeps_code() {
        assert_eq!(
            normalize_md("\n\na  \n\n\n\nb\n```\nx\n\n\ny\n```\n"),
            "a\n\nb\n```\nx\n\n\ny\n```"
        );
    }

    #[test]
    fn indent() {
        assert_eq!(indent_rest("a\n\nb", 2), "a\n\n  b");
    }
}
