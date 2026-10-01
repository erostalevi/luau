//! Native menus. Custom items emit `luau://menu` with a command id that the
//! frontend command registry executes; standard roles keep native clipboard,
//! undo and window behaviour working (required on macOS).

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

fn item<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    label: &str,
    accel: Option<&str>,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, label, true, accel)
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadata {
        name: Some("Luau".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        ..Default::default()
    };
    let app_menu = Submenu::with_items(
        app,
        "Luau",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Luau"), Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "app.openSettings", "Settings…", Some("CmdOrCtrl+,"))?,
            &item(app, "app.checkForUpdates", "Check for Updates…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    // Accelerators for app commands are handled by the in-app keybinding
    // resolver (so users can remap them); menus show them without binding.
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &item(app, "card.new", "New Card", None)?,
            &item(app, "board.new", "New Board…", None)?,
            &item(app, "window.new", "New Window", None)?,
            &item(app, "board.open", "Open Board…", None)?,
            &item(app, "board.import", "Import…", None)?,
            &item(app, "board.export", "Export…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "tab.close", "Close Tab", None)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "board.filter", "Find in Board", None)?,
            &item(app, "search.focus", "Search Everywhere", None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &item(app, "palette.commands", "Command Palette…", None)?,
            &item(app, "palette.quickOpen", "Quick Open…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "panel.toggle", "Toggle Sidebar", None)?,
            &item(app, "panel.explorer", "Explorer", None)?,
            &item(app, "panel.search", "Search", None)?,
            &item(app, "panel.history", "History", None)?,
            &item(app, "panel.integrations", "Integrations", None)?,
            &item(app, "panel.extensions", "Extensions", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(
                app,
                "board.toggleOrientation",
                "Toggle Rows / Columns",
                None,
            )?,
            &item(app, "board.toggleArchived", "Toggle Show Archived", None)?,
            &item(app, "view.zoomIn", "Zoom In", None)?,
            &item(app, "view.zoomOut", "Zoom Out", None)?,
            &item(app, "view.zoomReset", "Actual Size", None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "view.splitRight", "Split Right", None)?,
            &item(app, "tab.next", "Next Tab", None)?,
            &item(app, "tab.previous", "Previous Tab", None)?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &item(app, "app.openKeybindings", "Keyboard Shortcuts", None)?,
            &item(app, "app.showWelcome", "Welcome", None)?,
            &item(app, "app.openLogs", "Open Logs Folder", None)?,
            &item(app, "app.exportDiagnostics", "Export Diagnostics…", None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window, &help])
}

pub fn on_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    // Deliver to the focused window only.
    let target = app
        .webview_windows()
        .into_iter()
        .find(|(_, w)| w.is_focused().unwrap_or(false))
        .map(|(l, _)| l);
    match target {
        Some(label) => {
            let _ = app.emit_to(label.as_str(), "luau://menu", id);
        }
        None => {
            let _ = app.emit("luau://menu", id);
        }
    }
}
