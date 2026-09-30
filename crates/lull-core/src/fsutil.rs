//! File-system helpers: atomic writes, hashing, safe moves.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::ids::random_suffix;

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Short content hash used for change detection (first 16 hex chars of sha256).
pub fn short_hash(bytes: &[u8]) -> String {
    let mut h = sha256_hex(bytes);
    h.truncate(16);
    h
}

/// Write via temp file + fsync + rename so readers never see partial files.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::invalid(format!("no parent: {}", path.display())))?;
    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{name}.tmp-{}", random_suffix(6)));
    {
        let mut f = fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        f.write_all(bytes).map_err(|e| Error::io(&tmp, e))?;
        f.sync_all().map_err(|e| Error::io(&tmp, e))?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }
    #[cfg(unix)]
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

pub fn read_to_string(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// Rename, falling back to copy + delete across devices.
pub fn move_path(from: &Path, to: &Path) -> Result<()> {
    if let Some(p) = to.parent() {
        fs::create_dir_all(p).map_err(|e| Error::io(p, e))?;
    }
    if to.exists() {
        return Err(Error::Conflict(format!("destination exists: {}", to.display())));
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_recursive(from, to)?;
            remove_path(from)
        }
    }
}

pub fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(from).map_err(|e| Error::io(from, e))?;
    if meta.is_dir() {
        fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
        for entry in fs::read_dir(from).map_err(|e| Error::io(from, e))? {
            let entry = entry.map_err(|e| Error::io(from, e))?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else if meta.file_type().is_symlink() {
        Ok(()) // never follow symlinks
    } else {
        fs::copy(from, to).map_err(|e| Error::io(to, e))?;
        Ok(())
    }
}

pub fn remove_path(p: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(Error::io(p, e)),
    };
    if meta.is_dir() {
        fs::remove_dir_all(p).map_err(|e| Error::io(p, e))
    } else {
        fs::remove_file(p).map_err(|e| Error::io(p, e))
    }
}

/// Resolve `rel` under `root`, rejecting absolute paths and `..` traversal.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.replace('\\', "/");
    let mut out = root.to_path_buf();
    for comp in Path::new(&rel).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return Err(Error::invalid(format!("unsafe path: {rel}"))),
        }
    }
    Ok(out)
}

/// Sanitize a user file name into a safe, short, portable fragment.
pub fn sanitize_name(name: &str, max: usize) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-');
    s.chars().take(max).collect::<String>().trim_matches('-').to_string()
}

pub fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_and_read() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a/b.txt");
        atomic_write(&p, b"hello").unwrap();
        atomic_write(&p, b"world").unwrap();
        assert_eq!(read_to_string(&p).unwrap(), "world");
        let leftovers: Vec<_> = fs::read_dir(d.path().join("a")).unwrap().collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let root = Path::new("/r");
        assert!(safe_join(root, "../x").is_err());
        assert!(safe_join(root, "/etc/passwd").is_err());
        assert_eq!(safe_join(root, "a/./b.png").unwrap(), PathBuf::from("/r/a/b.png"));
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize_name("My Report (final).PDF", 20), "my-report-final-pdf");
        assert_eq!(sanitize_name("../../etc", 20), "etc");
        assert_eq!(sanitize_name("ñandú", 3), "ñan");
    }
}
