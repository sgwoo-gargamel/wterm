use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::Result;

const FILE_NAME: &str = "wterm-settings.json";

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
}

/// Settings live next to the executable; AppData is only a fallback when the
/// executable path cannot be resolved
fn settings_path(app: &AppHandle) -> PathBuf {
    if let Some(dir) = exe_dir() {
        return dir.join(FILE_NAME);
    }
    let dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    dir.join(FILE_NAME)
}

#[tauri::command]
pub fn settings_location(app: AppHandle) -> String {
    settings_path(&app).to_string_lossy().to_string()
}

/// Whole settings document as JSON text; empty string when nothing is stored yet
#[tauri::command]
pub fn load_settings(app: AppHandle) -> String {
    std::fs::read_to_string(settings_path(&app)).unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, json: String) -> Result<()> {
    let path = settings_path(&app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

/// Window geometry remembered between runs, in physical pixels — the same units
/// the webview reports and the OS takes back
#[derive(serde::Deserialize)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    #[serde(default)]
    maximized: bool,
}

fn stored_window_state(app: &AppHandle) -> Option<WindowState> {
    let text = std::fs::read_to_string(settings_path(app)).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    serde_json::from_value(doc.get("window")?.clone()).ok()
}

/// True when the saved rectangle still overlaps a connected monitor by enough to
/// grab; a window saved on a monitor that is now unplugged would otherwise come
/// back off-screen
fn on_some_monitor(window: &tauri::WebviewWindow, s: &WindowState) -> bool {
    let (l, t) = (s.x, s.y);
    let (r, b) = (s.x + s.width as i32, s.y + s.height as i32);
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let p = m.position();
            let size = m.size();
            let (ml, mt) = (p.x, p.y);
            let (mr, mb) = (p.x + size.width as i32, p.y + size.height as i32);
            let overlap_w = r.min(mr) - l.max(ml);
            let overlap_h = b.min(mb) - t.max(mt);
            overlap_w >= 120 && overlap_h >= 40
        })
}

/// Puts the window back where it was left. Called before the window is shown, so
/// a restored position never flashes at the default one first.
pub fn restore_window(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Some(state) = stored_window_state(app) else {
        return;
    };
    if state.width < 400 || state.height < 300 || !on_some_monitor(window, &state) {
        return;
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
    let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
    if state.maximized {
        let _ = window.maximize();
    }
}
