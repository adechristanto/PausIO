package com.pausio.app.wear

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
import android.os.VibrationEffect
import android.os.Vibrator
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import java.time.Instant

/**
 * The watch schedules a single next transition, then chains the next one from
 * its receiver. This survives long offline periods without keeping a service
 * alive and makes the exact/inexact policy visible to the paired phone.
 */
internal object PausIOWearReminderScheduler {
    private const val preBreakRequestCode = 4101
    private const val breakRequestCode = 4102
    private const val breakEndRequestCode = 4103
    private const val preBreakAction = "com.pausio.app.wear.PRE_BREAK"
    private const val breakAction = "com.pausio.app.wear.BREAK_START"
    private const val breakEndAction = "com.pausio.app.wear.BREAK_END"
    private const val pauseAction = "com.pausio.app.wear.PAUSE"
    private const val startAction = "com.pausio.app.wear.START_BREAK"
    private const val preBreakChannel = "pausio.pre_break"
    private const val breakDueChannel = "pausio.break_due"

    fun replace(context: Context) {
        cancel(context)
        ensureNotificationChannels(context)
        val now = Instant.now()
        val state = PausIOWearStateStore.read(context, now)
        if (state.paused || state.phase == WatchTimerPhase.Dormant || state.phase == WatchTimerPhase.Paused) {
            PausIOWearHealth.publish(context, horizon = null)
            return
        }
        val horizon = when (state.phase) {
            WatchTimerPhase.Working, WatchTimerPhase.PreBreak -> {
                val reminder = PausIOWearReminderPlanner.nextReminder(state, now)
                if (reminder != null) {
                    if (state.preBreakSeconds > 0) {
                        val preBreakAt = reminder.minusSeconds(state.preBreakSeconds)
                        if (preBreakAt.isAfter(now)) schedule(context, preBreakRequestCode, preBreakAction, preBreakAt)
                    }
                    schedule(context, breakRequestCode, breakAction, reminder)
                }
                reminder
            }
            WatchTimerPhase.BreakDue -> now.plusSeconds(1).also {
                schedule(context, breakRequestCode, breakAction, it)
            }
            WatchTimerPhase.Breaking -> state.phaseDeadlineAt?.takeIf { it.isAfter(now) }?.also {
                schedule(context, breakEndRequestCode, breakEndAction, it)
            } ?: run {
                PausIOWearStateStore.finishLocalBreak(context, now)
                replace(context)
                return
            }
            WatchTimerPhase.Dormant, WatchTimerPhase.Paused -> null
        }
        PausIOWearHealth.publish(context, horizon)
    }

    fun cancel(context: Context) {
        val alarmManager = context.getSystemService(AlarmManager::class.java)
        for ((requestCode, action) in listOf(
            preBreakRequestCode to preBreakAction,
            breakRequestCode to breakAction,
            breakEndRequestCode to breakEndAction,
            4201 to pauseAction,
            4202 to startAction,
        )) {
            val pendingIntent = pendingIntent(context, requestCode, action, PendingIntent.FLAG_NO_CREATE) ?: continue
            alarmManager.cancel(pendingIntent)
            pendingIntent.cancel()
        }
    }

    fun canScheduleExact(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.S || context.getSystemService(AlarmManager::class.java).canScheduleExactAlarms()

    fun notificationPermissionGranted(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    private fun schedule(context: Context, requestCode: Int, action: String, at: Instant) {
        pendingIntent(context, requestCode, action, PendingIntent.FLAG_UPDATE_CURRENT)?.let { pendingIntent ->
            val alarmManager = context.getSystemService(AlarmManager::class.java)
            if (canScheduleExact(context)) {
                alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at.toEpochMilli(), pendingIntent)
            } else {
                alarmManager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at.toEpochMilli(), pendingIntent)
            }
        }
    }

    fun vibrate(context: Context, event: HapticEvent = HapticEvent.BreakStart) {
        val vibrator = context.getSystemService(Vibrator::class.java) ?: return
        if (vibrator.hasVibrator()) {
            val effect = when (event) {
                HapticEvent.PreBreak -> VibrationEffect.createPredefined(VibrationEffect.EFFECT_DOUBLE_CLICK)
                HapticEvent.BreakStart -> VibrationEffect.createPredefined(VibrationEffect.EFFECT_HEAVY_CLICK)
            }
            vibrator.vibrate(effect)
        }
    }

    fun postReminder(context: Context, event: HapticEvent) {
        if (!notificationPermissionGranted(context)) return
        val manager = NotificationManagerCompat.from(context)
        val (channel, title, body, notificationId) = when (event) {
            HapticEvent.PreBreak -> ReminderPresentation(
                preBreakChannel,
                context.getString(R.string.notification_pre_break_title),
                context.getString(R.string.notification_pre_break_body),
                preBreakRequestCode,
            )
            HapticEvent.BreakStart -> ReminderPresentation(
                breakDueChannel,
                context.getString(R.string.notification_break_title),
                context.getString(R.string.notification_break_body),
                breakRequestCode,
            )
        }
        val contentIntent = PendingIntent.getActivity(
            context, notificationId,
            Intent(context, MainActivity::class.java),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val actionIntent = pendingIntent(
            context,
            notificationId + 100,
            if (event == HapticEvent.BreakStart) startAction else pauseAction,
            PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val actionLabel = if (event == HapticEvent.BreakStart) R.string.action_start_break else R.string.action_pause
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return
        manager.notify(
            notificationId,
            NotificationCompat.Builder(context, channel)
                .setSmallIcon(R.drawable.ic_launcher_foreground)
                .setContentTitle(title)
                .setContentText(body)
                .setContentIntent(contentIntent)
                .setAutoCancel(true)
                .setCategory(NotificationCompat.CATEGORY_REMINDER)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .addAction(0, context.getString(actionLabel), actionIntent)
                .build(),
        )
    }

    private fun ensureNotificationChannels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(preBreakChannel, context.getString(R.string.notification_channel_pre_break), NotificationManager.IMPORTANCE_DEFAULT).apply {
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 90, 70, 90)
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(breakDueChannel, context.getString(R.string.notification_channel_break_due), NotificationManager.IMPORTANCE_HIGH).apply {
                enableVibration(true)
                vibrationPattern = longArrayOf(0, 250, 80, 250)
            },
        )
    }

    private fun pendingIntent(context: Context, requestCode: Int, action: String, flags: Int): PendingIntent? = PendingIntent.getBroadcast(
        context,
        requestCode,
        Intent(context, PausIOWearReminderReceiver::class.java).setAction(action),
        flags or PendingIntent.FLAG_IMMUTABLE,
    )

    enum class HapticEvent { PreBreak, BreakStart }
    private data class ReminderPresentation(val channel: String, val title: String, val body: String, val id: Int)
}

internal class PausIOWearReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            "com.pausio.app.wear.PRE_BREAK" -> {
                PausIOWearReminderScheduler.vibrate(context, PausIOWearReminderScheduler.HapticEvent.PreBreak)
                PausIOWearReminderScheduler.postReminder(context, PausIOWearReminderScheduler.HapticEvent.PreBreak)
            }
            "com.pausio.app.wear.BREAK_START" -> {
                PausIOWearReminderScheduler.vibrate(context, PausIOWearReminderScheduler.HapticEvent.BreakStart)
                PausIOWearReminderScheduler.postReminder(context, PausIOWearReminderScheduler.HapticEvent.BreakStart)
                PausIOWearStateStore.beginLocalBreak(context)
            }
            "com.pausio.app.wear.BREAK_END" -> PausIOWearStateStore.finishLocalBreak(context)
            "com.pausio.app.wear.PAUSE" -> {
                PausIOWearStateStore.setLocallyPaused(context, true)
                PausIOWearRuntimeAction.publish(context, "pause")
            }
            "com.pausio.app.wear.START_BREAK" -> {
                PausIOWearStateStore.beginLocalBreak(context)
                PausIOWearRuntimeAction.publish(context, "take_break_now")
            }
        }
        PausIOWearReminderScheduler.replace(context)
    }
}

/** Restores the next autonomous transition after the OS clears alarms. */
internal class PausIOWearRescheduleReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in setOf(
                Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED,
                Intent.ACTION_TIME_CHANGED, Intent.ACTION_TIMEZONE_CHANGED,
                AlarmManager.ACTION_SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED,
            )
        ) return
        PausIOWearReminderScheduler.replace(context)
    }
}
