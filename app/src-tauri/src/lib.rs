//! The Tauri shell: one `api` command over `housewright_api::Api`, plus the
//! reminders plugin. On Android the same core is also reachable from Kotlin
//! through JNI (`android.rs`), so a notification's Done button records the
//! completion through exactly the same code, even when no window is open.
//!
//! Licensed AGPL-3.0-only.

mod core;

#[cfg(target_os = "android")]
mod android;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
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

/// A command with a file. The bytes travel as base64 text: on Android the
/// WebView cannot pass a raw request body to the app (requests go through
/// a JSON message bridge), so raw IPC bodies do not arrive there.
#[tauri::command]
async fn api_upload(
    app: tauri::AppHandle,
    cmd: String,
    args: Value,
    data: String,
) -> Result<Value, ApiError> {
    let path = db_path(&app)?;
    blocking(move || {
        let bytes = B64
            .decode(data.as_bytes())
            .map_err(|e| ApiError::invalid(format!("the file did not arrive whole: {e}")))?;
        core::with_api(&path, |api| api.call_upload(&cmd, args, &bytes))
    })
    .await
}

/// A command that returns bytes, as base64 text (see `api_upload`).
#[tauri::command]
async fn api_download(app: tauri::AppHandle, cmd: String, args: Value) -> Result<String, ApiError> {
    let path = db_path(&app)?;
    let bytes =
        blocking(move || core::with_api(&path, |api| api.call_download(&cmd, args))).await?;
    Ok(B64.encode(bytes))
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

/// Downloads straight from the manufacturer, one hop at a time, so the core
/// can check every redirect (D-151). No cookies are kept, no referrer is
/// sent, and the user agent names only the app.
struct ManualFetch(ureq::Agent);

impl ManualFetch {
    fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(120)))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent("Housewright (saving an appliance manual)")
            .build()
            .into();
        Self(agent)
    }
}

impl housewright_api::library::Fetch for ManualFetch {
    fn get(&mut self, url: &str, limit: usize) -> Result<housewright_api::library::Fetched, String> {
        let mut res = self
            .0
            .get(url)
            .header("Accept", "application/pdf")
            .call()
            .map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();
        let body = if status == 200 {
            res.body_mut()
                .with_config()
                .limit(limit as u64 + 1)
                .read_to_vec()
                .map_err(|e| format!("the download stopped ({e})"))?
        } else {
            Vec::new()
        };
        Ok((status, headers, body))
    }
}

/// "Save the official manual to this thing" (D-151): check the link
/// against the thing's model family, download it from the manufacturer
/// onto this device, and keep it as the thing's manual. The download runs
/// outside the database lock so the app stays responsive.
#[tauri::command]
async fn save_manual(
    app: tauri::AppHandle,
    item_id: String,
    url: String,
) -> Result<Value, ApiError> {
    let path = db_path(&app)?;
    blocking(move || {
        let args = serde_json::json!({ "item_id": item_id, "url": url });
        let src: housewright_api::library::ManualSource =
            serde_json::from_value(core::call(&path, "manual_source", args.clone())?)
                .map_err(|e| ApiError::internal(e.to_string()))?;
        let bytes = housewright_api::library::download_manual(&mut ManualFetch::new(), &src)?;
        core::with_api(&path, |api| api.call_upload("attach_manual", args, &bytes))
    })
    .await
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
            save_for_viewer,
            save_manual
        ])
        .run(tauri::generate_context!())
        .expect("error while running Housewright");
}
