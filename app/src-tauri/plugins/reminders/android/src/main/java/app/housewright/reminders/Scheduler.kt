package app.housewright.reminders

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

/**
 * Turns the core's reminder plan into AlarmManager alarms. Alarms are inexact
 * (setAndAllowWhileIdle): no exact-alarm permission is needed, and Android may
 * deliver a few minutes late in Doze, which is fine for home maintenance.
 */
object Scheduler {
    const val TAG = "housewright"
    const val CHANNEL = "reminders"
    const val ACTION_FIRE = "app.housewright.reminders.FIRE"
    private const val PREFS = "housewright_reminders"
    private const val PLAN_BASE = 1000
    const val TEST_CODE = 7

    fun ensureChannel(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            val ch = NotificationChannel(CHANNEL, "Reminders", NotificationManager.IMPORTANCE_DEFAULT)
            ch.description = "Home maintenance that is due"
            nm.createNotificationChannel(ch)
        }
    }

    private fun intentFor(ctx: Context, r: JSONObject): Intent =
        Intent(ctx, AlarmReceiver::class.java)
            .setAction(ACTION_FIRE)
            .putExtra("taskId", r.getString("task_id"))
            .putExtra("title", r.optString("title"))
            .putExtra("body", r.optString("body"))
            .putExtra("kind", r.optString("kind"))
            .putExtra("test", r.optBoolean("test", false))

    fun schedule(ctx: Context, r: JSONObject, code: Int) {
        val pi = PendingIntent.getBroadcast(
            ctx, code, intentFor(ctx, r),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val am = ctx.getSystemService(AlarmManager::class.java)
        am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, r.getLong("at_ms"), pi)
    }

    private fun cancel(ctx: Context, code: Int) {
        val pi = PendingIntent.getBroadcast(
            ctx, code, Intent(ctx, AlarmReceiver::class.java).setAction(ACTION_FIRE),
            PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE
        ) ?: return
        ctx.getSystemService(AlarmManager::class.java).cancel(pi)
        pi.cancel()
    }

    /** Replace every planned alarm with the given plan. */
    @Synchronized
    fun scheduleAll(ctx: Context, plan: JSONArray): Int {
        val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val before = prefs.getInt("count", 0)
        for (i in 0 until before) cancel(ctx, PLAN_BASE + i)
        for (i in 0 until plan.length()) schedule(ctx, plan.getJSONObject(i), PLAN_BASE + i)
        prefs.edit()
            .putInt("count", plan.length())
            .putLong("synced_at", System.currentTimeMillis())
            .apply()
        Log.i(TAG, "scheduled ${plan.length()} reminders (replaced $before)")
        return plan.length()
    }

    /** Ask the core for the plan and schedule it. Safe to call from any thread. */
    fun syncFromCore(ctx: Context): Int {
        ensureChannel(ctx)
        return try {
            val plan = NativeCore.run(ctx, "reminder_plan") as? JSONArray ?: JSONArray()
            scheduleAll(ctx, plan)
        } catch (e: Exception) {
            Log.w(TAG, "could not plan reminders: ${e.message}")
            0
        }
    }
}
