//! `lull://localhost/<boardId>/<relative path>[?w=<thumb width>]`
//! Serves board files (attachments, images) to the webview with optional
//! cached thumbnails and HTTP range support (for audio/video seeking).

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use lull_core::app::{Core, files};
use tauri::http::{Request, Response, StatusCode, header};

fn respond(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Content-Type-Options", "nosniff")
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
    let path = match files::resolve_board_file(&root, rel) {
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
    if let Some(range) = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        && let Some(spec) = range.strip_prefix("bytes=")
    {
        let (s, e) = spec.split_once('-').unwrap_or((spec, ""));
        let start: u64 = s.parse().unwrap_or(0);
        let end: u64 = e
            .parse()
            .unwrap_or(len.saturating_sub(1))
            .min(len.saturating_sub(1));
        let end = end.min(start + 8 * 1024 * 1024); // cap chunk at 8 MiB
        if start <= end && start < len {
            let mut buf = vec![0u8; (end - start + 1) as usize];
            if f.seek(SeekFrom::Start(start)).is_ok() && f.read_exact(&mut buf).is_ok() {
                return Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, mime)
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                    .header(header::CONTENT_LENGTH, buf.len().to_string())
                    .body(buf)
                    .unwrap_or_else(|_| Response::new(Vec::new()));
            }
        }
    }
    let mut body = Vec::with_capacity(len as usize);
    let _ = f.read_to_end(&mut body);
    let mut resp = respond(StatusCode::OK, mime, body);
    resp.headers_mut().insert(
        header::ACCEPT_RANGES,
        header::HeaderValue::from_static("bytes"),
    );
    resp
}
