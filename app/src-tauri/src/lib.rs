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

fn db_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, ApiError> {
    core::db_path()
        .or_else(|| {
            app.path()
                .app_data_dir()
                .ok()
                .map(|d| d.join(core::DB_FILE))
        })
        .ok_or_else(|| ApiError::internal("no place to store the database"))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
}

#[tauri::command]
async fn api(app: tauri::AppHandle, cmd: String, args: Value) -> Result<Value, ApiError> {
    let path = db_path(&app)?;
    blocking(move || core::call(&path, &cmd, args)).await
}

/// A command with a file: the bytes are the raw request body; the command
/// name and JSON arguments travel in headers.
#[tauri::command]
async fn api_upload(
    app: tauri::AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<Value, ApiError> {
    let path = db_path(&app)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(ApiError::invalid("expected a file"));
    };
    let bytes = bytes.clone();
    let header = |k: &str| {
        request
            .headers()
            .get(k)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let cmd = header("x-cmd").ok_or_else(|| ApiError::invalid("no command"))?;
    let args_text = header("x-args").unwrap_or_default();
    let args_text = percent_decode(&args_text);
    let args: Value = if args_text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&args_text).map_err(|e| ApiError::invalid(e.to_string()))?
    };
    blocking(move || core::with_api(&path, |api| api.call_upload(&cmd, args, &bytes))).await
}

/// A command that returns bytes, as a raw (not JSON) response.
#[tauri::command]
async fn api_download(
    app: tauri::AppHandle,
    cmd: String,
    args: Value,
) -> Result<tauri::ipc::Response, ApiError> {
    let path = db_path(&app)?;
    let bytes =
        blocking(move || core::with_api(&path, |api| api.call_download(&cmd, args))).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Write a command's bytes to a file in the app's cache folder and return
/// its path, so the phone's own viewer or share sheet can open it (a PDF
/// manual, a sticker sheet, a backup).
#[tauri::command]
async fn save_for_viewer(
    app: tauri::AppHandle,
    cmd: String,
    args: Value,
    filename: String,
) -> Result<String, ApiError> {
    let path = db_path(&app)?;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| ApiError::internal(e.to_string()))?
        .join("shared");
    let safe: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || ".-_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    blocking(move || {
        let bytes = core::with_api(&path, |api| api.call_download(&cmd, args))?;
        // Only the latest few shared files are kept.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| ApiError::internal(e.to_string()))?;
        let out = dir.join(if safe.is_empty() { "file".into() } else { safe });
        std::fs::write(&out, bytes).map_err(|e| ApiError::internal(e.to_string()))?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
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
        .invoke_handler(tauri::generate_handler![
            api,
            api_upload,
            api_download,
            save_for_viewer
        ])
        .run(tauri::generate_context!())
        .expect("error while running Housewright");
}
