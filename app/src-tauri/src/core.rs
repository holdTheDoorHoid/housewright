//! The one open database per process. Tauri commands and the JNI bridge both
//! go through [`call`], so there is never more than one `Store` (and one
//! hybrid logical clock) per database file in a process.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use housewright_api::{Api, ApiError};
use housewright_core::store::Store;
use serde_json::Value;

pub const DB_FILE: &str = "housewright.db";

static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
static CORE: Mutex<Option<(PathBuf, Api)>> = Mutex::new(None);

/// Where the database lives, when the platform told us (Android: the
/// reminders plugin passes the app's files directory before anything runs).
pub fn db_path() -> Option<PathBuf> {
    DB_PATH.get().cloned()
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn set_db_path(p: PathBuf) {
    let _ = DB_PATH.set(p);
}

pub fn call(path: &Path, cmd: &str, args: Value) -> Result<Value, ApiError> {
    let mut guard = CORE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|(p, _)| p != path) {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ApiError::internal(e.to_string()))?;
        }
        let store = Store::open(path)?;
        *guard = Some((path.to_path_buf(), Api::new(store)?));
    }
    let (_, api) = guard.as_mut().expect("opened above");
    api.call(cmd, args)
}
