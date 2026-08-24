package com.pausio.app.eyecare

import android.content.Context
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.DataEvent
import com.google.android.gms.wearable.DataEventBuffer
import com.google.android.gms.wearable.DataMapItem
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject

/** Receipts are small, redacted transport facts: revision/result, never timer content. */
class PausIOWearReceiptListenerService : WearableListenerService() {
    override fun onMessageReceived(messageEvent: MessageEvent) {
        val payload = runCatching { JSONObject(String(messageEvent.data, Charsets.UTF_8)) }.getOrNull() ?: return
        consume(messageEvent.path, payload)
    }

    override fun onDataChanged(events: DataEventBuffer) {
        events.use { buffer ->
            for (event in buffer) {
                if (event.type != DataEvent.TYPE_CHANGED) continue
                val payload = runCatching {
                    val raw = DataMapItem.fromDataItem(event.dataItem).dataMap.getByteArray("payload")
                    JSONObject(raw?.toString(Charsets.UTF_8) ?: DataMapItem.fromDataItem(event.dataItem).dataMap.getString("payload") ?: "")
                }.getOrNull() ?: continue
                consume(event.dataItem.uri.path, payload)
            }
        }
    }

    private fun consume(path: String?, payload: JSONObject) {
        val preferences = getSharedPreferences("pausio.wear.bridge", Context.MODE_PRIVATE)
        when (path) {
            settingsAcknowledgementPath -> {
                val revision = payload.optLong("revision", -1)
                val result = payload.optString("result", "invalid")
                preferences.edit().apply {
                    if (result == "applied" && revision >= 0) putLong("last_applied_revision", revision)
                    putString("last_error", if (result == "applied") null else "watch_settings_$result")
                }.apply()
            }
            testNudgeAcknowledgementPath -> {
                val eventId = payload.optString("event_id", "")
                val result = payload.optString("result", "unavailable")
                preferences.edit().putString(
                    "last_error",
                    if (result == "delivered") null else "watch_test_nudge_$result",
                ).apply()
                if (eventId.isNotBlank()) PausIOWearTestNudgeReplies.complete(eventId, result)
            }
            runtimeActionPath -> PausIOWearRuntimeActions.enqueue(this, payload)
            watchHealthPath -> preferences.edit()
                .putString("watch_health", payload.toString())
                .putString("last_error", payload.optString("last_error", "").ifBlank { null })
                .apply()
        }
    }
}
