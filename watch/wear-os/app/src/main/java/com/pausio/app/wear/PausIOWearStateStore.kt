package com.pausio.app.wear

import android.content.Context
import org.json.JSONObject
import java.time.Instant
import java.time.ZoneId

/** Keeps a small autonomous timer on the watch until a newer phone revision replaces it. */
internal object PausIOWearStateStore {
    private const val preferencesName = "pausio"
    private const val settingsKey = "settings"
    private const val revisionKey = "revision"
    private const val localPhaseKey = "local_phase"
    private const val localDeadlineKey = "local_deadline_at"
    private const val locallyPausedKey = "locally_paused"

    fun currentRevision(context: Context): Long = preferences(context).getLong(revisionKey, -1)

    fun read(context: Context, now: Instant = Instant.now()): WatchTimerState {
        val preferences = preferences(context)
        val payload = preferences.getString(settingsKey, null) ?: defaultPayload(now).also {
            preferences.edit().putString(settingsKey, it).putLong(localDeadlineKey, now.plusSeconds(1_200).toEpochMilli()).apply()
        }
        val base = parseWatchTimerState(payload) ?: parseWatchTimerState(defaultPayload(now))!!
        val localPhase = preferences.getString(localPhaseKey, null)?.let(::parseLocalPhase)
        val localDeadline = preferences.takeIf { it.contains(localDeadlineKey) }
            ?.getLong(localDeadlineKey, 0)?.takeIf { it > 0 }?.let(Instant::ofEpochMilli)
        return base.copy(
            paused = base.paused || preferences.getBoolean(locallyPausedKey, false),
            phase = localPhase ?: base.phase,
            phaseDeadlineAt = localDeadline ?: base.phaseDeadlineAt,
        )
    }

    /** Acknowledgements are sent only after this synchronous, durable save succeeds. */
    fun saveIncoming(context: Context, payload: String, state: WatchTimerState): Boolean = preferences(context).edit()
        .putString(settingsKey, payload)
        .putLong(revisionKey, state.revision)
        .remove(localPhaseKey)
        .remove(localDeadlineKey)
        .remove(locallyPausedKey)
        .commit()

    fun setLocallyPaused(context: Context, paused: Boolean) {
        preferences(context).edit().putBoolean(locallyPausedKey, paused).apply()
    }

    fun isLocallyPaused(context: Context): Boolean = preferences(context).getBoolean(locallyPausedKey, false)

    fun beginLocalBreak(context: Context, now: Instant = Instant.now()) {
        val state = read(context, now)
        saveLocalTimer(context, WatchTimerPhase.Breaking, now.plusSeconds(state.breakDurationSeconds))
    }

    fun finishLocalBreak(context: Context, now: Instant = Instant.now()) {
        val state = read(context, now)
        saveLocalTimer(context, WatchTimerPhase.Working, now.plusSeconds(state.workIntervalSeconds))
    }

    private fun saveLocalTimer(context: Context, phase: WatchTimerPhase, deadline: Instant) {
        preferences(context).edit()
            .putString(localPhaseKey, phase.name)
            .putLong(localDeadlineKey, deadline.toEpochMilli())
            .apply()
    }

    private fun preferences(context: Context) = context.getSharedPreferences(preferencesName, Context.MODE_PRIVATE)

    private fun parseLocalPhase(value: String): WatchTimerPhase? = runCatching { WatchTimerPhase.valueOf(value) }.getOrNull()

    private fun defaultPayload(now: Instant): String = JSONObject()
        .put("schema_version", watchSchemaVersion)
        .put("revision", 0)
        .put("timezone", ZoneId.systemDefault().id)
        .put("work_interval_seconds", 1_200)
        .put("short_break_seconds", 20)
        .put("long_break_seconds", 20)
        .put("pre_break_seconds", 0)
        .put("active_days_mask", 127)
        .put("active_start_minutes", 0)
        .put("active_end_minutes", 0)
        .put("paused", false)
        .put("updated_at", now.toString())
        .put("phase", "working")
        .put("phase_deadline_at", now.plusSeconds(1_200).toString())
        .toString()
}
