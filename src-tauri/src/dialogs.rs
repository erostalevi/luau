//! Native file dialogs opened by the backend. Whatever the user picks is
//! recorded in `grants`, so later RPCs can use exactly those locations.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};

use crate::grants;
use crate::rpc::{R, opt};

#[derive(Deserialize)]
struct Filter {
    name: String,
    extensions: Vec<String>,
}

fn builder(
    app: &AppHandle,
    window: &str,
    p: &Value,
) -> Result<FileDialogBuilder<tauri::Wry>, crate::rpc::RpcError> {
    let mut b = app.dialog().file();
    if let Some(w) = app.get_webview_window(window) {
        b = b.set_parent(&w);
    }
    if let Some(t) = opt::<String>(p, "title")? {
        b = b.set_title(t);
    }
    for f in opt::<Vec<Filter>>(p, "filters")?.unwrap_or_default() {
        let exts: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
        b = b.add_filter(f.name, &exts);
    }
    if let Some(d) = opt::<String>(p, "defaultPath")?.filter(|d| !d.is_empty()) {
        let dp = PathBuf::from(&d);
        if dp.is_dir() {
            b = b.set_directory(dp);
        } else {
            if let Some(parent) = dp.parent().filter(|x| x.is_dir()) {
                b = b.set_directory(parent);
            }
            if let Some(name) = dp.file_name() {
                b = b.set_file_name(name.to_string_lossy());
            }
        }
    }
    Ok(b)
}

fn to_path(f: tauri_plugin_dialog::FilePath) -> Option<PathBuf> {
    f.into_path().ok()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn dispatch_sync(app: &AppHandle, window: &str, method: &str, p: &Value) -> Option<R> {
    let run = || -> R {
        match method {
            "dialog.pickFolder" => {
                let picked = builder(app, window, p)?
                    .blocking_pick_folder()
                    .and_then(to_path);
                if let Some(d) = &picked {
                    grants::grant(d, true);
                }
                Ok(json!(picked.map(|d| s(&d))))
            }
            "dialog.pickFiles" => {
                let multiple: bool = opt(p, "multiple")?.unwrap_or(false);
                let b = builder(app, window, p)?;
                let picked: Vec<PathBuf> = if multiple {
                    b.blocking_pick_files()
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(to_path)
                        .collect()
                } else {
                    b.blocking_pick_file()
                        .and_then(to_path)
                        .into_iter()
                        .collect()
                };
                for f in &picked {
                    grants::grant(f, false);
                }
                Ok(if picked.is_empty() {
                    Value::Null
                } else {
                    json!(picked.iter().map(|f| s(f)).collect::<Vec<_>>())
                })
            }
            "dialog.save" => {
                let picked = builder(app, window, p)?
                    .blocking_save_file()
                    .and_then(to_path);
                if let Some(f) = &picked {
                    grants::grant(f, false);
                }
                Ok(json!(picked.map(|f| s(&f))))
            }
            _ => unreachable!(),
        }
    };
    match method {
        "dialog.pickFolder" | "dialog.pickFiles" | "dialog.save" => Some(run()),
        _ => None,
    }
}
