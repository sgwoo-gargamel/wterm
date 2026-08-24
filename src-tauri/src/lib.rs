mod commands;
mod error;
mod session;
mod settings;

use session::SessionManager;
use tauri::Manager;

/// `v0.1.0 (eacd377)` — the revision is baked in by build.rs
fn version_label() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let rev = env!("WTERM_GIT_REV");
    if rev.is_empty() {
        format!("v{version}")
    } else {
        format!("v{version} ({rev})")
    }
}

/// `wterm v0.1.0 (eacd377)`
fn window_title() -> String {
    format!("wterm {}", version_label())
}

#[derive(serde::Serialize)]
struct VersionInfo {
    version: &'static str,
    /// Short git revision from build.rs, `+` suffixed when built from a dirty tree; may be empty
    rev: &'static str,
}

/// The toolbar's "?" pop-up shows this since the window has no native title bar to carry it
#[tauri::command]
fn app_version() -> VersionInfo {
    VersionInfo {
        version: env!("CARGO_PKG_VERSION"),
        rev: env!("WTERM_GIT_REV"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&window_title());
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SessionManager::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            commands::list_serial_ports,
            commands::list_fonts,
            commands::list_shells,
            commands::session_open,
            commands::session_write,
            commands::session_resize,
            commands::session_close,
            commands::session_start_log,
            commands::session_stop_log,
            settings::load_settings,
            settings::save_settings,
            settings::settings_location,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
