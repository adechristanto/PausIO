use chrono::{DateTime, Utc};
use pausio_core::{SessionCheckpoint, Settings};
use pausio_protocol::ContextReason;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::state::{ApiResult, internal_error};

// One scheduled break produces several lifecycle events. Keep enough local
// history for a full year of unusually dense work without silently truncating
// the 3-month and all-retained-data Analytics ranges.
pub(crate) const HISTORY_LIMIT: usize = 50_000;
pub(crate) const SETTINGS_PROFILES_KEY: &str = "settings_profiles";
pub(crate) const ONBOARDING_KEY: &str = "onboarding";

/// Store keys that only ever belong to a phone talking to a wearable.
///
/// Desktop never connects to a watch, but earlier desktop builds wrote these
/// anyway while reporting no watch support, so launch purges them. Naming them
/// once keeps the purge and its regression test from drifting apart.
pub(crate) const WATCH_ONLY_KEYS: [&str; 2] = ["watch_revision", "watch_last_envelope"];

static HISTORY_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Two deliberately simple local presets. They are settings snapshots, not
/// accounts, and never leave the device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SettingsProfiles {
    pub work: Option<Settings>,
    pub home: Option<Settings>,
}

pub(crate) fn profile_name_is_valid(name: &str) -> bool {
    matches!(name, "work" | "home")
}

pub(crate) fn load_settings_profiles(app: &AppHandle) -> ApiResult<SettingsProfiles> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    Ok(store
        .get(SETTINGS_PROFILES_KEY)
        .map(|mut value| {
            // Profiles store whole settings snapshots, so a profile saved
            // before the unified sound model needs the same migration as the
            // live settings document.
            if let Some(object) = value.as_object_mut() {
                for key in ["work", "home"] {
                    if let Some(profile) = object.get_mut(key).and_then(|v| v.as_object_mut()) {
                        let (migrated, _) = migrate_sound_timing(std::mem::replace(
                            profile,
                            serde_json::Map::new(),
                        ));
                        *profile = migrated;
                    }
                }
            }
            value
        })
        .and_then(|value| serde_json::from_value::<SettingsProfiles>(value).ok())
        .unwrap_or_default())
}

/// One-time migration into the unified sound model: derives `sound_timing`
/// from the retired `notification_sound` / `sound_theme` pair in a stored
/// settings document that predates the field, and drops the retired keys so
/// the next save is clean. Returns the (possibly rewritten) document and
/// whether it changed. Idempotent — a document that already carries
/// `sound_timing` is returned untouched.
pub(crate) fn migrate_sound_timing(
    mut object: serde_json::Map<String, serde_json::Value>,
) -> (serde_json::Map<String, serde_json::Value>, bool) {
    if object.contains_key("sound_timing") {
        return (object, false);
    }
    let banner = object
        .get("notification_sound")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    // A missing sound_theme predates even that field; its default was an
    // audible break-end cue, matching "end".
    let end = object
        .get("sound_theme")
        .and_then(|value| value.as_str())
        .map(|theme| theme != "silence")
        .unwrap_or(true);
    let timing = match (banner, end) {
        (true, true) => "both",
        (true, false) => "banner",
        (false, true) => "end",
        (false, false) => "silent",
    };
    object.insert(
        "sound_timing".to_string(),
        serde_json::Value::String(timing.to_string()),
    );
    object.remove("notification_sound");
    object.remove("sound_theme");
    object.remove("sound_volume");
    (object, true)
}

pub(crate) fn save_settings_profiles(
    app: &AppHandle,
    profiles: &SettingsProfiles,
) -> ApiResult<()> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    store.set(
        SETTINGS_PROFILES_KEY,
        serde_json::to_value(profiles).map_err(internal_error)?,
    );
    store.save().map_err(internal_error)
}

/// A person who has already found and used Settings does not need a guided
/// tour, so this gates a one-time first-run flow rather than anything ongoing.
/// Missing or unreadable reads as "not yet shown" -- the safer direction, since
/// showing it twice costs a Skip click and hiding it once costs the entire
/// feature's purpose.
pub(crate) fn onboarding_completed(app: &AppHandle) -> ApiResult<bool> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    Ok(store
        .get(ONBOARDING_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

pub(crate) fn mark_onboarding_completed(app: &AppHandle) -> ApiResult<()> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    store.set(ONBOARDING_KEY, serde_json::Value::Bool(true));
    store.save().map_err(internal_error)
}

/// PausIO's local activity history deliberately contains only its own timer
/// decisions. It is never populated with active-app names, URLs, titles,
/// input, display pixels, microphone, or camera information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct HistoryEvent {
    #[serde(default = "history_schema_version")]
    pub schema_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub kind: HistoryEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub break_kind: Option<pausio_protocol::BreakKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_break_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_interval_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HistoryEventKind {
    Due,
    Started,
    Completed,
    /// A break ended early by explicit person action, as opposed to
    /// `Completed` (ran its course, including while the person was away).
    Skipped,
    Postponed,
    Deferred,
}

pub(crate) const fn history_schema_version() -> u8 {
    4
}

pub(crate) fn persist_settings(app: &AppHandle, settings: &Settings) -> ApiResult<()> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    let value = serde_json::to_value(settings).map_err(internal_error)?;
    store.set("settings", value);
    store.save().map_err(internal_error)
}

/// Checkpoints contain only timer state and transient-free context flags. They
/// intentionally never store app/window names, input, screen data, audio, or
/// camera content.
pub(crate) fn persist_session(app: &AppHandle, checkpoint: &SessionCheckpoint) -> ApiResult<()> {
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    let value = serde_json::to_value(checkpoint).map_err(internal_error)?;
    store.set("session", value);
    store.save().map_err(internal_error)
}

/// The pure part of `append_history` (append, retain, cap) -- pulled out so it
/// can be exercised and timed without a running Tauri store. See
/// `tests::append_history_stays_fast_at_the_history_cap` below: before
/// changing this to something other than an in-memory rewrite (e.g. an
/// append-only log), benchmark it at realistic sizes first, per the product
/// audit's own guidance not to optimize speculatively.
pub(crate) fn apply_retention_and_cap(
    mut history: Vec<HistoryEvent>,
    entries: Vec<HistoryEvent>,
    retention_days: Option<u16>,
) -> Vec<HistoryEvent> {
    history.extend(entries);
    if let Some(days) = retention_days {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        history.retain(|event| event.occurred_at >= cutoff);
    }
    let drain = history.len().saturating_sub(HISTORY_LIMIT);
    if drain > 0 {
        history.drain(..drain);
    }
    history
}

/// Deserializes the stored history array leniently: one malformed record (a
/// hand-edited file, a future schema this build does not understand, a
/// partial write truncated mid-record) must never discard every other event.
/// Only a value that is not a JSON array at all — i.e. the whole key is
/// unusable — falls back to empty; everything else keeps every record that
/// parses and silently drops the rest.
pub(crate) fn parse_history_leniently(value: serde_json::Value) -> Vec<HistoryEvent> {
    let serde_json::Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<HistoryEvent>(item).ok())
        .collect()
}

pub(crate) fn append_history(
    app: &AppHandle,
    entries: Vec<HistoryEvent>,
    retention_days: Option<u16>,
) -> ApiResult<()> {
    let store = app.store(history_store_name()).map_err(internal_error)?;
    let history = store
        .get("history")
        .map(parse_history_leniently)
        .unwrap_or_default();
    let history = apply_retention_and_cap(history, entries, retention_days);
    store.set(
        "history",
        serde_json::to_value(history).map_err(internal_error)?,
    );
    store.save().map_err(internal_error)
}

pub(crate) fn next_history_break_id() -> String {
    let sequence = HISTORY_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!(
        "break-{}-{sequence}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

pub(crate) fn history_event(
    app: &AppHandle,
    event: &pausio_core::EngineEvent,
    settings: &Settings,
) -> Option<HistoryEvent> {
    use pausio_core::EngineEvent;
    let (kind, break_kind, context) = match event {
        EngineEvent::Due(kind) => (HistoryEventKind::Due, Some(kind.clone()), None),
        EngineEvent::Started(kind) => (HistoryEventKind::Started, Some(kind.clone()), None),
        EngineEvent::Ended(kind) => (HistoryEventKind::Completed, Some(kind.clone()), None),
        EngineEvent::Skipped(kind) => (HistoryEventKind::Skipped, Some(kind.clone()), None),
        EngineEvent::Postponed(kind) => (HistoryEventKind::Postponed, Some(kind.clone()), None),
        EngineEvent::ContextDeferred { kind, reason } => (
            HistoryEventKind::Deferred,
            Some(kind.clone()),
            Some(reason.clone()),
        ),
        EngineEvent::Incoming(_)
        | EngineEvent::StateChanged(_)
        | EngineEvent::Tick(_)
        | EngineEvent::BlinkNudge
        | EngineEvent::PostureNudge
        | EngineEvent::HydrationNudge => {
            return None;
        }
    };
    let tracker = app.try_state::<crate::state::HistoryTracker>()?;
    let mut current = tracker
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let break_id = history_break_id(&mut current, event);
    let target_break_seconds = break_kind.as_ref().map(|kind| match kind {
        pausio_protocol::BreakKind::Short => settings.short_break_seconds,
        pausio_protocol::BreakKind::Long => settings.long_break_seconds,
    });
    Some(HistoryEvent {
        schema_version: history_schema_version(),
        break_id,
        occurred_at: Utc::now(),
        kind,
        break_kind,
        context,
        target_break_seconds,
        work_interval_seconds: Some(settings.work_seconds),
        schedule_fingerprint: Some(format!(
            "{}:{}:{}:{}:{}",
            settings.work_seconds,
            settings.short_break_seconds,
            settings.long_break_seconds,
            settings.active_start_minutes,
            settings.active_end_minutes
        )),
    })
}

fn history_break_id(
    current: &mut Option<String>,
    event: &pausio_core::EngineEvent,
) -> Option<String> {
    use pausio_core::EngineEvent;
    match event {
        EngineEvent::Due(_) | EngineEvent::ContextDeferred { .. } => {
            current.clone().or_else(|| {
                let id = next_history_break_id();
                *current = Some(id.clone());
                Some(id)
            })
        }
        EngineEvent::Started(_) => {
            if current.is_none() {
                *current = Some(next_history_break_id());
            }
            current.clone()
        }
        EngineEvent::Postponed(_) => current.clone(),
        EngineEvent::Ended(_) | EngineEvent::Skipped(_) => current.take(),
        _ => None,
    }
}

pub(crate) fn settings_store_name() -> &'static str {
    if crate::is_e2e() {
        "pausio-e2e-settings.json"
    } else {
        "pausio-settings.json"
    }
}

/// History lives in its own store file, separate from settings/session. It is
/// by far the largest thing PausIO persists (up to `HISTORY_LIMIT` events),
/// and it never needs to be rewritten by the frequent settings/session saves
/// that touch the other store — keeping it separate means a plain heartbeat
/// checkpoint stays a tiny write instead of rewriting the whole history array.
pub(crate) fn history_store_name() -> &'static str {
    if crate::is_e2e() {
        "pausio-e2e-history.json"
    } else {
        "pausio-history.json"
    }
}

#[cfg(test)]
mod tests {
    use pausio_core::EngineEvent;
    use pausio_protocol::BreakKind;

    use super::history_break_id;
    use super::{
        HISTORY_LIMIT, HistoryEvent, HistoryEventKind, apply_retention_and_cap,
        parse_history_leniently,
    };

    fn synthetic_history(count: usize) -> Vec<HistoryEvent> {
        let now = chrono::Utc::now();
        (0..count)
            .map(|i| HistoryEvent {
                schema_version: super::history_schema_version(),
                break_id: Some(format!("break-{i}")),
                occurred_at: now - chrono::Duration::seconds(i as i64),
                kind: HistoryEventKind::Completed,
                break_kind: Some(BreakKind::Short),
                context: None,
                target_break_seconds: Some(20),
                work_interval_seconds: Some(1200),
                schedule_fingerprint: Some("1200:20".into()),
            })
            .collect()
    }

    /// Not a correctness test -- a recorded timing. `append_history` currently does
    /// a full in-memory load/extend/retain/drain/rewrite on every append, up to
    /// `HISTORY_LIMIT` (50,000) events. This exercises that pipeline (minus the
    /// Tauri store I/O itself) at the cap and asserts it stays comfortably
    /// fast, so a future change to something like an append-only log is a
    /// decision backed by a measurement, not a guess. If this ever approaches
    /// the threshold, that's the signal to actually design pagination --
    /// don't preemptively build it before this test says it's warranted.
    #[test]
    fn append_history_stays_fast_at_the_history_cap() {
        let existing = synthetic_history(HISTORY_LIMIT);
        let incoming = synthetic_history(5);
        let stored_json = serde_json::to_value(&existing).expect("serialize fixture");

        // Times the same round trip `append_history` performs against the real
        // Tauri store: deserialize what's on disk, append+retain+cap, re-serialize.
        // The store's own read/write I/O is not included -- that's a constant,
        // separately-measurable cost, not what a pagination redesign would change.
        let started = std::time::Instant::now();
        let loaded: Vec<HistoryEvent> =
            serde_json::from_value(stored_json).expect("deserialize fixture");
        let result = apply_retention_and_cap(loaded, incoming, None);
        let _reserialized = serde_json::to_value(&result).expect("reserialize result");
        let elapsed = started.elapsed();

        assert_eq!(result.len(), HISTORY_LIMIT);
        // Generous on purpose: this is a regression guard against an algorithmic
        // change (e.g. accidentally going quadratic), not a strict perf budget --
        // a shared/contended CI runner can be several times slower than a local
        // machine for the same code, and this happens off the UI thread besides.
        assert!(
            elapsed < std::time::Duration::from_millis(2500),
            "the deserialize/append/retain/cap/reserialize round trip took {elapsed:?} at \
             the {HISTORY_LIMIT}-event cap -- investigate before assuming pagination is \
             needed; do not optimize speculatively without a measurement like this one"
        );
    }

    #[test]
    fn postponement_keeps_one_break_id_until_resolution() {
        let mut current = None;
        let due = history_break_id(&mut current, &EngineEvent::Due(BreakKind::Short)).unwrap();
        let postponed =
            history_break_id(&mut current, &EngineEvent::Postponed(BreakKind::Short)).unwrap();
        let resurfaced =
            history_break_id(&mut current, &EngineEvent::Due(BreakKind::Short)).unwrap();
        let started =
            history_break_id(&mut current, &EngineEvent::Started(BreakKind::Short)).unwrap();
        let completed =
            history_break_id(&mut current, &EngineEvent::Ended(BreakKind::Short)).unwrap();

        assert_eq!(due, postponed);
        assert_eq!(due, resurfaced);
        assert_eq!(due, started);
        assert_eq!(due, completed);
        assert!(current.is_none());
    }

    #[test]
    fn manual_break_keeps_an_id_until_resolution() {
        let mut current = None;
        let started =
            history_break_id(&mut current, &EngineEvent::Started(BreakKind::Short)).unwrap();
        let completed =
            history_break_id(&mut current, &EngineEvent::Ended(BreakKind::Short)).unwrap();

        assert_eq!(started, completed);
        assert!(current.is_none());
    }

    /// The desktop purge in `run()` iterates `WATCH_ONLY_KEYS`, so these are
    /// the exact keys a desktop store is guaranteed not to retain. Serialized
    /// desktop settings must also never contain them.
    #[test]
    fn watch_only_keys_are_never_part_of_desktop_settings() {
        let serialized =
            serde_json::to_value(pausio_core::Settings::default()).expect("serialize settings");
        let object = serialized.as_object().expect("settings serialize to a map");
        for key in super::WATCH_ONLY_KEYS {
            assert!(
                !object.contains_key(key),
                "{key} is watch transport state and must never live in settings"
            );
        }
        assert_eq!(
            super::WATCH_ONLY_KEYS,
            ["watch_revision", "watch_last_envelope"],
            "changing these names requires updating the desktop purge in run()"
        );
    }

    #[test]
    fn enriched_history_events_capture_schedule_without_private_activity_data() {
        let settings = pausio_core::Settings::default();
        let mut current = None;
        let id = history_break_id(&mut current, &EngineEvent::Due(BreakKind::Short));
        let kind = Some(BreakKind::Short);
        let target_break_seconds = kind.as_ref().map(|kind| match kind {
            BreakKind::Short => settings.short_break_seconds,
            BreakKind::Long => settings.long_break_seconds,
        });

        assert!(id.is_some());
        assert_eq!(target_break_seconds, Some(settings.short_break_seconds));
        assert!(settings.work_seconds > 0);
    }

    /// One record failing to deserialize (a hand-edited file, a future schema
    /// this build predates, a value truncated by a partial write) must not
    /// discard the rest of a person's history. This is the exact bug this
    /// function fixes: `serde_json::from_value::<Vec<HistoryEvent>>` fails the
    /// whole array the moment any one element is invalid.
    #[test]
    fn one_malformed_record_does_not_discard_the_rest_of_the_history() {
        let good = synthetic_history(3);
        let mut array: Vec<serde_json::Value> = good
            .iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect();
        // Not a HistoryEvent at all: `kind` is neither a known variant nor
        // present, so this element fails to deserialize on its own.
        array.insert(1, serde_json::json!({ "not": "a history event" }));

        let recovered = parse_history_leniently(serde_json::Value::Array(array));

        assert_eq!(recovered.len(), 3);
        assert_eq!(
            recovered
                .iter()
                .map(|e| e.break_id.clone())
                .collect::<Vec<_>>(),
            good.iter().map(|e| e.break_id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_non_array_history_value_reads_as_empty_rather_than_panicking() {
        assert!(parse_history_leniently(serde_json::json!("not an array")).is_empty());
        assert!(parse_history_leniently(serde_json::json!(null)).is_empty());
        assert!(parse_history_leniently(serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_fully_valid_history_array_round_trips_unchanged() {
        let good = synthetic_history(5);
        let value = serde_json::to_value(&good).unwrap();

        let recovered = parse_history_leniently(value);

        assert_eq!(recovered.len(), good.len());
    }
}
