package app.housewright.reminders

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import org.json.JSONObject
import kotlin.concurrent.thread

/**
 * The notification's Done button (P1): records the completion through the
 * core, with no activity or WebView, then re-plans the remaining reminders.
 * Works when the app process was not running; Android starts it for us.
 */
class ActionReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action != ACTION_DONE) return
        val taskId = intent.getStringExtra("taskId") ?: return
        val nid = intent.getIntExtra("notificationId", 0)
        if (intent.getBooleanExtra("test", false)) {
            // A test reminder only checks that buttons work; it must not log a fake completion.
            NotificationManagerCompat.from(ctx).cancel(nid)
            Log.i(Scheduler.TAG, "test reminder done for $taskId: nothing recorded")
            return
        }
        val result = goAsync()
        val app = ctx.applicationContext
        thread(name = "housewright-done") {
            try {
                NativeCore.run(app, "complete", JSONObject().put("id", taskId))
                Log.i(Scheduler.TAG, "done recorded from notification for $taskId")
                NotificationManagerCompat.from(app).cancel(nid)
                Scheduler.syncFromCore(app)
            } catch (e: Exception) {
                Log.e(Scheduler.TAG, "could not record done for $taskId: ${e.message}")
            } finally {
                result.finish()
            }
        }
    }

    companion object {
        const val ACTION_DONE = "app.housewright.reminders.DONE"
    }
}
