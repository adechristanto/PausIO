package com.pausio.app.eyecare

import android.content.Context
import android.app.Activity
import android.os.Handler
import android.os.Looper
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import com.google.android.gms.wearable.CapabilityClient
import com.google.android.gms.wearable.PutDataMapRequest
import com.google.android.gms.wearable.Wearable
import org.json.JSONObject
import org.json.JSONArray
import java.util.UUID
import java.util.TimeZone
import java.text.ParsePosition
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap

private const val watchCapability = "pausio_watch_v1"
private const val settingsPath = "/pausio/settings/v1"
private const val testNudgePath = "/pausio/test-nudge/v1"
internal const val settingsAcknowledgementPath = "/pausio/settings-ack/v1"
internal const val testNudgeAcknowledgementPath = "/pausio/test-nudge-ack/v1"
internal const val runtimeActionPath = "/pausio/runtime-action/v1"
internal const val watchHealthPath = "/pausio/health/v1"

/** Completes immediate test requests only after the watch listener acknowledges them. */
internal object PausIOWearTestNudgeReplies {
    private val replies = ConcurrentHashMap<String, (String) -> Unit>()
    private val mainHandler = Handler(Looper.getMainLooper())

    fun await(eventId: String, completion: (String) -> Unit) {
        replies[eventId] = completion
        mainHandler.postDelayed({
            replies.remove(eventId)?.invoke("unavailable")
        }, 5_000)
    }

    fun complete(eventId: String, result: String) {
        mainHandler.post { replies.remove(eventId)?.invoke(result) }
    }
}

/** Android mobile-plugin bridge. DataItem is durable; MessageClient is only an optional diagnostic nudge. */
class PausIOWearBridge(private val context: Context) {
    fun syncSettings(json: String): String {
        if (!isValidWatchSettings(json)) {
            context.getSharedPreferences("pausio.wear.bridge", Context.MODE_PRIVATE)
                .edit().putString("last_error", "invalid_watch_settings").apply()
            return "unavailable"
        }
        val request = PutDataMapRequest.create(settingsPath).apply {
            dataMap.putString("payload", json)
            asPutDataRequest().setUrgent()
        }.asPutDataRequest().setUrgent()
        val revision = runCatching { JSONObject(json).getLong("revision") }.getOrNull()
        Wearable.getDataClient(context).putDataItem(request)
            .addOnSuccessListener {
                if (revision != null) {
                    context.getSharedPreferences("pausio.wear.bridge", Context.MODE_PRIVATE)
                        .edit().putLong("last_queued_revision", revision).putString("last_error", null).apply()
                }
            }
            .addOnFailureListener { error ->
                context.getSharedPreferences("pausio.wear.bridge", Context.MODE_PRIVATE)
                    .edit().putString("last_error", error.message ?: "settings_queue_failed").apply()
            }
        return "queued"
    }

    fun testNudgePayload(eventId: String): ByteArray = JSONObject()
        .put("kind", "test")
        .put("event_id", eventId)
        .toString()
        .toByteArray()
}

/** Same v1 gate as Rust/Swift/Wear: phone data is authoritative but never blindly forwarded. */
private fun isValidWatchSettings(raw: String): Boolean = try {
    val value = JSONObject(raw)
    value.optInt("schema_version", -1) == 1 && value.optLong("revision", -1) >= 0 &&
        isValidTimeZone(value.optString("timezone", "")) &&
        value.optLong("work_interval_seconds", 0) in 300..7_200 &&
        value.optLong("short_break_seconds", 0) in 5..120 &&
        value.optLong("long_break_seconds", 0) in 5..3_600 &&
        value.optLong("pre_break_seconds", -1) in setOf(0L, 10L, 30L, 60L) &&
        value.optInt("active_days_mask", 0) in 1..127 &&
        value.optInt("active_start_minutes", -1) in 0 until 1_440 &&
        value.optInt("active_end_minutes", -1) in 0 until 1_440 &&
        isValidIsoTimestamp(value.optString("updated_at", ""))
} catch (_: Exception) {
    false
}

private fun isValidTimeZone(identifier: String): Boolean {
    if (identifier.isBlank()) return false
    val zone = TimeZone.getTimeZone(identifier)
    return identifier == "GMT" || zone.id != "GMT"
}

private fun isValidIsoTimestamp(value: String): Boolean {
    if (value.isBlank()) return false
    return listOf("yyyy-MM-dd'T'HH:mm:ss.SSSX", "yyyy-MM-dd'T'HH:mm:ssX").any { pattern ->
        val position = ParsePosition(0)
        SimpleDateFormat(pattern, Locale.US).apply { isLenient = false }.parse(value, position)
        position.errorIndex < 0 && position.index == value.length
    }
}

/** Persists a small LRU before Rust drains a mirrored watch action. */
internal object PausIOWearRuntimeActions {
    private const val preferencesName = "pausio.wear.bridge"
    private const val pendingKey = "pending_runtime_actions"
    private const val seenKey = "seen_runtime_action_ids"
    private const val maxEntries = 32

    fun enqueue(context: Context, payload: JSONObject): Boolean {
        val id = payload.optString("action_id", "")
        val action = payload.optString("action", "")
        val schema = payload.optInt("schema_version", -1)
        if (schema != 1 || id.isBlank() || action !in setOf("pause", "resume", "take_break_now", "skip_break") ||
            payload.optLong("base_revision", -1) < 0 || !isValidIsoTimestamp(payload.optString("occurred_at", ""))
        ) return false
        val preferences = context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
        val seen = JSONArray(preferences.getString(seenKey, "[]"))
        if ((0 until seen.length()).any { seen.optString(it) == id }) return true
        val pending = JSONArray(preferences.getString(pendingKey, "[]"))
        pending.put(payload)
        val trimmed = JSONArray()
        for (index in maxOf(0, pending.length() - maxEntries) until pending.length()) trimmed.put(pending.get(index))
        seen.put(id)
        val retained = JSONArray()
        for (index in maxOf(0, seen.length() - maxEntries) until seen.length()) retained.put(seen.get(index))
        preferences.edit().putString(pendingKey, trimmed.toString()).putString(seenKey, retained.toString()).apply()
        return true
    }

    fun take(context: Context): String? {
        val preferences = context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)
        val pending = JSONArray(preferences.getString(pendingKey, "[]"))
        if (pending.length() == 0) return null
        val next = pending.getJSONObject(0).toString()
        val rest = JSONArray()
        for (index in 1 until pending.length()) rest.put(pending.get(index))
        preferences.edit().putString(pendingKey, rest.toString()).apply()
        return next
    }
}

@TauriPlugin
class PausIOEyecarePlugin(private val activity: Activity) : Plugin(activity) {
    private val bridge = PausIOWearBridge(activity)

    @Command
    fun syncSettings(invoke: Invoke) {
        invoke.resolveObject(bridge.syncSettings(invoke.getRawArgs()))
    }

    @Command
    fun sendTestNudge(invoke: Invoke) {
        Wearable.getCapabilityClient(activity)
            .getCapability(watchCapability, CapabilityClient.FILTER_REACHABLE)
            .addOnSuccessListener { capability ->
                val node = capability.nodes.firstOrNull()
                if (node == null) {
                    invoke.resolveObject("unavailable")
                    return@addOnSuccessListener
                }
                val eventId = UUID.randomUUID().toString()
                PausIOWearTestNudgeReplies.await(eventId) { result -> invoke.resolveObject(result) }
                Wearable.getMessageClient(activity)
                    .sendMessage(node.id, testNudgePath, bridge.testNudgePayload(eventId))
                    .addOnFailureListener { PausIOWearTestNudgeReplies.complete(eventId, "unavailable") }
            }
            .addOnFailureListener { invoke.resolveObject("unavailable") }
    }

    @Command
    fun getStatus(invoke: Invoke) {
        Wearable.getCapabilityClient(activity)
            .getCapability(watchCapability, CapabilityClient.FILTER_ALL)
            .addOnSuccessListener { capability ->
                val appInstalled = capability.nodes.isNotEmpty()
                Wearable.getNodeClient(activity).connectedNodes.addOnSuccessListener { nodes ->
                    val reachable = capability.nodes.any { capable -> nodes.any { it.id == capable.id } }
                    val connectionState = when {
                        !appInstalled && nodes.isEmpty() -> "unpaired"
                        !appInstalled -> "app_not_installed"
                        reachable -> "connected"
                        else -> "disconnected"
                    }
                    val preferences = activity.getSharedPreferences("pausio.wear.bridge", Context.MODE_PRIVATE)
                    val health = runCatching { JSONObject(preferences.getString("watch_health", "{}")) }.getOrDefault(JSONObject())
                    invoke.resolveObject(mapOf(
                        "platform" to "android",
                        "available" to true,
                        "paired" to nodes.isNotEmpty(),
                        "app_installed" to appInstalled,
                        "reachable" to reachable,
                        "last_synced_revision" to preferences.takeIf { it.contains("last_applied_revision") }
                            ?.getLong("last_applied_revision", 0),
                        "last_queued_revision" to preferences.takeIf { it.contains("last_queued_revision") }
                            ?.getLong("last_queued_revision", 0),
                        "last_error" to preferences.getString("last_error", null),
                        "notification_permission" to health.optString("notification_permission", "unknown"),
                        "reminder_precision" to health.optString("reminder_precision", "not_available"),
                        "schedule_horizon_at" to health.optString("schedule_horizon_at", "").ifBlank { null },
                        "last_successful_sync_at" to health.optString("last_successful_sync_at", "").ifBlank { null },
                        "app_version" to health.optString("app_version", "").ifBlank { null },
                        "connection_state" to connectionState,
                        "capabilities" to mapOf(
                            "timer_display" to appInstalled,
                            "local_reminders" to appInstalled,
                            "test_haptic" to appInstalled,
                            "remote_actions" to appInstalled,
                            "standalone" to appInstalled,
                            "complication" to false,
                        ),
                    ))
                }.addOnFailureListener { error ->
                    invoke.resolveObject(mapOf(
                        "platform" to "android", "available" to true, "paired" to false,
                        "app_installed" to appInstalled, "reachable" to false,
                        "last_synced_revision" to null, "last_queued_revision" to null,
                        "last_error" to error.message, "connection_state" to "degraded",
                        "capabilities" to emptyMap<String, Boolean>(),
                    ))
                }
            }.addOnFailureListener { error ->
            invoke.resolveObject(mapOf(
                "platform" to "android", "available" to true, "paired" to false,
                "app_installed" to false, "reachable" to false, "last_synced_revision" to null,
                "last_queued_revision" to null, "last_error" to error.message,
                "connection_state" to "degraded", "capabilities" to emptyMap<String, Boolean>(),
            ))
        }
    }

    @Command
    fun takePendingAction(invoke: Invoke) {
        invoke.resolveObject(PausIOWearRuntimeActions.take(activity) ?: "")
    }
}
