//! Import / export RPC methods.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use lull_core::app::Core;
use serde_json::Value;

use crate::rpc::R;

/// Zip a directory recursively (used for logs and board exports).
pub fn zip_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    zip_dir_filtered(src, dest, &|_| true)
}

pub fn zip_dir_filtered(
    src: &Path,
    dest: &Path,
    keep: &dyn Fn(&Path) -> bool,
) -> std::io::Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let base = src.parent().unwrap_or(src);
    for entry in walkdir(src) {
        let rel = entry.strip_prefix(base).unwrap_or(&entry);
        if !keep(rel) {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        if entry.is_dir() {
            zip.add_directory(format!("{name}/"), opts)?;
        } else {
            zip.start_file(name, opts)?;
            zip.write_all(&std::fs::read(&entry)?)?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn walkdir(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&p) {
            for e in rd.flatten() {
                let path = e.path();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    stack.push(path.clone());
                }
                out.push(path);
            }
        }
    }
    out
}

/// Import/export, templates and board maintenance RPC methods.
/// Returns `None` for methods this module does not handle.
pub fn dispatch(
    _app: &tauri::AppHandle,
    _core: &Arc<Core>,
    _window: &str,
    _method: &str,
    _p: &Value,
) -> Option<R> {
    None
}
