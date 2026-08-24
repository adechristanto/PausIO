package com.pausio.app.wear

import android.content.Context
import com.google.android.gms.wearable.Wearable
import org.json.JSONObject
import java.time.Instant
import java.util.UUID

/** Best-effort mirror of an already-applied local control; never queued offline. */
internal object PausIOWearRuntimeAction {
    const val path = "/pausio/runtime-action/v1"

    fun publish(context: Context, action: String) {
        val payload = JSONObject()
            .put("schema_version", 1)
            .put("action_id", UUID.randomUUID().toString())
            .put("action", action)
            .put("base_revision", PausIOWearStateStore.currentRevision(context).coerceAtLeast(0))
            .put("occurred_at", Instant.now().toString())
            .toString()
            .toByteArray(Charsets.UTF_8)
        Wearable.getNodeClient(context).connectedNodes.addOnSuccessListener { nodes ->
            nodes.firstOrNull()?.let { node ->
                Wearable.getMessageClient(context).sendMessage(node.id, path, payload)
            }
        }
    }
}
