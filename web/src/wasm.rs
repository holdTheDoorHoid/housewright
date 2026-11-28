//! WebAssembly bindings: the same [`Api`] the Tauri app uses, with SQLite
//! stored in the browser's Origin Private File System through the
//! `opfs-sahpool` VFS (D-61).
//!
//! The sahpool VFS needs synchronous file handles, which browsers only give to
//! dedicated workers, so this module runs inside a worker
//! (`app/src/lib/api/worker.ts`) and the page talks to it by messages. It also
//! takes exclusive handles: a second tab cannot open the same database, which
//! the worker turns into a clear message with a Web Lock before trying.

use std::cell::RefCell;

use housewright_api::Api;
use housewright_core::store::{Store, SystemClock};
use sqlite_wasm_rs::WasmOsCallback;
use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfgBuilder, install};
use wasm_bindgen::prelude::*;

thread_local! {
    static API: RefCell<Option<Api>> = const { RefCell::new(None) };
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// Open (or create) the household database in OPFS. Returns the storage
/// kind ("opfs").
#[wasm_bindgen]
pub async fn open_opfs(db_name: String) -> Result<String, JsValue> {
    let cfg = OpfsSAHPoolCfgBuilder::new()
        .vfs_name("housewright-opfs")
        .directory("housewright")
        .build();
    install::<WasmOsCallback>(&cfg, true)
        .await
        .map_err(js_err)?;
    let store = Store::open(&db_name).map_err(js_err)?;
    let api = Api::new(store).map_err(js_err)?;
    API.with(|a| *a.borrow_mut() = Some(api));
    Ok("opfs".into())
}

/// Fallback when OPFS is unavailable (old browsers, some private windows):
/// an in-memory database that is lost when the tab closes.
#[wasm_bindgen]
pub fn open_memory() -> Result<String, JsValue> {
    let store = Store::open_in_memory(Box::new(SystemClock)).map_err(js_err)?;
    let api = Api::new(store).map_err(js_err)?;
    API.with(|a| *a.borrow_mut() = Some(api));
    Ok("memory".into())
}

/// Run one command; JSON text in, `{"ok": ...}` or `{"error": ...}` out.
#[wasm_bindgen]
pub fn call(cmd: &str, args_json: &str) -> String {
    API.with(|a| match a.borrow_mut().as_mut() {
        Some(api) => api.call_json(cmd, args_json),
        None => r#"{"error":{"code":"internal","message":"database is not open"}}"#.into(),
    })
}

/// Close the database (releases the OPFS handles for another tab).
#[wasm_bindgen]
pub fn close() {
    API.with(|a| *a.borrow_mut() = None);
}
