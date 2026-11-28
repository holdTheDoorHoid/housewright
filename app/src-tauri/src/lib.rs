//! The Tauri shell: one `api` command over `housewright_api::Api`, plus the
//! reminders plugin. On Android the same core is also reachable from Kotlin
//! through JNI (`android.rs`), so a notification's Done button records the
//! completion through exactly the same code, even when no window is open.
//!
//! Licensed AGPL-3.0-only.

mod core;

#[cfg(target_os = "android")]
mod android;

use housewright_api::ApiError;
use serde_json::Value;
use tauri::Manager;

#[tauri::command]
async fn api(app: tauri::AppHandle, cmd: String, args: Value) -> Result<Value, ApiError> {
    let path = core::db_path().or_else(|| {
        app.path()
            .app_data_dir()
            .ok()
            .map(|d| d.join(core::DB_FILE))
    });
    let path = path.ok_or_else(|| ApiError::internal("no place to store the database"))?;
    tauri::async_runtime::spawn_blocking(move || core::call(&path, &cmd, args))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("housewright"),
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_reminders::init())
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("error while running Housewright");
}
