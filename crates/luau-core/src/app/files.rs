//! Attachments, thumbnails, cross-board transfer, unlinked-file cleanup, git.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::fsutil::{atomic_write, move_path, safe_join, sanitize_name, sha256_hex};
use crate::ids::attachment_token;
use crate::model::*;
use crate::store::{
    BoardStore, Changes, Placement, list_attachments, load_node_tree, marker_dir,
    parse_attachment_name, trash,
};

/// Largest side for images downloaded from integrations (keeps storage small).
pub const MAX_IMPORTED_IMAGE_SIDE: u32 = 2400;

pub enum Source<'a> {
    Path(&'a Path),
    Bytes(&'a [u8]),
}

/// Copy a file into the card's attachment folder. Returns the new attachment.
pub fn add_attachment(
    store: &mut BoardStore,
    card: &str,
    src: Source,
    name: &str,
) -> Result<(Attachment, Changes)> {
    let dir = store
        .state
        .attachment_dir(card)
        .ok_or_else(|| Error::not_found(card))?;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e))
            if !e.is_empty() && e.len() <= 12 && e.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            (s, e.to_ascii_lowercase())
        }
        _ => (name, "bin".to_string()),
    };
    let stem = sanitize_name(stem, 40);
    let file = loop {
        let tok = attachment_token();
        let f = if stem.is_empty() {
            format!("{card}.{tok}.{ext}")
        } else {
            format!("{card}.{tok}-{stem}.{ext}")
        };
        if !dir.join(&f).exists() {
            break f;
        }
    };
    let dest = dir.join(&file);
    match src {
        Source::Path(p) => {
            fs::copy(p, &dest).map_err(|e| Error::io(&dest, e))?;
        }
        Source::Bytes(b) => atomic_write(&dest, b)?,
    }
    store.touch(&dest);
    store.rescan_attachments(card);
    let att = store
        .state
        .nodes
        .get(card)
        .and_then(|n| n.attachments.iter().find(|a| a.file == file).cloned())
        .ok_or_else(|| Error::not_found(file.clone()))?;
    let mut ch = Changes::default();
    ch.nodes.insert(card.to_string());
    Ok((att, ch))
}

/// Downscale very large images and re-encode (used for integration imports).
pub fn optimize_image(bytes: &[u8]) -> Option<(Vec<u8>, &'static str)> {
    let img = image::load_from_memory(bytes).ok()?;
    let (w, h) = (img.width(), img.height());
    let img = if w.max(h) > MAX_IMPORTED_IMAGE_SIDE {
        img.resize(
            MAX_IMPORTED_IMAGE_SIDE,
            MAX_IMPORTED_IMAGE_SIDE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let mut out = std::io::Cursor::new(Vec::new());
    if img.color().has_alpha() {
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        let v = out.into_inner();
        (v.len() < bytes.len() || w.max(h) > MAX_IMPORTED_IMAGE_SIDE).then_some((v, "png"))
    } else {
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 84);
        img.to_rgb8().write_with_encoder(enc).ok()?;
        let v = out.into_inner();
        (v.len() < bytes.len() || w.max(h) > MAX_IMPORTED_IMAGE_SIDE).then_some((v, "jpg"))
    }
}

/// Resolve a board-relative path for the `luau://` protocol (never inside `.luau/`).
pub fn resolve_board_file(root: &Path, rel: &str) -> Result<PathBuf> {
    let p = safe_join(root, rel)?;
    let first = Path::new(rel.trim_start_matches("./")).components().next();
    if first.is_some_and(|c| c.as_os_str() == crate::brand::MARKER_DIR) {
        return Err(Error::invalid("internal path"));
    }
    Ok(p)
}

pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        Some("pdf") => "application/pdf",
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("m4a") => "audio/mp4",
        Some("txt") | Some("log") | Some("md") | Some("csv") => "text/plain; charset=utf-8",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

/// Thumbnail (max `width` px wide) cached under `.luau/cache/thumbs/`.
/// Returns `None` for formats we do not rasterize (served as original).
pub fn thumbnail(root: &Path, file: &Path, width: u32) -> Option<(Vec<u8>, &'static str)> {
    if !crate::store::is_board(root) {
        return None; // folders opened "as is" get no `.luau/cache`
    }
    let ext = file.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
    ) {
        return None;
    }
    let meta = fs::metadata(file).ok()?;
    let width = width.clamp(32, 2048);
    let key = sha256_hex(
        format!(
            "{}|{}|{}|{width}",
            file.display(),
            meta.len(),
            crate::fsutil::mtime_ms(&meta)
        )
        .as_bytes(),
    );
    let dir = marker_dir(root).join("cache").join("thumbs");
    for (e, mime) in [("jpg", "image/jpeg"), ("png", "image/png")] {
        let p = dir.join(format!("{}.{e}", &key[..24]));
        if let Ok(b) = fs::read(&p) {
            return Some((b, mime));
        }
    }
    let img = image::open(file).ok()?;
    if img.width() <= width {
        return None;
    }
    let t = img.thumbnail(width, width.saturating_mul(4));
    let mut out = std::io::Cursor::new(Vec::new());
    let (e, mime) = if t.color().has_alpha() {
        t.write_to(&mut out, image::ImageFormat::Png).ok()?;
        ("png", "image/png")
    } else {
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
        t.to_rgb8().write_with_encoder(enc).ok()?;
        ("jpg", "image/jpeg")
    };
    let bytes = out.into_inner();
    let _ = fs::create_dir_all(&dir);
    let _ = atomic_write(&dir.join(format!("{}.{e}", &key[..24])), &bytes);
    Some((bytes, mime))
}

/// Move nodes (with subtrees and attachments) from `src` board into `dst`.
pub fn transfer(
    src: &mut BoardStore,
    dst: &mut BoardStore,
    items: &[Placement],
) -> Result<(Changes, Changes)> {
    let mut sch = Changes::default();
    let mut dch = Changes::default();
    let ids: Vec<&Placement> = items
        .iter()
        .filter(|i| src.state.nodes.contains_key(&i.id))
        .collect();
    let ids: Vec<&Placement> = ids
        .iter()
        .filter(|i| {
            !ids.iter()
                .any(|o| o.id != i.id && src.state.is_ancestor(&o.id, &i.id))
        })
        .copied()
        .collect();
    // Collision check on the full subtrees.
    for it in &ids {
        let mut all = src.state.descendants(&it.id);
        all.push(it.id.clone());
        if let Some(c) = all.iter().find(|x| dst.state.nodes.contains_key(*x)) {
            return Err(Error::Conflict(format!("id already on target board: {c}")));
        }
    }
    let mut sorted = ids.clone();
    sorted.sort_by_key(|p| p.index);
    for it in sorted {
        if let Parent::Card(c) = &it.parent {
            dst.ensure_group(c, &mut dch)?;
        }
        let n = src
            .state
            .nodes
            .get(&it.id)
            .cloned()
            .ok_or_else(|| Error::not_found(it.id.clone()))?;
        let from_dir = src
            .state
            .container_dir(&n.parent)
            .ok_or_else(|| Error::not_found("source"))?;
        let to_dir = dst
            .state
            .container_dir(&it.parent)
            .ok_or_else(|| Error::not_found("target"))?;
        fs::create_dir_all(&to_dir).map_err(|e| Error::io(&to_dir, e))?;
        if n.is_group {
            move_path(&from_dir.join(&it.id), &to_dir.join(&it.id))?;
        } else {
            move_path(
                &from_dir.join(format!("{}.md", it.id)),
                &to_dir.join(format!("{}.md", it.id)),
            )?;
            for a in list_attachments(&from_dir, &it.id) {
                move_path(&from_dir.join(&a.file), &to_dir.join(&a.file))?;
            }
        }
        src.touch(&from_dir.join(&it.id));
        src.touch(&from_dir.join(format!("{}.md", it.id)));
        dst.touch(&to_dir.join(&it.id));
        dst.touch(&to_dir.join(format!("{}.md", it.id)));
        // Detach from source.
        let desc = src.state.descendants(&it.id);
        if let Some(list) = src.state.children_of_mut(&n.parent) {
            list.retain(|c| c != &it.id);
        }
        src.state.nodes.remove(&it.id);
        for d in &desc {
            src.state.nodes.remove(d);
        }
        sch.removed.insert(it.id.clone());
        sch.removed.extend(desc);
        sch.touch_parent(&n.parent);
        if let Parent::Card(g) = &n.parent
            && src
                .state
                .nodes
                .get(g)
                .is_some_and(|x| x.children.is_empty())
        {
            src.ensure_plain(g, &mut sch)?;
        }
        src.save_container(&n.parent)?;
        // Attach to destination.
        let loaded = load_node_tree(&dst.state.root, &to_dir, it.parent.clone(), &it.id)?;
        for (k, mut v) in loaded {
            if k == it.id {
                v.archived = n.archived;
                v.cover = n.cover.clone();
            }
            dch.nodes.insert(k.clone());
            dst.state.nodes.insert(k, v);
        }
        let list = dst
            .state
            .children_of_mut(&it.parent)
            .ok_or_else(|| Error::not_found("target"))?;
        let at = it.index.min(list.len());
        list.insert(at, it.id.clone());
        dst.save_container(&it.parent)?;
        dch.touch_parent(&it.parent);
    }
    src.state.version += 1;
    dst.state.version += 1;
    Ok((sch, dch))
}

fn tracker_path(root: &Path) -> PathBuf {
    marker_dir(root).join("cache").join("unlinked.json")
}

/// Move attachments no longer referenced by their card to trash once they have
/// been unreferenced for `ttl_days` (or immediately when `now` is set).
pub fn sweep_unlinked(
    store: &mut BoardStore,
    ttl_days: u64,
    now: bool,
) -> Result<(usize, Changes)> {
    if store.state.loose.is_some() {
        return Ok((0, Changes::default())); // never touch files of folders opened "as is"
    }
    let root = store.state.root.clone();
    let mut tracker: BTreeMap<String, i64> = fs::read_to_string(tracker_path(&root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let ttl_ms = ttl_days as i64 * 86_400_000;
    let mut seen = HashSet::new();
    let mut moved = 0;
    let mut ch = Changes::default();
    let ids: Vec<String> = store.state.nodes.keys().cloned().collect();
    for id in ids {
        let (atts, refs, cover) = {
            let n = &store.state.nodes[&id];
            let refs: HashSet<String> = n
                .meta
                .file_refs
                .iter()
                .map(|r| r.rsplit('/').next().unwrap_or(r).to_string())
                .collect();
            (
                n.attachments.clone(),
                refs,
                n.cover.as_ref().map(|c| c.file.clone()),
            )
        };
        let Some(dir) = store.state.attachment_dir(&id) else {
            continue;
        };
        for a in atts {
            if refs.contains(&a.file) || cover.as_deref() == Some(a.file.as_str()) {
                continue;
            }
            let rel = store.state.rel(&dir.join(&a.file));
            seen.insert(rel.clone());
            let first = *tracker.entry(rel.clone()).or_insert(now_ms);
            if now || now_ms - first >= ttl_ms {
                let dest = trash::orphans_dir(&root).join(format!(
                    "{}-{}",
                    chrono::Utc::now().format("%Y%m%dT%H%M%S%3f"),
                    a.file
                ));
                if move_path(&dir.join(&a.file), &dest).is_ok() {
                    store.touch(&dir.join(&a.file));
                    tracker.remove(&rel);
                    moved += 1;
                    ch.nodes.insert(id.clone());
                }
            }
        }
        if ch.nodes.contains(&id) {
            store.rescan_attachments(&id);
        }
    }
    tracker.retain(|k, _| seen.contains(k));
    let _ = fs::create_dir_all(marker_dir(&root).join("cache"));
    let _ = atomic_write(
        &tracker_path(&root),
        serde_json::to_string(&tracker)
            .unwrap_or_default()
            .as_bytes(),
    );
    Ok((moved, ch))
}

/// Parse an attachment reference to its owner id (for UI helpers).
pub fn owner_of(file: &str) -> Option<String> {
    parse_attachment_name(file).map(|(o, _)| o)
}

fn git_cmd() -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// `git init` unless the folder is already inside a work tree (then leave it alone).
pub fn git_init(path: &Path) -> bool {
    let inside = git_cmd()
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if inside {
        return false;
    }
    git_cmd()
        .arg("-C")
        .arg(path)
        .args(["init", "-q"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
