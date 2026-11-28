package app.housewright.reminders

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.util.Log
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

/** The commands the interface calls: `plugin:reminders|sync`, `|test`, `|pending`. */
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

    companion object {
        const val EXTRA_ACTION = "housewright_action"
        const val EXTRA_TASK = "housewright_task"
    }
}
