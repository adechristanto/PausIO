package com.pausio.app.eyecare

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * The phone's own break delivery.
 *
 * Android doze and app standby mean a backgrounded process is not guaranteed
 * to run at the moment a break falls due, so reminders are handed to
 * `AlarmManager` in advance. This is what lets PausIO alert someone on an
 * Android phone with no Wear OS watch, no pairing, and no network.
 *
 * Only the *next* transition is armed at a time and the receiver chains the
 * one after it. Pre-registering a long plan would burn alarm slots the system
 * may reclaim, and it would go stale the moment settings changed; the Wear
 * companion already uses this same chaining approach.
 */
internal object PausIOPhoneReminders {
    private const val preferencesName = "pausio.phone.reminders"
    private const val planKey = "plan"
    private const val preBreakChannel = "pausio.phone.pre_break"
    private const val breakDueChannel = "pausio.phone.break_due"
    private const val preBreakRequestCode = 5101
    private const val breakRequestCode = 5102
    private const val startActionRequestCode = 5201
    private const val pauseActionRequestCode = 5202

    internal const val preBreakAction = "com.pausio.app.PHONE_PRE_BREAK"
    internal const val breakAction = "com.pausio.app.PHONE_BREAK_DUE"
    internal const val startBreakAction = "com.pausio.app.PHONE_START_BREAK"
    internal const val pauseRemindersAction = "com.pausio.app.PHONE_PAUSE"

    /**
     * Stores the plan and arms its first future instant.
     *
     * An empty [slots] cancels everything, which is how a pause or a
     * watch-only alert target clears the phone's reminders.
     */
    fun replace(context: Context, slots: JSONArray): JSONObject {
        cancel(context)
        ensureChannels(context)
        val now = System.currentTimeMillis()
        val upcoming = JSONArray()
        for (index in 0 until slots.length()) {
            val slot = slots.optJSONObject(index) ?: continue
            val at = parseInstant(slot.optString("at", "")) ?: continue
            if (at <= now) continue
            upcoming.put(JSONObject().put("at", at).put("kind", slot.optString("kind", "break_due")))
        }
        preferences(context).edit().putString(planKey, upcoming.toString()).apply()

        if (upcoming.length() == 0) {
            return report(0, null, context)
        }
        if (!notificationsAllowed(context)) {
            // Without notification permission there is no standalone delivery
            // path at all, so this is reported as a failure rather than a
            // silent no-op that looks like success.
            return report(0, null, context)
                .put("last_error", "Notifications are not permitted, so breaks cannot be announced")
        }
        val horizon = armNext(context, now)
        return report(upcoming.length(), horizon, context)
    }

    /** Arms the earliest future instant in the stored plan. */
    fun armNext(context: Context, now: Long = System.currentTimeMillis()): Long? {
        val plan = storedPlan(context)
        var next: JSONObject? = null
        for (index in 0 until plan.length()) {
            val slot = plan.optJSONObject(index) ?: continue
            val at = slot.optLong("at", 0)
            if (at <= now) continue
            if (next == null || at < next!!.optLong("at", Long.MAX_VALUE)) next = slot
        }
        val slot = next ?: return null
        val at = slot.optLong("at", 0)
        val isPreBreak = slot.optString("kind") == "pre_break"
        schedule(
            context,
            if (isPreBreak) preBreakRequestCode else breakRequestCode,
            if (isPreBreak) preBreakAction else breakAction,
            at,
        )
        return at
    }

    /** Drops instants already in the past, then arms the next one. */
    fun advance(context: Context) {
        val now = System.currentTimeMillis()
        val plan = storedPlan(context)
        val remaining = JSONArray()
        for (index in 0 until plan.length()) {
            val slot = plan.optJSONObject(index) ?: continue
            if (slot.optLong("at", 0) > now) remaining.put(slot)
        }
        preferences(context).edit().putString(planKey, remaining.toString()).apply()
        armNext(context, now)
    }

    fun cancel(context: Context) {
        val alarmManager = context.getSystemService(AlarmManager::class.java) ?: return
        for ((requestCode, action) in listOf(
            preBreakRequestCode to preBreakAction,
            breakRequestCode to breakAction,
        )) {
            val pending = pendingIntent(context, requestCode, action, PendingIntent.FLAG_NO_CREATE)
                ?: continue
            alarmManager.cancel(pending)
            pending.cancel()
        }
    }

    fun clear(context: Context) {
        cancel(context)
        preferences(context).edit().remove(planKey).apply()
    }

    fun notificationsAllowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    fun permissionState(context: Context): String =
        if (notificationsAllowed(context)) "granted" else "denied"

    fun canScheduleExact(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
            context.getSystemService(AlarmManager::class.java)?.canScheduleExactAlarms() == true

    fun postReminder(context: Context, isPreBreak: Boolean) {
        if (!notificationsAllowed(context)) return
        ensureChannels(context)
        val channel = if (isPreBreak) preBreakChannel else breakDueChannel
        val title = if (isPreBreak) "Break coming up" else "Time to rest your eyes"
        val body = if (isPreBreak) {
            "Find a good place to pause."
        } else {
            "Look about 20 feet away for 20 seconds."
        }
        val builder = NotificationCompat.Builder(context, channel)
            .setSmallIcon(android.R.drawable.ic_popup_reminder)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setPriority(
                if (isPreBreak) NotificationCompat.PRIORITY_DEFAULT
                else NotificationCompat.PRIORITY_HIGH,
            )
        if (isPreBreak) {
            builder.addAction(
                0,
                "Pause reminders",
                pendingIntent(
                    context, pauseActionRequestCode, pauseRemindersAction,
                    PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
        } else {
            builder.addAction(
                0,
                "Start break",
                pendingIntent(
                    context, startActionRequestCode, startBreakAction,
                    PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
        }
        NotificationManagerCompat.from(context).notify(
            if (isPreBreak) preBreakRequestCode else breakRequestCode,
            builder.build(),
        )
    }

    private fun schedule(context: Context, requestCode: Int, action: String, at: Long) {
        val alarmManager = context.getSystemService(AlarmManager::class.java) ?: return
        val pending = pendingIntent(context, requestCode, action, PendingIntent.FLAG_UPDATE_CURRENT)
            ?: return
        // Exact when the OS allows it. When exact alarms have been revoked the
        // reminder still fires, just later; the degraded precision is reported
        // rather than hidden, because a break that arrives ten minutes late is
        // a different product promise.
        if (canScheduleExact(context)) {
            alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        } else {
            alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        }
    }

    private fun report(scheduled: Int, horizon: Long?, context: Context): JSONObject {
        val payload = JSONObject()
            .put("scheduled", scheduled)
            .put("precision", if (canScheduleExact(context)) "exact" else "inexact")
            .put("permission", permissionState(context))
        if (horizon != null) {
            payload.put("horizon_at", formatInstant(horizon))
        }
        return payload
    }

    private fun storedPlan(context: Context): JSONArray =
        runCatching { JSONArray(preferences(context).getString(planKey, "[]")) }
            .getOrDefault(JSONArray())

    private fun ensureChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        manager.createNotificationChannel(
            NotificationChannel(
                preBreakChannel, "Upcoming breaks", NotificationManager.IMPORTANCE_DEFAULT,
            ),
        )
        manager.createNotificationChannel(
            NotificationChannel(
                breakDueChannel, "Break reminders", NotificationManager.IMPORTANCE_HIGH,
            ),
        )
    }

    private fun pendingIntent(
        context: Context,
        requestCode: Int,
        action: String,
        flags: Int,
    ): PendingIntent? = PendingIntent.getBroadcast(
        context,
        requestCode,
        Intent(context, PausIOPhoneReminderReceiver::class.java).setAction(action),
        flags or PendingIntent.FLAG_IMMUTABLE,
    )

    private fun preferences(context: Context) =
        context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)

    private fun parseInstant(value: String): Long? = runCatching {
        java.time.Instant.parse(value).toEpochMilli()
    }.getOrNull()

    private fun formatInstant(value: Long): String =
        java.time.Instant.ofEpochMilli(value).toString()
}

/**
 * Fires a reminder, advances the chain, and turns action taps into runtime
 * actions the Rust tick loop already drains.
 */
internal class PausIOPhoneReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            PausIOPhoneReminders.preBreakAction ->
                PausIOPhoneReminders.postReminder(context, isPreBreak = true)
            PausIOPhoneReminders.breakAction ->
                PausIOPhoneReminders.postReminder(context, isPreBreak = false)
            PausIOPhoneReminders.startBreakAction ->
                PausIOWearRuntimeActions.enqueue(context, localAction("take_break_now"))
            PausIOPhoneReminders.pauseRemindersAction ->
                PausIOWearRuntimeActions.enqueue(context, localAction("pause"))
            else -> return
        }
        PausIOPhoneReminders.advance(context)
    }

    private fun localAction(action: String): JSONObject = JSONObject()
        .put("schema_version", 1)
        .put("action_id", java.util.UUID.randomUUID().toString())
        .put("action", action)
        .put("base_revision", 0)
        .put("occurred_at", java.time.format.DateTimeFormatter.ISO_INSTANT.format(java.time.Instant.now()))
}

/**
 * Re-arms the chain after the system drops pending alarms. Without this, a
 * reboot would silently end reminders until the app was next opened.
 */
internal class PausIOPhoneRescheduleReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in setOf(
                Intent.ACTION_BOOT_COMPLETED,
                Intent.ACTION_MY_PACKAGE_REPLACED,
                Intent.ACTION_TIME_CHANGED,
                Intent.ACTION_TIMEZONE_CHANGED,
                AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED,
            )
        ) {
            return
        }
        PausIOPhoneReminders.advance(context)
    }
}
