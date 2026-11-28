package app.housewright.reminders

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.util.Log
import androidx.core.content.FileProvider
import java.io.File
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import kotlin.concurrent.thread

@InvokeArg
class TestArgs {
    var inSeconds: Int = 30
}

@InvokeArg
class FileArgs {
    lateinit var path: String
    var mime: String = "application/octet-stream"
    var title: String = ""
}

/**
 * The commands the interface calls: `plugin:reminders|sync`, `|test`,
 * `|pending`, and (Phase 4) `|view_file` and `|share_file`, which hand a
 * file in the app's cache to the phone's own viewer or share sheet. The
 * plugin also notices `housewright://item/<id>` links from a QR sticker
 * (D-30) and reports them through `pending` like a notification tap.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")
    ]
)
class RemindersPlugin(private val activity: Activity) : Plugin(activity) {
    private var pendingAction: String? = null
    private var pendingTask: String? = null

    init {
        // Tell the core where the database is before the interface asks for
        // anything, so Tauri commands and the receivers open the same file.
        NativeCore.setDbPath(NativeCore.dbPath(activity))
        Scheduler.ensureChannel(activity)
        capture(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        capture(intent)
    }

    private fun capture(intent: Intent?) {
        val data = intent?.data
        if (data != null && data.scheme == "housewright" && data.host == "item") {
            val id = data.pathSegments.firstOrNull()
            if (id != null && id.matches(Regex("[0-9a-fA-F-]{36}"))) {
                pendingAction = "open_item"
                pendingTask = id
                Log.i(Scheduler.TAG, "sticker link for item $id")
            }
            intent.data = null
            return
        }
        val action = intent?.getStringExtra(EXTRA_ACTION) ?: return
        pendingAction = action
        pendingTask = intent.getStringExtra(EXTRA_TASK)
        intent.removeExtra(EXTRA_ACTION)
        intent.removeExtra(EXTRA_TASK)
        Log.i(Scheduler.TAG, "notification action $action for $pendingTask")
    }

    @Command
    fun sync(invoke: Invoke) {
        thread(name = "housewright-sync") {
            val n = Scheduler.syncFromCore(activity.applicationContext)
            invoke.resolve(JSObject().put("scheduled", n))
        }
    }

    @Command
    fun test(invoke: Invoke) {
        val args = invoke.parseArgs(TestArgs::class.java)
        thread(name = "housewright-test") {
            try {
                val ctx = activity.applicationContext
                val r = NativeCore.run(ctx, "test_reminder", JSONObject().put("in_seconds", args.inSeconds)) as JSONObject
                r.put("test", true) // Done on a test reminder must never log a real completion
                Scheduler.ensureChannel(ctx)
                Scheduler.schedule(ctx, r, Scheduler.TEST_CODE)
                invoke.resolve(JSObject(r.toString()))
            } catch (e: Exception) {
                invoke.reject(e.message ?: "could not schedule a test reminder")
            }
        }
    }

    @Command
    fun pending(invoke: Invoke) {
        val out = JSObject()
        pendingAction?.let { out.put("action", it) }
        pendingTask?.let { out.put("taskId", it) }
        pendingAction = null
        pendingTask = null
        invoke.resolve(out)
    }

    private fun fileUri(path: String) = FileProvider.getUriForFile(
        activity, activity.packageName + ".fileprovider", File(path)
    )

    @Command
    fun view_file(invoke: Invoke) {
        val args = invoke.parseArgs(FileArgs::class.java)
        try {
            val intent = Intent(Intent.ACTION_VIEW)
                .setDataAndType(fileUri(args.path), args.mime)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            activity.startActivity(Intent.createChooser(intent, args.title.ifEmpty { null }))
            invoke.resolve()
        } catch (e: ActivityNotFoundException) {
            invoke.reject("No app on this phone can open this kind of file.")
        } catch (e: Exception) {
            invoke.reject(e.message ?: "could not open the file")
        }
    }

    @Command
    fun share_file(invoke: Invoke) {
        val args = invoke.parseArgs(FileArgs::class.java)
        try {
            val intent = Intent(Intent.ACTION_SEND)
                .setType(args.mime)
                .putExtra(Intent.EXTRA_STREAM, fileUri(args.path))
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            activity.startActivity(Intent.createChooser(intent, args.title.ifEmpty { null }))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "could not share the file")
        }
    }

    companion object {
        const val EXTRA_ACTION = "housewright_action"
        const val EXTRA_TASK = "housewright_task"
    }
}
