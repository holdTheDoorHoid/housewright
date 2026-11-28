package app.housewright.reminders

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import kotlin.concurrent.thread

/**
 * Android clears alarms on reboot (and they may be stale after an app update
 * or a time zone change), so re-plan everything from the core.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        Log.i(Scheduler.TAG, "rescheduling after ${intent.action}")
        val result = goAsync()
        val app = ctx.applicationContext
        thread(name = "housewright-boot") {
            try {
                Scheduler.syncFromCore(app)
            } finally {
                result.finish()
            }
        }
    }
}
