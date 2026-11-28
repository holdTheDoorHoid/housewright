package app.housewright.reminders

import android.Manifest
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/** An alarm went off: post the reminder with Done and Reschedule buttons. */
class AlarmReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val taskId = intent.getStringExtra("taskId") ?: return
        val title = intent.getStringExtra("title") ?: "Home maintenance"
        val body = intent.getStringExtra("body") ?: ""
        Scheduler.ensureChannel(ctx)
        val nid = notificationId(taskId)

        val done = PendingIntent.getBroadcast(
            ctx, nid,
            Intent(ctx, ActionReceiver::class.java)
                .setAction(ActionReceiver.ACTION_DONE)
                .putExtra("taskId", taskId)
                .putExtra("notificationId", nid),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val reschedule = PendingIntent.getActivity(
            ctx, nid * 2 + 1, launch(ctx, "reschedule", taskId),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val open = PendingIntent.getActivity(
            ctx, nid * 2, launch(ctx, "open", taskId),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val n = NotificationCompat.Builder(ctx, Scheduler.CHANNEL)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle(title)
            .setContentText(body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setContentIntent(open)
            .setAutoCancel(true)
            .addAction(0, "Done", done)
            .addAction(0, "Reschedule", reschedule)
            .build()

        val allowed = Build.VERSION.SDK_INT < 33 ||
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!allowed) {
            Log.w(Scheduler.TAG, "reminder for $taskId not shown: notifications not allowed")
            return
        }
        NotificationManagerCompat.from(ctx).notify(nid, n)
        Log.i(Scheduler.TAG, "reminder shown for $taskId")
    }

    companion object {
        fun notificationId(taskId: String): Int = (taskId.hashCode() and 0x3fffffff)

        fun launch(ctx: Context, action: String, taskId: String): Intent {
            val i = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)
                ?: Intent()
            return i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(RemindersPlugin.EXTRA_ACTION, action)
                .putExtra(RemindersPlugin.EXTRA_TASK, taskId)
        }
    }
}
