package com.pausio.app.wear

import com.google.android.gms.wearable.DataEvent
import com.google.android.gms.wearable.DataEventBuffer
import com.google.android.gms.wearable.DataMapItem
import com.google.android.gms.wearable.PutDataMapRequest
import com.google.android.gms.wearable.MessageEvent
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import org.json.JSONObject

class PausIOWearListenerService : WearableListenerService() {
    override fun onDataChanged(events: DataEventBuffer) {
        events.use { buffer ->
            for (event in buffer) {
                if (event.type != DataEvent.TYPE_CHANGED || event.dataItem.uri.path != "/pausio/settings/v1") continue
                val sourceNodeId = event.dataItem.uri.host
                val payload = runCatching { DataMapItem.fromDataItem(event.dataItem).dataMap.getString("payload") }.getOrNull()
                val state = payload?.let(::parseIncomingWatchSettings)
                val result = when {
                    state == null -> "invalid"
                    !isNewerWatchSettings(payload!!, PausIOWearStateStore.currentRevision(this)) -> "stale"
                    PausIOWearStateStore.saveIncoming(this, payload!!, state) -> {
                        PausIOWearReminderScheduler.replace(this)
                        "applied"
                    }
                    else -> "storage_failed"
                }
                if (!sourceNodeId.isNullOrBlank()) {
                    sendAcknowledgement(sourceNodeId, PausIOWearAcknowledgement.settingsPath, PausIOWearAcknowledgement.settings(result, state?.revision))
                }
            }
        }
    }

    override fun onMessageReceived(messageEvent: MessageEvent) {
        if (messageEvent.path != "/pausio/test-nudge/v1") return
        val body = String(messageEvent.data, Charsets.UTF_8)
        val testEvent = runCatching { JSONObject(body) }.getOrNull()
        val valid = body.isBlank() || testEvent?.optString("kind", "test") == "test"
        val eventId = testEvent?.optString("event_id", "")?.takeIf { it.isNotBlank() }
        if (valid) {
            getSharedPreferences("pausio", MODE_PRIVATE).edit().putLong("last_nudge", System.currentTimeMillis()).apply()
            PausIOWearReminderScheduler.vibrate(this)
        }
        sendAcknowledgement(
            messageEvent.sourceNodeId,
            PausIOWearAcknowledgement.testNudgePath,
            PausIOWearAcknowledgement.testNudge(if (valid) "delivered" else "invalid", eventId),
        )
    }

    private fun sendAcknowledgement(sourceNodeId: String, path: String, payload: ByteArray) {
        // The Data Item persists the latest acknowledgement across a disconnected
        // phone; the message is only a low-latency hint for an open setup screen.
        val durable = PutDataMapRequest.create(path).apply {
            dataMap.putByteArray("payload", payload)
        }.asPutDataRequest().setUrgent()
        Wearable.getDataClient(this).putDataItem(durable)
        if (sourceNodeId.isNotBlank()) {
            Wearable.getMessageClient(this).sendMessage(sourceNodeId, path, payload)
        }
    }
}
