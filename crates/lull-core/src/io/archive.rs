//! Zip writing (board exports, Markdown bundles) and hardened extraction.
//!
//! Extraction rejects absolute paths, `..` traversal, symlinks, too many
//! entries and archives that inflate beyond [`MAX_UNPACKED`] (zip bombs).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

use crate::brand::MARKER_DIR;
use crate::error::{Error, Result};

pub const MAX_ENTRIES: usize = 200_000;
pub const MAX_UNPACKED: u64 = 8 * 1024 * 1024 * 1024;

fn zerr(path: &Path, e: impl std::fmt::Display) -> Error {
    Error::Other(format!("zip {}: {e}", path.display()))
}

/// Incremental zip writer to a temporary file, renamed into place on `finish`.
pub struct ZipOut {
    zip: Option<zip::ZipWriter<fs::File>>,
    tmp: PathBuf,
    dest: PathBuf,
    opts: SimpleFileOptions,
    pub files: usize,
    pub bytes: u64,
}

impl ZipOut {
    pub fn create(dest: &Path) -> Result<Self> {
        let dir = dest.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let tmp = dir.join(format!(
            ".{}.tmp-{}",
            dest.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("export"),
            crate::ids::random_suffix(6)
        ));
        let file = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        Ok(ZipOut {
            zip: Some(zip::ZipWriter::new(file)),
            tmp,
            dest: dest.to_path_buf(),
            opts: SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644),
            files: 0,
            bytes: 0,
        })
    }

    pub fn add_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let (opts, dest) = (self.opts, self.dest.clone());
        let w = self.writer()?;
        w.start_file(name, opts).map_err(|e| zerr(&dest, e))?;
        w.write_all(bytes).map_err(|e| Error::io(&dest, e))?;
        self.files += 1;
        self.bytes += bytes.len() as u64;
        Ok(())
    }

    pub fn add_file(&mut self, name: &str, src: &Path) -> Result<()> {
        let meta = fs::symlink_metadata(src).map_err(|e| Error::io(src, e))?;
        if !meta.is_file() {
            return Ok(());
        }
        let (opts, dest) = (self.opts, self.dest.clone());
        let mut f = fs::File::open(src).map_err(|e| Error::io(src, e))?;
        let w = self.writer()?;
        w.start_file(name, opts).map_err(|e| zerr(&dest, e))?;
        let n = std::io::copy(&mut f, w).map_err(|e| Error::io(src, e))?;
        self.files += 1;
        self.bytes += n;
        Ok(())
    }

    /// Add `dir` recursively under `prefix`, keeping entries for which `keep(rel)` is true.
    pub fn add_dir(&mut self, dir: &Path, prefix: &str, keep: &dyn Fn(&str) -> bool) -> Result<()> {
        let mut stack = vec![(dir.to_path_buf(), String::new())];
        while let Some((d, rel)) = stack.pop() {
            let mut entries: Vec<_> = fs::read_dir(&d)
                .map_err(|e| Error::io(&d, e))?
                .flatten()
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned();
                let r = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_symlink() || name == ".DS_Store" || !keep(&r) {
                    continue;
                }
                if ft.is_dir() {
                    stack.push((e.path(), r));
                } else if ft.is_file() {
                    self.add_file(&format!("{prefix}{r}"), &e.path())?;
                }
            }
        }
        Ok(())
    }

    fn writer(&mut self) -> Result<&mut zip::ZipWriter<fs::File>> {
        self.zip
            .as_mut()
            .ok_or_else(|| Error::Other("zip already finished".into()))
    }

    pub fn finish(mut self) -> Result<PathBuf> {
        let zip = self
            .zip
            .take()
            .ok_or_else(|| Error::Other("zip already finished".into()))?;
        let f = zip.finish().map_err(|e| zerr(&self.dest, e))?;
        f.sync_all().map_err(|e| Error::io(&self.tmp, e))?;
        drop(f);
        if self.dest.exists() {
            let _ = fs::remove_file(&self.dest);
        }
        fs::rename(&self.tmp, &self.dest).map_err(|e| Error::io(&self.dest, e))?;
        Ok(self.dest.clone())
    }
}

impl Drop for ZipOut {
    fn drop(&mut self) {
        // Leftover temp file after an error (no-op once renamed).
        let _ = fs::remove_file(&self.tmp);
    }
}

/// Should a board-relative path be part of a board `.zip`?
/// Excludes caches, trash, VCS data and (optionally) history.
pub fn board_entry_kept(rel: &str, include_history: bool) -> bool {
    let mut parts = rel.split('/');
    let first = parts.next().unwrap_or("");
    if first == ".git" {
        return false;
    }
    if first == MARKER_DIR {
        return match parts.next() {
            Some("cache") | Some("trash") | Some("originals") => false,
            Some("history") => include_history,
            _ => true,
        };
    }
    true
}

/// Zip a whole board folder (its name becomes the top folder in the archive).
pub fn zip_board(root: &Path, dest: &Path, include_history: bool) -> Result<(usize, u64)> {
    if super::is_within(dest, root) {
        return Err(Error::invalid("the export file cannot be inside the board"));
    }
    let top = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "board".into());
    let mut z = ZipOut::create(dest)?;
    z.add_dir(root, &format!("{top}/"), &|rel| {
        board_entry_kept(rel, include_history)
    })?;
    let (files, bytes) = (z.files, z.bytes);
    z.finish()?;
    Ok((files, bytes))
}

/// Extract `archive` into `dest` (created). Returns the number of files written.
pub fn extract(archive: &Path, dest: &Path) -> Result<usize> {
    let f = fs::File::open(archive).map_err(|e| Error::io(archive, e))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| zerr(archive, e))?;
    if zip.len() > MAX_ENTRIES {
        return Err(Error::invalid("archive has too many entries"));
    }
    fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    let mut total = 0u64;
    let mut written = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| zerr(archive, e))?;
        let Some(rel) = entry.enclosed_name() else {
            return Err(Error::invalid(format!(
                "unsafe path in archive: {}",
                entry.name()
            )));
        };
        if entry.is_symlink() {
            continue; // never materialize links from archives
        }
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        if rel_s.split('/').any(|c| c == "__MACOSX") {
            continue;
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| Error::io(&out, e))?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| Error::io(p, e))?;
        }
        let mut file = fs::File::create(&out).map_err(|e| Error::io(&out, e))?;
        let budget = MAX_UNPACKED.saturating_sub(total);
        let n = std::io::copy(&mut (&mut entry).take(budget + 1), &mut file)
            .map_err(|e| Error::io(&out, e))?;
        total += n;
        if total > MAX_UNPACKED {
            return Err(Error::invalid("archive is too large once unpacked"));
        }
        written += 1;
    }
    Ok(written)
}

/// Find the board root inside an extracted folder (itself or one level down).
pub fn find_board_root(dir: &Path) -> Option<PathBuf> {
    if crate::store::is_board(dir) {
        return Some(dir.to_path_buf());
    }
    let rd = fs::read_dir(dir).ok()?;
    let subs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    subs.into_iter().find(|p| crate::store::is_board(p))
}

/// Peek into an archive without extracting: does it hold a Lull board?
pub fn archive_has_board(archive: &Path) -> Result<bool> {
    let f = fs::File::open(archive).map_err(|e| Error::io(archive, e))?;
    let zip = zip::ZipArchive::new(f).map_err(|e| zerr(archive, e))?;
    let marker = format!("{MARKER_DIR}/{}", crate::brand::BOARD_FILE);
    Ok(zip
        .file_names()
        .any(|n| n == marker || n.split_once('/').is_some_and(|(_, r)| r == marker)))
}

/// Read a small file's text (used for `.md` / `.json` sources), capped at `max` bytes.
pub fn read_capped(path: &Path, max: u64) -> Result<String> {
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.len() > max {
        return Err(Error::invalid(format!(
            "file too large ({} bytes, max {max})",
            meta.len()
        )));
    }
    crate::fsutil::read_to_string(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_entries_filter() {
        assert!(board_entry_kept(".lull/board.json", false));
        assert!(!board_entry_kept(".lull/cache/thumbs/x", true));
        assert!(!board_entry_kept(".lull/trash/t1/x.md", true));
        assert!(!board_entry_kept(".lull/history/2026.jsonl", false));
        assert!(board_entry_kept(".lull/history/2026.jsonl", true));
        assert!(!board_entry_kept(".git/HEAD", true));
        assert!(board_entry_kept("k123456/c123456.md", false));
    }

    #[test]
    fn zip_and_extract_roundtrip_and_rejects_traversal() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("B");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/a.txt"), "hello").unwrap();
        let dest = d.path().join("out.zip");
        let mut z = ZipOut::create(&dest).unwrap();
        z.add_dir(&src, "B/", &|_| true).unwrap();
        z.finish().unwrap();
        let x = d.path().join("x");
        assert_eq!(extract(&dest, &x).unwrap(), 1);
        assert_eq!(fs::read_to_string(x.join("B/sub/a.txt")).unwrap(), "hello");

        // A crafted archive with `../` must be refused.
        let evil = d.path().join("evil.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&evil).unwrap());
        w.start_file("../escape.txt", SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        assert!(extract(&evil, &d.path().join("y")).is_err());
        assert!(!d.path().join("escape.txt").exists());
    }
}
