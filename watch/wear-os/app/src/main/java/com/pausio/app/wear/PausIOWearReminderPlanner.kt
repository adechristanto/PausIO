package com.pausio.app.wear

import java.time.Instant
import java.time.ZonedDateTime
import org.json.JSONObject

/** Pure next-transition calculation, kept separate so JVM tests exercise it without Android APIs. */
internal object PausIOWearReminderPlanner {
    private const val minutesPerDay = 24 * 60

    fun nextReminder(state: WatchTimerState, now: Instant): Instant? {
        if (state.paused || state.phase == WatchTimerPhase.Breaking || state.phase == WatchTimerPhase.Dormant) return null
        var candidate = state.phaseDeadlineAt?.takeIf { it.isAfter(now) } ?: now.plusSeconds(state.workIntervalSeconds)
        if (isActive(candidate.atZone(state.timezone), state.activeDaysMask, state.activeStartMinutes, state.activeEndMinutes)) return candidate

        candidate = candidate.atZone(state.timezone).withSecond(0).withNano(0).plusMinutes(1).toInstant()
        repeat(10_080) {
            if (isActive(candidate.atZone(state.timezone), state.activeDaysMask, state.activeStartMinutes, state.activeEndMinutes)) return candidate
            candidate = candidate.plusSeconds(60)
        }
        return null
    }

    /** Compatibility entry point for the shared v1 fixture tests. */
    fun nextReminder(payload: String, now: Instant): Instant? =
        parseWatchTimerState(payload)?.let { nextReminder(it, now) }

    fun preBreakSeconds(payload: String): Long = try {
        JSONObject(payload).optLong("pre_break_seconds", 0).takeIf { it in setOf(0L, 10L, 30L, 60L) } ?: 0
    } catch (_: Exception) {
        0
    }

    private fun isActive(value: ZonedDateTime, daysMask: Int, start: Int, end: Int): Boolean {
        val sundayBasedDay = value.dayOfWeek.value % 7
        if (daysMask and (1 shl sundayBasedDay) == 0) return false
        val minute = value.hour * 60 + value.minute
        return when {
            start == end -> true
            start < end -> minute in start until end
            else -> minute >= start || minute < end
        }
    }
}
