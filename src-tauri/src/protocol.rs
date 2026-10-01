//! `luau://localhost/<boardId>/<relative path>[?w=<thumb width>]`
//! Serves board files (attachments, images) to the webview with optional
//! cached thumbnails and HTTP range support (for audio/video seeking).

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use luau_core::app::{Core, files};
use tauri::http::{Request, Response, StatusCode, header};

/// Largest body served in one response; bigger files are served in ranges.
const MAX_BODY: u64 = 32 * 1024 * 1024;
const CHUNK: u64 = 8 * 1024 * 1024;
/// Board files are data, never active documents (SVG scripts, HTML).
const CSP: &str = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; sandbox";

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

pub fn handle(core: &Arc<Core>, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = req.uri();
    let raw_path = uri.path().trim_start_matches('/');
    let decoded = percent_encoding::percent_decode_str(raw_path)
        .decode_utf8_lossy()
        .into_owned();
    let Some((board, rel)) = decoded.split_once('/') else {
        return respond(StatusCode::BAD_REQUEST, "text/plain", b"bad path".to_vec());
    };
    let width: Option<u32> = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("w=")))
        .and_then(|w| w.parse().ok());
    let root = match core.board_root(board) {
        Ok(r) => r,
        Err(_) => {
            return respond(
                StatusCode::NOT_FOUND,
                "text/plain",
                b"unknown board".to_vec(),
            );
        }
    };
    let path = match core.board_file(board, rel) {
        Ok(p) => p,
        Err(_) => return respond(StatusCode::FORBIDDEN, "text/plain", b"forbidden".to_vec()),
    };
    if !path.is_file() {
        return respond(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec());
    }
    if let Some(w) = width
        && let Some((bytes, mime)) = files::thumbnail(&root, &path, w)
    {
        return respond(StatusCode::OK, mime, bytes);
    }
    let mime = files::mime_for(&path);
    let Ok(mut f) = std::fs::File::open(&path) else {
        return respond(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec());
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| r.strip_prefix("bytes="))
        .map(|spec| parse_range(spec, len));
    let (start, end) = match range {
        Some(Some(r)) => r,
        Some(None) => {
            let mut r = respond(StatusCode::RANGE_NOT_SATISFIABLE, "text/plain", Vec::new());
            if let Ok(v) = header::HeaderValue::from_str(&format!("bytes */{len}")) {
                r.headers_mut().insert(header::CONTENT_RANGE, v);
            }
            return r;
        }
        // No Range: whole file when small, otherwise the first chunk as 206
        // (media elements continue with ranges) — never the whole file in memory.
        None if len <= MAX_BODY => {
            let mut body = Vec::with_capacity(len as usize);
            let _ = f.read_to_end(&mut body);
            let mut resp = respond(StatusCode::OK, mime, body);
            resp.headers_mut().insert(
                header::ACCEPT_RANGES,
                header::HeaderValue::from_static("bytes"),
            );
            return resp;
        }
        None => (0, CHUNK - 1),
    };
    let end = end
        .min(start.saturating_add(CHUNK - 1))
        .min(len.saturating_sub(1));
    let mut buf = vec![0u8; (end - start + 1) as usize];
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut buf).is_err() {
        return respond(StatusCode::INTERNAL_SERVER_ERROR, "text/plain", Vec::new());
    }
    let mut resp = respond(StatusCode::PARTIAL_CONTENT, mime, buf);
    let h = resp.headers_mut();
    h.insert(
        header::ACCEPT_RANGES,
        header::HeaderValue::from_static("bytes"),
    );
    if let Ok(v) = header::HeaderValue::from_str(&format!("bytes {start}-{end}/{len}")) {
        h.insert(header::CONTENT_RANGE, v);
    }
    resp
}

/// `start-end`, `start-` or suffix `-n`. `None` = not satisfiable (416).
fn parse_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    let (s, e) = spec.split(',').next()?.trim().split_once('-')?;
    if len == 0 {
        return None;
    }
    let (start, end) = if s.is_empty() {
        let n: u64 = e.parse().ok()?;
        if n == 0 {
            return None;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = s.parse().ok()?;
        let end = if e.is_empty() {
            len - 1
        } else {
            e.parse::<u64>().ok()?.min(len - 1)
        };
        (start, end)
    };
    (start <= end && start < len).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::parse_range;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_range("1000-", 1000), None);
        assert_eq!(parse_range("50-10", 1000), None);
        assert_eq!(parse_range("x-1", 1000), None);
        assert_eq!(parse_range("18446744073709551615-", 10), None);
    }
}
