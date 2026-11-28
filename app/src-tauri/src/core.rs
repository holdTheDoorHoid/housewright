//! The one open database per process. Tauri commands and the JNI bridge both
//! go through [`call`], so there is never more than one `Store` (and one
//! hybrid logical clock) per database file in a process.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use housewright_api::{Api, ApiError};
use housewright_core::blobs::DirBlobs;
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

/// Run `f` on the one open [`Api`] for the database at `path`.
pub fn with_api<T>(
    path: &Path,
    f: impl FnOnce(&mut Api) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let mut guard = CORE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|(p, _)| p != path) {
        let dir = path
            .parent()
            .ok_or_else(|| ApiError::internal("the database has no folder"))?;
        std::fs::create_dir_all(dir).map_err(|e| ApiError::internal(e.to_string()))?;
        let store = Store::open(path)?;
        // Photos and documents live beside the database, named by content
        // hash (D-71).
        let blobs = DirBlobs::open(dir.join("blobs"))?;
        let mut api = Api::with_blobs(store, Box::new(blobs))?;
        // Sync over HTTPS with rustls (D-96, D-97). Only used once sync is
        // set up; until then nothing touches the network.
        api.set_transport(Box::new(
            housewright_sync::native::UreqTransport::default(),
        ));
        #[cfg(feature = "ocr")]
        api.set_ocr_loader(Box::new(|| {
            // Bundled with the app, never downloaded (D-74).
            static DETECTION: &[u8] = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../models/ocr/text-detection.onnx"
            ));
            static RECOGNITION: &[u8] = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../models/ocr/text-recognition.onnx"
            ));
            housewright_media::ocr::Ocr::from_static(DETECTION, RECOGNITION)
        }));
        *guard = Some((path.to_path_buf(), api));
    }
    let (_, api) = guard.as_mut().expect("opened above");
    f(api)
}

pub fn call(path: &Path, cmd: &str, args: Value) -> Result<Value, ApiError> {
    with_api(path, |api| api.call(cmd, args))
}
