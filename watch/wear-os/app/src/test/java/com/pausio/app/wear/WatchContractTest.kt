package com.pausio.app.wear

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.time.Instant
import java.time.ZoneId

class WatchContractTest {
    @Test
    fun fixtureDecodesKnownFieldsAndToleratesFutureFields() {
        val payload = File("../../../tests/fixtures/watch-settings-v1.json").readText()
        val value = JSONObject(payload)
        assertEquals(1, value.getInt("schema_version"))
        assertEquals(7L, value.getLong("revision"))
        assertEquals("Europe/Berlin", value.getString("timezone"))
        assertEquals("ignored by v1 readers", value.getString("future_field"))
    }

    @Test
    fun malformedPayloadHasNoRevision() {
        val malformed = JSONObject("{\"timezone\":\"UTC\"}")
        assertNull(malformed.opt("revision"))
    }

    @Test
    fun staleAndMalformedRevisionsAreRejected() {
        assertTrue(isNewerWatchSettings("{\"revision\":8}", 7))
        assertFalse(isNewerWatchSettings("{\"revision\":7}", 7))
        assertFalse(isNewerWatchSettings("not json", 7))
    }

    @Test
    fun incomingEnvelopeRequiresSupportedSchemaAndCompleteSettings() {
        val fixture = File("../../../tests/fixtures/watch-settings-v1.json").readText()
        assertTrue(parseIncomingWatchSettings(fixture) != null)
        assertTrue(parseIncomingWatchSettings(envelope("\"phase\":\"working\"").replace("\"UTC\"", "\"CEST\"")) != null)
        assertNull(parseIncomingWatchSettings("{\"schema_version\":2,\"revision\":8}"))
        assertNull(parseIncomingWatchSettings("{\"schema_version\":1,\"revision\":8}"))
    }

    @Test
    fun optionalPhaseDeadlineOverridesLegacyNextBreakDeadline() {
        val payload = envelope(
            "\"phase\":\"breaking\",\"break_kind\":\"long\",\"next_break_at\":\"2026-07-26T10:20:00Z\",\"phase_deadline_at\":\"2026-07-26T10:05:00Z\"",
        )
        val state = parseIncomingWatchSettings(payload)!!
        assertEquals(WatchTimerPhase.Breaking, state.phase)
        assertEquals(WatchBreakKind.Long, state.breakKind)
        assertEquals(Instant.parse("2026-07-26T10:05:00Z"), state.phaseDeadlineAt)
    }

    @Test
    fun activeBreakNeverSchedulesAnotherBreakReminder() {
        val payload = envelope("\"phase\":\"breaking\",\"phase_deadline_at\":\"2026-07-26T10:05:00Z\"")
        assertNull(PausIOWearReminderPlanner.nextReminder(payload, Instant.parse("2026-07-26T10:00:00Z")))
    }

    @Test
    fun acknowledgementPayloadsAreMachineReadableAndTimestamped() {
        val at = Instant.parse("2026-07-26T10:00:00Z")
        val settings = JSONObject(String(PausIOWearAcknowledgement.settings("applied", 9, at), Charsets.UTF_8))
        val nudge = JSONObject(String(PausIOWearAcknowledgement.testNudge("delivered", now = at), Charsets.UTF_8))
        assertEquals("settings", settings.getString("kind"))
        assertEquals("applied", settings.getString("result"))
        assertEquals(9L, settings.getLong("revision"))
        assertEquals("test_nudge", nudge.getString("kind"))
        assertEquals("delivered", nudge.getString("result"))
        assertEquals("2026-07-26T10:00:00Z", nudge.getString("received_at"))
    }

    @Test
    fun plannerUsesNextPhoneBreakWhenItFallsInActiveHours() {
        val payload = """{"timezone":"UTC","work_interval_seconds":1200,"active_days_mask":127,"active_start_minutes":0,"active_end_minutes":0,"paused":false,"next_break_at":"2026-07-26T10:20:00Z"}"""
        assertEquals(Instant.parse("2026-07-26T10:20:00Z"), PausIOWearReminderPlanner.nextReminder(payload, Instant.parse("2026-07-26T10:00:00Z")))
    }

    @Test
    fun plannerSkipsPausedAndOutsideSchedule() {
        val paused = """{"timezone":"UTC","work_interval_seconds":1200,"active_days_mask":127,"active_start_minutes":0,"active_end_minutes":0,"paused":true}"""
        assertNull(PausIOWearReminderPlanner.nextReminder(paused, Instant.parse("2026-07-26T10:00:00Z")))

        val weekdayOnly = """{"timezone":"UTC","work_interval_seconds":3600,"active_days_mask":2,"active_start_minutes":540,"active_end_minutes":1080,"paused":false}"""
        assertEquals(Instant.parse("2026-07-27T09:00:00Z"), PausIOWearReminderPlanner.nextReminder(weekdayOnly, Instant.parse("2026-07-26T10:00:00Z")))
    }

    @Test
    fun plannerBoundsPreBreakHapticLeadTime() {
        assertEquals(30, PausIOWearReminderPlanner.preBreakSeconds("{\"pre_break_seconds\":30}"))
        assertEquals(0, PausIOWearReminderPlanner.preBreakSeconds("{\"pre_break_seconds\":120}"))
        assertEquals(0, PausIOWearReminderPlanner.preBreakSeconds("not json"))
    }

    @Test
    fun timerRingDrainsFromFullToEmpty() {
        val state = WatchTimerState(
            revision = 1,
            timezone = ZoneId.of("UTC"),
            workIntervalSeconds = 1_200,
            shortBreakSeconds = 20,
            longBreakSeconds = 300,
            preBreakSeconds = 30,
            activeDaysMask = 127,
            activeStartMinutes = 0,
            activeEndMinutes = 0,
            paused = false,
            phase = WatchTimerPhase.Working,
            phaseDeadlineAt = null,
            breakKind = null,
        )

        assertEquals(1f, timerRemainingFraction(state, 1_200), 0.001f)
        assertEquals(0.5f, timerRemainingFraction(state, 600), 0.001f)
        assertEquals(0f, timerRemainingFraction(state, 0), 0.001f)
    }

    private fun envelope(extra: String): String = """{
        "schema_version":1,"revision":8,"timezone":"UTC",
        "work_interval_seconds":1200,"short_break_seconds":20,"long_break_seconds":300,
        "pre_break_seconds":30,"active_days_mask":127,"active_start_minutes":0,
        "active_end_minutes":0,"paused":false,"updated_at":"2026-07-26T09:00:00Z",$extra
    }""".trimIndent()
}
