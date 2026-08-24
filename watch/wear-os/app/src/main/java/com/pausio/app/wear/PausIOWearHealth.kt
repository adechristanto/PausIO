package com.pausio.app.wear

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import com.google.android.gms.wearable.PutDataMapRequest
import com.google.android.gms.wearable.Wearable
import org.json.JSONObject
import java.time.Instant

/** Redacted health report used by the phone setup screen and fallback policy. */
internal object PausIOWearHealth {
    const val path = "/pausio/health/v1"

    fun publish(context: Context, horizon: Instant?) {
        val notificationPermission = when {
            Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU -> "granted"
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED -> "granted"
            else -> "denied"
        }
        val payload = JSONObject()
            .put("schema_version", 1)
            .put("app_version", BuildConfig.VERSION_NAME)
            .put("notification_permission", notificationPermission)
            .put("reminder_precision", if (PausIOWearReminderScheduler.canScheduleExact(context)) "exact" else "inexact")
            .put("schedule_horizon_at", horizon?.toString() ?: JSONObject.NULL)
            .put("last_successful_sync_at", Instant.now().toString())
            .toString()
        val request = PutDataMapRequest.create(path).apply {
            dataMap.putString("payload", payload)
        }.asPutDataRequest().setUrgent()
        Wearable.getDataClient(context).putDataItem(request)
    }
}
