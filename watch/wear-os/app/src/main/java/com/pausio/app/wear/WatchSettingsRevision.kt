package com.pausio.app.wear

import org.json.JSONObject
import java.time.Instant
import java.time.ZoneId

internal const val watchSchemaVersion = 1

private val requiredEnvelopeFields = setOf(
    "schema_version", "revision", "timezone", "work_interval_seconds", "short_break_seconds",
    "long_break_seconds", "pre_break_seconds", "active_days_mask", "active_start_minutes",
    "active_end_minutes", "paused", "updated_at",
)

/** The subset of timer phases a watch can render and schedule without owning the phone timer. */
internal enum class WatchTimerPhase {
    Dormant,
    Working,
    PreBreak,
    BreakDue,
    Breaking,
    Paused,
}

internal enum class WatchBreakKind { Short, Long }

internal data class WatchTimerState(
    val revision: Long,
    val timezone: ZoneId,
    val workIntervalSeconds: Long,
    val shortBreakSeconds: Long,
    val longBreakSeconds: Long,
    val preBreakSeconds: Long,
    val activeDaysMask: Int,
    val activeStartMinutes: Int,
    val activeEndMinutes: Int,
    val paused: Boolean,
    val phase: WatchTimerPhase,
    val phaseDeadlineAt: Instant?,
    val breakKind: WatchBreakKind?,
) {
    val breakDurationSeconds: Long
        get() = if (breakKind == WatchBreakKind.Long) longBreakSeconds else shortBreakSeconds
}

private data class ParsedPhase(val phase: WatchTimerPhase, val breakKind: WatchBreakKind?)

/**
 * Validates the phone-owned v1 envelope. New optional phase fields are deliberately parsed
 * permissively so watches already paired with an older phone continue to accept its payload.
 */
internal fun parseIncomingWatchSettings(payload: String): WatchTimerState? =
    parseWatchTimerState(payload, requireEnvelope = true)

/** Parses enough of an older payload for the local scheduler's backwards-compatible fallback. */
internal fun parseWatchTimerState(payload: String): WatchTimerState? =
    parseWatchTimerState(payload, requireEnvelope = false)

/** Pure contract gate so stale DataItems can be checked without a wearable runtime. */
internal fun isNewerWatchSettings(payload: String, currentRevision: Long): Boolean = try {
    val value = JSONObject(payload)
    value.optLong("revision", -1) > currentRevision
} catch (_: Exception) {
    false
}

private fun parseWatchTimerState(payload: String, requireEnvelope: Boolean): WatchTimerState? {
    return try {
        val value = JSONObject(payload)
    val schemaVersion = value.optInt("schema_version", -1)
    val revision = value.optLong("revision", -1)
    val timezone = parseTimezone(value.optString("timezone", "UTC")) ?: return null
    val workIntervalSeconds = value.optLong("work_interval_seconds", 0)
    val shortBreakSeconds = value.optLong("short_break_seconds", if (requireEnvelope) 0 else 20)
    val longBreakSeconds = value.optLong("long_break_seconds", if (requireEnvelope) 0 else 20)
    val preBreakSeconds = value.optLong("pre_break_seconds", 0)
    val activeDaysMask = value.optInt("active_days_mask", 0)
    val activeStartMinutes = value.optInt("active_start_minutes", -1)
    val activeEndMinutes = value.optInt("active_end_minutes", -1)
    val paused = value.optBoolean("paused", false)

    if (requireEnvelope && (
            !requiredEnvelopeFields.all(value::has) || schemaVersion != watchSchemaVersion || revision < 0 ||
                !value.has("updated_at") || parseInstant(value.optString("updated_at", "")) == null
        )
    ) return null
    if (workIntervalSeconds !in 300..7_200 || shortBreakSeconds !in 5..120 ||
        longBreakSeconds !in 5..3_600 || preBreakSeconds !in setOf(0L, 10L, 30L, 60L) ||
        activeDaysMask !in 1..127 || activeStartMinutes !in 0 until 1_440 ||
        activeEndMinutes !in 0 until 1_440
    ) return null

    val parsedPhase = parsePhase(value.opt("phase"), paused, value.optBoolean("break_active", false), value.opt("break_kind"))
    val deadline = parseInstant(value.optString("phase_deadline_at", ""))
        ?: parseInstant(value.optString("next_break_at", ""))
    WatchTimerState(
        revision = revision.coerceAtLeast(0),
        timezone = timezone,
        workIntervalSeconds = workIntervalSeconds,
        shortBreakSeconds = shortBreakSeconds,
        longBreakSeconds = longBreakSeconds,
        preBreakSeconds = preBreakSeconds,
        activeDaysMask = activeDaysMask,
        activeStartMinutes = activeStartMinutes,
        activeEndMinutes = activeEndMinutes,
        paused = paused || parsedPhase.phase == WatchTimerPhase.Paused,
        phase = parsedPhase.phase,
        phaseDeadlineAt = deadline,
        breakKind = parsedPhase.breakKind,
    )
    } catch (_: Exception) {
        null
    }
}

private fun parsePhase(value: Any?, paused: Boolean, breakActive: Boolean, legacyKind: Any?): ParsedPhase {
    val fallback = when {
        paused -> ParsedPhase(WatchTimerPhase.Paused, null)
        breakActive -> ParsedPhase(WatchTimerPhase.Breaking, parseBreakKind(legacyKind))
        else -> ParsedPhase(WatchTimerPhase.Working, null)
    }
    return when (value) {
        is String -> when (value.lowercase()) {
            "dormant" -> ParsedPhase(WatchTimerPhase.Dormant, null)
            "working" -> ParsedPhase(WatchTimerPhase.Working, null)
            "pre_break" -> ParsedPhase(WatchTimerPhase.PreBreak, null)
            "break_due" -> ParsedPhase(WatchTimerPhase.BreakDue, null)
            "breaking" -> ParsedPhase(WatchTimerPhase.Breaking, parseBreakKind(legacyKind))
            "paused" -> ParsedPhase(WatchTimerPhase.Paused, null)
            else -> fallback
        }
        is JSONObject -> sequenceOf(
            "break_due" to WatchTimerPhase.BreakDue,
            "breaking" to WatchTimerPhase.Breaking,
            "paused" to WatchTimerPhase.Paused,
        ).firstOrNull { value.has(it.first) }?.let { (key, phase) ->
            ParsedPhase(phase, parseBreakKind(value.opt(key)))
        } ?: fallback
        else -> fallback
    }
}

private fun parseBreakKind(value: Any?): WatchBreakKind? {
    val raw = when (value) {
        is String -> value
        is JSONObject -> value.optString("kind", "")
        else -> ""
    }
    return when (raw.lowercase()) {
        "short" -> WatchBreakKind.Short
        "long" -> WatchBreakKind.Long
        else -> null
    }
}

private fun parseInstant(value: String): Instant? = runCatching { Instant.parse(value) }.getOrNull()

private fun parseTimezone(value: String): ZoneId? = runCatching { ZoneId.of(value) }.getOrElse {
    // Older phone builds published the local %Z abbreviation. Preserve their CEST/CET payloads.
    when (value.uppercase()) {
        "CET", "CEST" -> ZoneId.of("Europe/Berlin")
        else -> null
    }
}
