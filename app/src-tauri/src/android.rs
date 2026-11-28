//! JNI entry points for the reminders plugin's Kotlin code
//! (`app.housewright.reminders.NativeCore`). They let a broadcast receiver
//! record "Done" or recompute the reminder plan with no activity or WebView.

use std::path::PathBuf;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use serde_json::{Value, json};

fn text(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_housewright_reminders_NativeCore_setDbPath(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
) {
    let p = text(&mut env, &path);
    if !p.is_empty() {
        crate::core::set_db_path(PathBuf::from(p));
    }
}

/// `call(dbPath, cmd, argsJson)`: JSON `{"ok": ...}` or `{"error": ...}`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_housewright_reminders_NativeCore_call(
    mut env: JNIEnv,
    _class: JClass,
    path: JString,
    cmd: JString,
    args: JString,
) -> jstring {
    let path = PathBuf::from(text(&mut env, &path));
    crate::core::set_db_path(path.clone());
    let cmd = text(&mut env, &cmd);
    let args = text(&mut env, &args);
    let out = match serde_json::from_str::<Value>(if args.is_empty() { "null" } else { &args }) {
        Ok(a) => match crate::core::call(&path, &cmd, a) {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "error": e }),
        },
        Err(e) => json!({ "error": { "code": "invalid", "message": e.to_string() } }),
    };
    log::info!(
        "native {cmd}: {}",
        if out.get("ok").is_some() {
            "ok"
        } else {
            "error"
        }
    );
    env.new_string(out.to_string())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
