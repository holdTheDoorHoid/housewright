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
use housewright_core::blobs::{MemoryBlobs, SqliteBlobs};
use housewright_core::store::{Store, SystemClock};
use housewright_media::ocr::Ocr;
use sqlite_wasm_rs::WasmOsCallback;
use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfgBuilder, install};
use wasm_bindgen::prelude::*;

thread_local! {
    static API: RefCell<Option<Api>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
extern "C" {
    /// A synchronous XMLHttpRequest, defined by the worker
    /// (`app/src/lib/api/worker.ts`). Synchronous requests are allowed in
    /// dedicated workers; the worker owns the database, so a sync simply
    /// holds it for the length of the exchange (D-97).
    #[wasm_bindgen(js_namespace = globalThis, js_name = hwHttp, catch)]
    fn hw_http(method: &str, url: &str, headers: &str, body: &[u8]) -> Result<JsValue, JsValue>;
}

/// Sync's transport on the web.
struct XhrTransport;

impl housewright_sync::Transport for XhrTransport {
    fn send(
        &mut self,
        req: housewright_sync::HttpRequest,
    ) -> Result<housewright_sync::HttpResponse, String> {
        let headers = serde_json::to_string(&req.headers).map_err(|e| e.to_string())?;
        let v = hw_http(&req.method, &req.url, &headers, &req.body)
            .map_err(|e| e.as_string().unwrap_or_else(|| "network error".into()))?;
        let get =
            |k: &str| js_sys::Reflect::get(&v, &JsValue::from_str(k)).unwrap_or(JsValue::UNDEFINED);
        let status = get("status").as_f64().unwrap_or(0.0) as u16;
        let raw = get("headers").as_string().unwrap_or_default();
        let headers = raw
            .split("\r\n")
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let body = js_sys::Uint8Array::new(&get("body")).to_vec();
        Ok(housewright_sync::HttpResponse {
            status,
            headers,
            body,
        })
    }
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
    // Photos and documents: a second SQLite file in the same private
    // storage, beside the database (D-71).
    let blobs = SqliteBlobs::open(format!("{db_name}-blobs")).map_err(js_err)?;
    let mut api = Api::with_blobs(store, Box::new(blobs)).map_err(js_err)?;
    api.set_transport(Box::new(XhrTransport));
    API.with(|a| *a.borrow_mut() = Some(api));
    Ok("opfs".into())
}

/// Fallback when OPFS is unavailable (old browsers, some private windows):
/// an in-memory database that is lost when the tab closes.
#[wasm_bindgen]
pub fn open_memory() -> Result<String, JsValue> {
    let store = Store::open_in_memory(Box::new(SystemClock)).map_err(js_err)?;
    let mut api = Api::with_blobs(store, Box::new(MemoryBlobs::default())).map_err(js_err)?;
    api.set_transport(Box::new(XhrTransport));
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

const NOT_OPEN: &str = r#"{"error":{"code":"internal","message":"database is not open"}}"#;

fn parse_args(args_json: &str) -> Result<serde_json::Value, String> {
    if args_json.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(args_json).map_err(|e| {
        serde_json::json!({"error": {"code": "invalid", "message": format!("arguments are not JSON: {e}")}})
            .to_string()
    })
}

/// Run a command that takes a file's bytes (`attach`, `scan_label`,
/// `read_label`, `restore_backup`).
#[wasm_bindgen]
pub fn call_upload(cmd: &str, args_json: &str, bytes: &[u8]) -> String {
    let a = match parse_args(args_json) {
        Ok(a) => a,
        Err(e) => return e,
    };
    API.with(|api| match api.borrow_mut().as_mut() {
        Some(api) => match api.call_upload(cmd, a, bytes) {
            Ok(v) => serde_json::json!({ "ok": v }).to_string(),
            Err(e) => serde_json::json!({ "error": e }).to_string(),
        },
        None => NOT_OPEN.into(),
    })
}

/// Run a command that returns bytes. Errors are thrown as the same JSON
/// envelope text the other calls return.
#[wasm_bindgen]
pub fn call_download(cmd: &str, args_json: &str) -> Result<Vec<u8>, JsValue> {
    let a = parse_args(args_json).map_err(|e| JsValue::from_str(&e))?;
    API.with(|api| match api.borrow_mut().as_mut() {
        Some(api) => api
            .call_download(cmd, a)
            .map_err(|e| JsValue::from_str(&serde_json::json!({ "error": e }).to_string())),
        None => Err(JsValue::from_str(NOT_OPEN)),
    })
}

/// Load the label-reading models (fetched by the worker from the app's own
/// files, never from anywhere else; D-74).
#[wasm_bindgen]
pub fn load_ocr(detection: Vec<u8>, recognition: Vec<u8>) -> Result<(), JsValue> {
    let ocr = Ocr::new(detection, recognition).map_err(js_err)?;
    API.with(|api| match api.borrow_mut().as_mut() {
        Some(api) => {
            api.set_ocr(ocr);
            Ok(())
        }
        None => Err(JsValue::from_str("database is not open")),
    })
}

#[wasm_bindgen]
pub fn has_ocr() -> bool {
    API.with(|api| api.borrow().as_ref().is_some_and(Api::has_ocr))
}
