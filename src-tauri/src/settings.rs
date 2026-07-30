use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::Result;

const FILE_NAME: &str = "wterm-settings.json";
/// Presence of this file next to the executable switches on portable mode
const PORTABLE_MARKER: &str = "portable.txt";

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
}

/// Settings live next to the executable in portable mode, otherwise in AppData
fn settings_path(app: &AppHandle) -> PathBuf {
    if let Some(dir) = exe_dir() {
        if dir.join(PORTABLE_MARKER).exists() {
            return dir.join(FILE_NAME);
        }
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
