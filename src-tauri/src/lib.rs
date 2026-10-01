//! Lull desktop shell: wires the framework-free `lull_core::app::Core` to
//! Tauri (windows, menus, protocol, events, plugins).

mod ai_rpc;
mod exports;
mod fonts;
mod integrations_rpc;
mod menu;
mod protocol;
mod rpc;
mod windows;

use std::sync::Arc;

use lull_core::app::{AppPaths, Core, CoreEvent, EventSink};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

pub struct AppState {
    pub core: Arc<Core>,
}

struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, event: CoreEvent) {
        let _ = self.0.emit("lull://event", &event);
    }
}

fn init_logging(dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};
    let _ = std::fs::create_dir_all(dir);
    let file = tracing_appender::rolling::daily(dir, "lull.log");
    let (nb, guard) = tracing_appender::non_blocking(file);
    let filter = EnvFilter::try_from_env("LULL_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(nb).with_ansi(false));
    if cfg!(debug_assertions) {
        let _ = registry
            .with(fmt::layer().with_writer(std::io::stderr))
            .try_init();
    } else {
        let _ = registry.try_init();
    }
    Some(guard)
}

fn run_in_background(core: &Core) -> bool {
    core.settings()
        .get("app.runInBackground")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                windows::focus_any(app)
            }))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--background"]),
            ))
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol("lull", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let core = app.state::<AppState>().core.clone();
                responder.respond(protocol::handle(&core, &request));
            });
        })
        .invoke_handler(tauri::generate_handler![rpc::rpc])
        .setup(|app| {
            let handle = app.handle().clone();
            let base = app.path().app_data_dir()?;
            let paths = AppPaths {
                data: base.join("data"),
                config: app.path().app_config_dir()?.join("config"),
                logs: app.path().app_log_dir()?,
            };
            let guard = init_logging(&paths.logs);
            app.manage(guard);
            tracing::info!("starting Lull {}", env!("CARGO_PKG_VERSION"));
            let core = Core::new(paths, Arc::new(TauriSink(handle.clone()))).map_err(|e| {
                Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error>
            })?;
            app.manage(AppState { core: core.clone() });
            ai_rpc::start_scheduler(&handle, core.clone());

            let menu = menu::build(&handle)?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, ev| menu::on_event(app, ev.id().as_ref()));

            let background_launch = std::env::args().any(|a| a == "--background");
            if !background_launch {
                windows::create(&handle, "main", "")?;
            }
            // Discover boards shortly after start (never blocks the UI).
            let c = core.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                c.rescan();
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event
                && let Some(state) = window.try_state::<AppState>()
            {
                state.core.release_window(window.label());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Lull")
        .run(|app, event| match event {
            RunEvent::ExitRequested { api, code, .. } => {
                let state = app.state::<AppState>();
                if code.is_none() && run_in_background(&state.core) {
                    api.prevent_exit();
                } else {
                    state.core.shutdown();
                }
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows,
                ..
            } if !has_visible_windows => {
                windows::focus_any(app);
            }
            _ => {}
        });
}
