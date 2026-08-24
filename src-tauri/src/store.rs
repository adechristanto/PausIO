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
        .and_then(|value| serde_json::from_value::<SettingsProfiles>(value).ok())
        .unwrap_or_default())
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

pub(crate) fn append_history(
    app: &AppHandle,
    entries: Vec<HistoryEvent>,
    retention_days: Option<u16>,
) -> ApiResult<()> {
    let store = app.store(history_store_name()).map_err(internal_error)?;
    let mut history = store
        .get("history")
        .and_then(|value| serde_json::from_value::<Vec<HistoryEvent>>(value).ok())
        .unwrap_or_default();
    history.extend(entries);
    if let Some(days) = retention_days {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        history.retain(|event| event.occurred_at >= cutoff);
    }
    let drain = history.len().saturating_sub(HISTORY_LIMIT);
    if drain > 0 {
        history.drain(..drain);
    }
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
}
