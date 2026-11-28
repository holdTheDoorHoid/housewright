// Android glue for Housewright's reminders (D-62). Kotlin is used only here,
// because Tauri mobile plugins and Android broadcast receivers require it; all
// decisions (what to remind, when, and recording Done) are made by the Rust
// core through the same command surface the interface uses.
package app.housewright.reminders

import android.content.Context
import org.json.JSONObject
import java.io.File

/** JNI bridge into the Rust core (app/src-tauri/src/android.rs). */
object NativeCore {
    init {
        System.loadLibrary("housewright_app_lib")
    }

    @JvmStatic external fun setDbPath(path: String)

    /** Run one core command: JSON in, `{"ok": ...}` or `{"error": ...}` out. */
    @JvmStatic external fun call(dbPath: String, cmd: String, args: String): String

    fun dbPath(ctx: Context): String = File(ctx.filesDir, "housewright.db").absolutePath

    /** The command's result, or throws with the core's error message. */
    @Synchronized
    fun run(ctx: Context, cmd: String, args: JSONObject = JSONObject()): Any? {
        val out = JSONObject(call(dbPath(ctx), cmd, args.toString()))
        if (out.has("error")) {
            throw IllegalStateException(out.getJSONObject("error").optString("message", "core error"))
        }
        return out.opt("ok")
    }
}
