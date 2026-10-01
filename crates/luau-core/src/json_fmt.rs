//! Git-friendly JSON formatting.
//!
//! Objects are expanded one key per line; values that fit on the remaining
//! line are printed compactly; long arrays of scalars wrap to fill the width
//! instead of printing one element per line. Output always ends with `\n`.

use serde::Serialize;
use serde_json::Value;

pub const DEFAULT_WIDTH: usize = 100;

pub fn to_string<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let v = serde_json::to_value(value)?;
    Ok(format_value(&v, DEFAULT_WIDTH))
}

pub fn format_value(v: &Value, width: usize) -> String {
    let mut out = String::new();
    write_value(&mut out, v, 0, width, 0);
    out.push('\n');
    out
}

/// Single-line JSON with a space after `:` and `,` (readable in diffs).
fn compact(v: &Value) -> String {
    let mut s = String::new();
    write_compact(&mut s, v);
    s
}

fn write_compact(s: &mut String, v: &Value) {
    match v {
        Value::Object(map) => {
            s.push('{');
            for (i, (k, val)) in map.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&serde_json::to_string(k).unwrap_or_default());
                s.push_str(": ");
                write_compact(s, val);
            }
            s.push('}');
        }
        Value::Array(items) => {
            s.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                write_compact(s, item);
            }
            s.push(']');
        }
        other => s.push_str(&serde_json::to_string(other).unwrap_or_default()),
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Array(_) | Value::Object(_))
}

/// `used` is the number of columns already consumed on the current line.
fn write_value(out: &mut String, v: &Value, indent: usize, width: usize, used: usize) {
    let c = compact(v);
    if used + c.len() <= width || is_scalar(v) {
        out.push_str(&c);
        return;
    }
    let pad = " ".repeat(indent + 2);
    match v {
        Value::Object(map) => {
            out.push_str("{\n");
            let n = map.len();
            for (i, (k, val)) in map.iter().enumerate() {
                let key = serde_json::to_string(k).unwrap_or_default();
                out.push_str(&pad);
                out.push_str(&key);
                out.push_str(": ");
                write_value(out, val, indent + 2, width, indent + 2 + key.len() + 2);
                if i + 1 < n {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(indent));
            out.push('}');
        }
        Value::Array(items) if items.iter().all(is_scalar) => {
            // Wrap scalars, filling lines up to `width`.
            out.push_str("[\n");
            let mut line = String::new();
            for (i, item) in items.iter().enumerate() {
                let mut piece = compact(item);
                if i + 1 < items.len() {
                    piece.push(',');
                }
                if !line.is_empty() && pad.len() + line.len() + 1 + piece.len() > width {
                    out.push_str(&pad);
                    out.push_str(&line);
                    out.push('\n');
                    line.clear();
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(&piece);
            }
            if !line.is_empty() {
                out.push_str(&pad);
                out.push_str(&line);
                out.push('\n');
            }
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                write_value(out, item, indent + 2, width, indent + 2);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(indent));
            out.push(']');
        }
        _ => out.push_str(&c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn short_object_expands_top_level_keys() {
        let v = json!({"schema": 1, "id": "k4m2p9", "name": "Backlog", "order": ["c1", "c2"]});
        let s = format_value(&v, 100);
        // The whole object fits, so it prints compactly on one line.
        assert_eq!(
            s,
            "{\"schema\": 1, \"id\": \"k4m2p9\", \"name\": \"Backlog\", \"order\": [\"c1\", \"c2\"]}\n"
        );
    }

    #[test]
    fn long_arrays_wrap_and_roundtrip() {
        let order: Vec<String> = (0..40).map(|i| format!("c{:06}", i)).collect();
        let v = json!({"schema": 1, "id": "k4m2p9", "name": "Backlog", "order": order});
        let s = format_value(&v, 60);
        assert!(s.lines().all(|l| l.len() <= 60), "{s}");
        assert!(s.lines().count() > 5);
        let back: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn nested_objects_roundtrip() {
        let v =
            json!({"a": {"b": [{"c": 1}, {"d": [1, 2, 3]}], "long": "x".repeat(120)}, "e": null});
        let s = format_value(&v, 40);
        let back: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
        assert!(s.ends_with('\n'));
    }
}
