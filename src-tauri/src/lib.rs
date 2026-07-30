mod commands;
mod error;
mod session;
mod settings;

use session::SessionManager;
use tauri::Manager;

/// `wterm v0.1.0 (eacd377)` — the revision is baked in by build.rs
fn window_title() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let rev = env!("WTERM_GIT_REV");
    if rev.is_empty() {
        format!("wterm v{version}")
    } else {
        format!("wterm v{version} ({rev})")
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
