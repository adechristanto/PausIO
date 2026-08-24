package com.pausio.app.wear

import org.json.JSONObject
import java.time.Instant

/** JSON builders are pure so receipt semantics can be covered by JVM tests. */
internal object PausIOWearAcknowledgement {
    const val settingsPath = "/pausio/settings-ack/v1"
    const val testNudgePath = "/pausio/test-nudge-ack/v1"

    fun settings(result: String, revision: Long?, now: Instant = Instant.now()): ByteArray = JSONObject()
        .put("kind", "settings")
        .put("result", result)
        .put("revision", revision ?: JSONObject.NULL)
        .put("received_at", now.toString())
        .toString().toByteArray(Charsets.UTF_8)

    fun testNudge(result: String, eventId: String? = null, now: Instant = Instant.now()): ByteArray = JSONObject()
        .put("kind", "test_nudge")
        .put("result", result)
        .put("event_id", eventId ?: JSONObject.NULL)
        .put("received_at", now.toString())
        .toString().toByteArray(Charsets.UTF_8)
}
