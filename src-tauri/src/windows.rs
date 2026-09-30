//! Window creation with per-platform chrome.

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

static COUNTER: AtomicU32 = AtomicU32::new(1);

pub fn next_label() -> String {
    format!("w{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Create a new app window. `query` is appended to the URL (e.g. `?board=b123`).
pub fn create(app: &AppHandle, label: &str, query: &str) -> tauri::Result<WebviewWindow> {
    let url = WebviewUrl::App(format!("index.html{query}").into());
    #[allow(unused_mut)]
    let mut b = WebviewWindowBuilder::new(app, label, url)
        .title("Lull")
        .inner_size(1280.0, 820.0)
        .min_inner_size(640.0, 420.0)
        .visible(false)
        .accept_first_mouse(true);

    #[cfg(target_os = "macos")]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::{Effect, EffectState};
        b = b
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 20.0))
            .transparent(true)
            .effects(WindowEffectsConfig {
                // Liquid Glass on macOS 26+, falls back to the sidebar material.
                effects: vec![Effect::LiquidGlassRegular, Effect::Sidebar],
                state: Some(EffectState::FollowsWindowActiveState),
                radius: None,
                color: None,
                interactive: false,
            });
    }
    #[cfg(target_os = "windows")]
    {
        use tauri::utils::config::WindowEffectsConfig;
        use tauri::window::Effect;
        b = b
            .decorations(false)
            .transparent(true)
            .effects(WindowEffectsConfig {
                effects: vec![Effect::Mica],
                state: None,
                radius: None,
                color: None,
                interactive: false,
            });
    }
    let w = b.build()?;
    // Safety net: the UI shows the window once mounted; if it failed to boot,
    // show it anyway so the app is never invisible.
    let fallback = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        if !fallback.is_visible().unwrap_or(true) {
            let _ = fallback.show();
            let _ = fallback.set_focus();
        }
    });
    Ok(w)
}

pub fn focus_any(app: &AppHandle) {
    if let Some((_, w)) = app.webview_windows().into_iter().next() {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    } else {
        let label = next_label();
        let _ = create(app, &label, "");
    }
}
