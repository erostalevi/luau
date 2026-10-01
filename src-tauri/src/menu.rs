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

/// Menu label in the app language (`en`, `es`, `pt`); English is the key.
fn tr(lang: &str, en: &'static str) -> &'static str {
    let i = match lang {
        "es" => 0,
        "pt" => 1,
        _ => return en,
    };
    LABELS
        .iter()
        .find(|(k, _)| *k == en)
        .map(|(_, v)| v[i])
        .unwrap_or(en)
}

const LABELS: &[(&str, [&str; 2])] = &[
    ("Settings…", ["Ajustes…", "Configurações…"]),
    (
        "Check for Updates…",
        ["Buscar actualizaciones…", "Procurar atualizações…"],
    ),
    ("New Card", ["Nueva tarjeta", "Novo cartão"]),
    ("New Board…", ["Nuevo tablero…", "Novo quadro…"]),
    ("New Window", ["Nueva ventana", "Nova janela"]),
    ("Open Board…", ["Abrir tablero…", "Abrir quadro…"]),
    ("Import…", ["Importar…", "Importar…"]),
    ("Export…", ["Exportar…", "Exportar…"]),
    ("Close Tab", ["Cerrar pestaña", "Fechar aba"]),
    (
        "Find in Board",
        ["Buscar en el tablero", "Buscar no quadro"],
    ),
    ("Search Everywhere", ["Buscar en todo", "Pesquisar em tudo"]),
    (
        "Command Palette…",
        ["Paleta de comandos…", "Paleta de comandos…"],
    ),
    ("Quick Open…", ["Apertura rápida…", "Abertura rápida…"]),
    (
        "Toggle Sidebar",
        [
            "Mostrar u ocultar barra lateral",
            "Mostrar ou ocultar barra lateral",
        ],
    ),
    ("Explorer", ["Explorador", "Explorador"]),
    ("Search", ["Buscar", "Pesquisar"]),
    ("History", ["Historial", "Histórico"]),
    ("Integrations", ["Integraciones", "Integrações"]),
    ("Extensions", ["Extensiones", "Extensões"]),
    (
        "Toggle Rows / Columns",
        ["Alternar filas / columnas", "Alternar linhas / colunas"],
    ),
    (
        "Toggle Show Archived",
        [
            "Mostrar u ocultar archivadas",
            "Mostrar ou ocultar arquivados",
        ],
    ),
    ("Zoom In", ["Acercar", "Aumentar zoom"]),
    ("Zoom Out", ["Alejar", "Diminuir zoom"]),
    ("Actual Size", ["Tamaño real", "Tamanho real"]),
    ("Split Right", ["Dividir a la derecha", "Dividir à direita"]),
    ("Next Tab", ["Pestaña siguiente", "Próxima aba"]),
    ("Previous Tab", ["Pestaña anterior", "Aba anterior"]),
    (
        "Keyboard Shortcuts",
        ["Atajos de teclado", "Atalhos de teclado"],
    ),
    ("Welcome", ["Bienvenida", "Boas-vindas"]),
    (
        "Open Logs Folder",
        ["Abrir carpeta de registros", "Abrir pasta de registros"],
    ),
    (
        "Export Diagnostics…",
        ["Exportar diagnóstico…", "Exportar diagnóstico…"],
    ),
    ("File", ["Archivo", "Arquivo"]),
    ("Edit", ["Edición", "Editar"]),
    ("View", ["Ver", "Visualizar"]),
    ("Window", ["Ventana", "Janela"]),
    ("Help", ["Ayuda", "Ajuda"]),
    ("About Luau", ["Acerca de Luau", "Sobre o Luau"]),
];

pub fn build<R: Runtime>(app: &AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
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
            &PredefinedMenuItem::about(app, Some(tr(lang, "About Luau")), Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &item(
                app,
                "app.openSettings",
                tr(lang, "Settings…"),
                Some("CmdOrCtrl+,"),
            )?,
            &item(
                app,
                "app.checkForUpdates",
                tr(lang, "Check for Updates…"),
                None,
            )?,
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
        tr(lang, "File"),
        true,
        &[
            &item(app, "card.new", tr(lang, "New Card"), None)?,
            &item(app, "board.new", tr(lang, "New Board…"), None)?,
            &item(app, "window.new", tr(lang, "New Window"), None)?,
            &item(app, "board.open", tr(lang, "Open Board…"), None)?,
            &item(app, "board.import", tr(lang, "Import…"), None)?,
            &item(app, "board.export", tr(lang, "Export…"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "tab.close", tr(lang, "Close Tab"), None)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        tr(lang, "Edit"),
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
            &item(app, "board.filter", tr(lang, "Find in Board"), None)?,
            &item(app, "search.focus", tr(lang, "Search Everywhere"), None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        tr(lang, "View"),
        true,
        &[
            &item(app, "palette.commands", tr(lang, "Command Palette…"), None)?,
            &item(app, "palette.quickOpen", tr(lang, "Quick Open…"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "panel.toggle", tr(lang, "Toggle Sidebar"), None)?,
            &item(app, "panel.explorer", tr(lang, "Explorer"), None)?,
            &item(app, "panel.search", tr(lang, "Search"), None)?,
            &item(app, "panel.history", tr(lang, "History"), None)?,
            &item(app, "panel.integrations", tr(lang, "Integrations"), None)?,
            &item(app, "panel.extensions", tr(lang, "Extensions"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(
                app,
                "board.toggleOrientation",
                tr(lang, "Toggle Rows / Columns"),
                None,
            )?,
            &item(
                app,
                "board.toggleArchived",
                tr(lang, "Toggle Show Archived"),
                None,
            )?,
            &item(app, "view.zoomIn", tr(lang, "Zoom In"), None)?,
            &item(app, "view.zoomOut", tr(lang, "Zoom Out"), None)?,
            &item(app, "view.zoomReset", tr(lang, "Actual Size"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        tr(lang, "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(app, "view.splitRight", tr(lang, "Split Right"), None)?,
            &item(app, "tab.next", tr(lang, "Next Tab"), None)?,
            &item(app, "tab.previous", tr(lang, "Previous Tab"), None)?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        tr(lang, "Help"),
        true,
        &[
            &item(
                app,
                "app.openKeybindings",
                tr(lang, "Keyboard Shortcuts"),
                None,
            )?,
            &item(app, "app.showWelcome", tr(lang, "Welcome"), None)?,
            &item(app, "app.openLogs", tr(lang, "Open Logs Folder"), None)?,
            &item(
                app,
                "app.exportDiagnostics",
                tr(lang, "Export Diagnostics…"),
                None,
            )?,
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
        // No window is focused (e.g. a menu click right after alt-tab): use
        // the last focused one instead of running the command in every window.
        None => {
            let last = crate::windows::last_focused();
            let label = last
                .filter(|l| app.get_webview_window(l).is_some())
                .or_else(|| app.webview_windows().into_keys().next());
            if let Some(l) = label {
                let _ = app.emit_to(l.as_str(), "luau://menu", id);
            }
        }
    }
}
