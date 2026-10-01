//! Import / export RPC methods.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lull_core::app::Core;
use lull_core::io::service::{ExportFormat, ImportRequest};
use lull_core::io::templates::BoardTemplate;
use serde_json::Value;

use crate::rpc::{R, RpcError, arg, opt};

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
    core: &Arc<Core>,
    _window: &str,
    method: &str,
    p: &Value,
) -> Option<R> {
    let run = || -> R {
        match method {
            // Export a board, or one card (`card`), to `dest`.
            "io.export" => {
                let board: String = arg(p, "board")?;
                let card: Option<String> = opt(p, "card")?;
                let format: ExportFormat = arg(p, "format")?;
                let dest: PathBuf = arg(p, "dest")?;
                let history: bool = opt(p, "includeHistory")?.unwrap_or(false);
                ok(core.io_export(&board, card.as_deref(), format, &dest, history)?)
            }
            // HTML for printing to PDF through the system print dialog.
            "io.renderHtml" => {
                let board: String = arg(p, "board")?;
                let card: Option<String> = opt(p, "card")?;
                ok(core.io_render_html(&board, card.as_deref())?)
            }
            "io.inspect" => {
                let path: PathBuf = arg(p, "path")?;
                ok(core.io_inspect(&path)?)
            }
            "io.import" => {
                let req: ImportRequest =
                    serde_json::from_value(p.clone()).map_err(|e| RpcError {
                        code: "invalid".into(),
                        message: format!("params: {e}"),
                    })?;
                ok(core.io_import(&req)?)
            }
            "templates.list" => {
                let board: Option<String> = opt(p, "board")?;
                ok(core.card_templates(board.as_deref()))
            }
            "board.createFromTemplate" => {
                let path: PathBuf = arg(p, "path")?;
                let name: String = opt(p, "name")?.unwrap_or_default();
                let tpl: BoardTemplate = arg(p, "template")?;
                let vcs: bool = opt(p, "git")?.unwrap_or(false);
                ok(core.create_from_template(&path, &name, &tpl, vcs)?)
            }
            "board.upgrade" => {
                let board: String = arg(p, "board")?;
                let (report, snapshot) = core.upgrade_board_schema(&board)?;
                ok(serde_json::json!({ "report": report, "snapshot": snapshot }))
            }
            "settings.export" => {
                let path: PathBuf = arg(p, "path")?;
                let bundle: Value = arg(p, "bundle")?;
                ok(core.settings_export(&path, bundle)?)
            }
            "settings.import" => {
                let path: PathBuf = arg(p, "path")?;
                ok(core.settings_import(&path)?)
            }
            _ => unreachable!(),
        }
    };
    match method {
        "io.export"
        | "io.renderHtml"
        | "io.inspect"
        | "io.import"
        | "templates.list"
        | "board.createFromTemplate"
        | "board.upgrade"
        | "settings.export"
        | "settings.import" => Some(run()),
        _ => None,
    }
}

fn ok<T: serde::Serialize>(v: T) -> R {
    serde_json::to_value(v).map_err(|e| RpcError {
        code: "invalid".into(),
        message: e.to_string(),
    })
}
