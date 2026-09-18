use chrono::Local;
use pausio_core::{EngineError, EngineEvent, Settings, Snapshot, TimerEngine};
use pausio_protocol::{ContextReason, PauseReason};
#[cfg(mobile)]
use pausio_protocol::{
    NudgeResult, ReminderScheduleReport, WatchPermissionState, WatchSettingsEnvelopeV1, WatchStatus,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

#[cfg(mobile)]
use crate::state::platform_unavailable;
use crate::state::{ApiError, ApiResult, EngineState, drain_and_emit, internal_error, lock_engine};
use crate::store::{
    HistoryEvent, SettingsProfiles, history_store_name, load_settings_profiles, persist_settings,
    profile_name_is_valid, save_settings_profiles, settings_store_name,
};

#[derive(Debug, Serialize)]
pub(crate) struct AutostartStatus {
    pub supported: bool,
    pub enabled: bool,
}

/// A deliberately redacted local health summary. It reports platform support
/// and PausIO configuration, never apps, windows, input, displays names, or
/// user content.
#[derive(Debug, Serialize)]
pub(crate) struct DesktopHealth {
    pub platform: String,
    pub notification_permission: String,
    pub display_count: usize,
    pub autostart_supported: bool,
    pub autostart_enabled: bool,
    pub history_enabled: bool,
    pub history_retention_days: Option<u16>,
    pub display_target: pausio_core::DisplayTarget,
    /// Whether this platform can automatically detect fullscreen apps.
    /// Windows and macOS (`CGWindowListCopyWindowInfo`, permission-free);
    /// Linux reports `false` honestly.
    pub auto_context_fullscreen_supported: bool,
    /// Whether this platform can automatically detect Do Not Disturb / Focus.
    /// Windows only (`SHQueryUserNotificationState`) -- macOS has no public,
    /// permission-free API for this, and Linux reports `false` honestly. Kept
    /// separate from `auto_context_fullscreen_supported` so a platform that
    /// supports one signal but not the other (macOS) is not forced to claim
    /// or hide both together.
    pub auto_context_dnd_supported: bool,
}

/// UI commands and tray callbacks can arrive on macOS's main event loop. Window creation
/// and menu mutation may synchronously post work to that same loop, so perform every timer
/// transition on Tauri's blocking worker pool instead. Otherwise a tray-triggered break can
/// wait for the loop that is currently running the callback, leaving the application spinning.
pub(crate) fn apply_engine_transition<F>(app: &AppHandle, transition: F) -> ApiResult<Snapshot>
where
    F: FnOnce(&mut TimerEngine) -> Result<Vec<EngineEvent>, EngineError>,
{
    let engine = app.state::<EngineState>();
    let mut guard = lock_engine(&engine.0);
    let events = transition(&mut guard)?;
    let view = drain_and_emit(app, guard, events);
    Ok(view.snapshot)
}

pub(crate) async fn apply_engine_transition_async<F>(
    app: AppHandle,
    transition: F,
) -> ApiResult<Snapshot>
where
    F: FnOnce(&mut TimerEngine) -> Result<Vec<EngineEvent>, EngineError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || apply_engine_transition(&app, transition))
        .await
        .map_err(internal_error)?
}

pub(crate) fn spawn_engine_transition<F>(app: AppHandle, transition: F)
where
    F: FnOnce(&mut TimerEngine) -> Result<Vec<EngineEvent>, EngineError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let _ = apply_engine_transition(&app, transition);
    });
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn start_session(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::start_session).await
}

#[tauri::command]
pub(crate) async fn start_due_break(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::start_due_break).await
}

#[tauri::command]
pub(crate) fn get_state(engine: State<'_, EngineState>) -> Snapshot {
    lock_engine(&engine.0).snapshot()
}

#[tauri::command]
pub(crate) fn get_settings(engine: State<'_, EngineState>) -> Settings {
    lock_engine(&engine.0).settings().clone()
}

#[tauri::command]
pub(crate) fn get_settings_profiles(app: AppHandle) -> ApiResult<SettingsProfiles> {
    load_settings_profiles(&app)
}

#[tauri::command]
pub(crate) fn get_onboarding_state(app: AppHandle) -> ApiResult<bool> {
    crate::store::onboarding_completed(&app)
}

#[tauri::command]
pub(crate) fn complete_onboarding(app: AppHandle) -> ApiResult<()> {
    crate::store::mark_onboarding_completed(&app)
}

#[tauri::command]
pub(crate) fn save_settings_profile(
    app: AppHandle,
    engine: State<'_, EngineState>,
    name: String,
) -> ApiResult<SettingsProfiles> {
    if !profile_name_is_valid(&name) {
        return Err(internal_error("unknown settings profile"));
    }
    let mut profiles = load_settings_profiles(&app)?;
    let settings = lock_engine(&engine.0).settings().clone();
    match name.as_str() {
        "work" => profiles.work = Some(settings),
        "home" => profiles.home = Some(settings),
        _ => unreachable!(),
    }
    save_settings_profiles(&app, &profiles)?;
    Ok(profiles)
}

#[tauri::command]
pub(crate) async fn apply_settings_profile(app: AppHandle, name: String) -> ApiResult<Settings> {
    // `drain_and_emit` can update native tray/window state. It must never run
    // from Tauri's UI thread: another publisher may be waiting for that same
    // thread while holding publication order. Execute the whole durable
    // transition on the blocking pool, as the timer controls already do.
    tauri::async_runtime::spawn_blocking(move || apply_settings_profile_blocking(&app, name))
        .await
        .map_err(internal_error)?
}

fn apply_settings_profile_blocking(app: &AppHandle, name: String) -> ApiResult<Settings> {
    if !profile_name_is_valid(&name) {
        return Err(internal_error("unknown settings profile"));
    }
    let profiles = load_settings_profiles(app)?;
    let settings = match name.as_str() {
        "work" => profiles.work,
        "home" => profiles.home,
        _ => unreachable!(),
    }
    .ok_or_else(|| internal_error("save this profile before applying it"))?;
    // Applying a profile is an ordinary settings write and must take the ordinary
    // path. This used to be a near-copy of `set_settings_blocking` that omitted
    // `retranslate_tray` and `sync_global_shortcuts`, so a profile carrying a
    // different locale or different accelerators left the tray in the old language
    // and the old shortcuts still registered until the next unrelated save. It also
    // never emitted `settings:changed`.
    set_settings_blocking(app, settings)
}

#[tauri::command]
pub(crate) async fn set_settings(app: AppHandle, settings: Settings) -> ApiResult<Settings> {
    tauri::async_runtime::spawn_blocking(move || set_settings_blocking(&app, settings))
        .await
        .map_err(internal_error)?
}

fn set_settings_blocking(app: &AppHandle, settings: Settings) -> ApiResult<Settings> {
    // Validate and commit durable state before mutating the live engine. A store failure
    // therefore leaves both the current session and the next launch on the old settings.
    settings.validate().map_err(EngineError::from)?;
    persist_settings(app, &settings)?;
    let engine = app.state::<EngineState>();
    let mut guard = lock_engine(&engine.0);
    let events = guard.replace_settings(settings.clone(), Local::now())?;
    drain_and_emit(app, guard, events);
    // A saved mobile setting becomes the new durable watch context via
    // drain_and_emit's sync_watch_state call. Pairing is best effort; an
    // unpaired watch must not roll back a successfully validated local save.
    #[cfg(desktop)]
    {
        crate::tray_menu::retranslate_tray(settings.locale);
        sync_global_shortcuts(app, &settings);
    }
    let _ = tauri::Emitter::emit(app, "settings:changed", settings.clone());
    Ok(settings)
}

#[tauri::command]
pub(crate) async fn set_context(
    app: AppHandle,
    context: Option<ContextReason>,
    duration_minutes: Option<u16>,
) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, move |engine| match (context, duration_minutes) {
        (Some(context), Some(minutes)) => engine.set_context_for(context, minutes),
        (context, _) => Ok(engine.set_context(context)),
    })
    .await
}

#[tauri::command]
pub(crate) fn get_history(app: AppHandle) -> ApiResult<Vec<HistoryEvent>> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(history_store_name()).map_err(internal_error)?;
    Ok(store
        .get("history")
        .map(crate::store::parse_history_leniently)
        .unwrap_or_default())
}

#[tauri::command]
pub(crate) fn clear_history(app: AppHandle) -> ApiResult<()> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(history_store_name()).map_err(internal_error)?;
    store.delete("history");
    store.save().map_err(internal_error)?;
    if let Some(tracker) = app.try_state::<crate::state::HistoryTracker>() {
        *tracker
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn export_history(app: AppHandle, format: String) -> ApiResult<String> {
    let events = get_history(app)?;
    format_history_export(&events, &format).ok_or_else(|| {
        // format_history_export only returns None for an unrecognized format
        // string; a real serialization failure from serde_json::Error takes
        // the Err(String) branch below instead and is surfaced as-is.
        internal_error("history export format must be json or csv")
    })?
}

/// Pure history-export formatting, split out from [`export_history`] so the
/// CSV escaping and column layout can be unit tested without a Tauri store.
/// Returns `None` for an unrecognized `format`; `Some(Err(_))` only for a
/// genuine `serde_json` serialization failure on the "json" path.
fn format_history_export(events: &[HistoryEvent], format: &str) -> Option<ApiResult<String>> {
    match format {
        "json" => Some(serde_json::to_string_pretty(events).map_err(internal_error)),
        "csv" => Some(Ok(history_events_to_csv(events))),
        _ => None,
    }
}

fn history_events_to_csv(events: &[HistoryEvent]) -> String {
    let mut output = String::from(
        "schema_version,break_id,occurred_at,kind,break_kind,context,target_break_seconds,work_interval_seconds,schedule_fingerprint\n",
    );
    for event in events {
        let escape = |value: String| format!("\"{}\"", value.replace('"', "\"\""));
        output.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            event.schema_version,
            escape(event.break_id.clone().unwrap_or_default()),
            escape(event.occurred_at.to_rfc3339()),
            escape(format!("{:?}", event.kind).to_lowercase()),
            escape(
                event
                    .break_kind
                    .as_ref()
                    .map(|kind| format!("{kind:?}").to_lowercase())
                    .unwrap_or_default()
            ),
            escape(
                event
                    .context
                    .as_ref()
                    .map(|context| format!("{context:?}").to_lowercase())
                    .unwrap_or_default()
            ),
            event
                .target_break_seconds
                .map(|value| value.to_string())
                .unwrap_or_default(),
            event
                .work_interval_seconds
                .map(|value| value.to_string())
                .unwrap_or_default(),
            escape(event.schedule_fingerprint.clone().unwrap_or_default()),
        ));
    }
    output
}

/// Erases PausIO's durable state on this device. This intentionally does not
/// claim to erase a separately paired watch; that device owns its own local
/// schedule and requires its own reset action.
#[tauri::command]
pub(crate) async fn reset_local_data(app: AppHandle) -> ApiResult<Snapshot> {
    tauri::async_runtime::spawn_blocking(move || reset_local_data_blocking(&app))
        .await
        .map_err(internal_error)?
}

fn reset_local_data_blocking(app: &AppHandle) -> ApiResult<Snapshot> {
    use tauri_plugin_store::StoreExt;
    let store = app.store(settings_store_name()).map_err(internal_error)?;
    for key in [
        "settings",
        "session",
        crate::store::SETTINGS_PROFILES_KEY,
        crate::store::ONBOARDING_KEY,
    ] {
        store.delete(key);
    }
    // Keep this sequence monotonic. Paired watches retain their highest
    // revision and would reject all future contexts if this reset to zero.
    #[cfg(mobile)]
    store.delete("watch_last_envelope");
    store.save().map_err(internal_error)?;
    let history_store = app.store(history_store_name()).map_err(internal_error)?;
    history_store.delete("history");
    history_store.save().map_err(internal_error)?;

    let engine = app.state::<EngineState>();
    let mut guard = lock_engine(&engine.0);
    *guard = TimerEngine::new(Settings::default(), Local::now()).map_err(ApiError::from)?;
    let snapshot = guard.snapshot();
    drain_and_emit(
        app,
        guard,
        vec![
            EngineEvent::StateChanged(snapshot.phase.clone()),
            EngineEvent::Tick(snapshot.remaining_seconds),
        ],
    );
    if let Some(tracker) = app.try_state::<crate::state::HistoryTracker>() {
        *tracker
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn pause(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, |engine| engine.pause(PauseReason::Manual)).await
}

#[tauri::command]
pub(crate) async fn pause_for_minutes(app: AppHandle, minutes: u16) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, move |engine| engine.pause_for(minutes)).await
}

#[tauri::command]
pub(crate) async fn resume(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::resume).await
}

#[tauri::command]
pub(crate) async fn take_break_now(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::take_break_now).await
}

#[tauri::command]
pub(crate) async fn skip_break(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::skip_break).await
}

#[tauri::command]
pub(crate) async fn postpone_break(app: AppHandle) -> ApiResult<Snapshot> {
    apply_engine_transition_async(app, TimerEngine::postpone).await
}

#[cfg(mobile)]
#[tauri::command]
pub(crate) fn sync_watch_settings(
    app: AppHandle,
    engine: State<'_, EngineState>,
) -> ApiResult<WatchSettingsEnvelopeV1> {
    let guard = lock_engine(&engine.0);
    let snapshot = guard.snapshot();
    let settings = guard.settings().clone();
    drop(guard);
    // A watch is opt-in. Refusing here rather than silently succeeding keeps
    // "not connected" from looking like a transport failure in the UI.
    if !settings.watch_enabled {
        return Err(platform_unavailable(
            "connect a watch in Settings before syncing",
        ));
    }
    let envelope = crate::events::next_watch_settings_envelope(&app, &snapshot, &settings)?;
    crate::events::deliver_watch_settings(&app, &envelope)?;
    Ok(envelope)
}

#[cfg(mobile)]
#[tauri::command]
pub(crate) fn send_test_nudge(
    app: AppHandle,
    engine: State<'_, EngineState>,
) -> ApiResult<NudgeResult> {
    use tauri_plugin_eyecare::EyecareExt;
    if !lock_engine(&engine.0).settings().watch_enabled {
        return Err(platform_unavailable(
            "connect a watch in Settings before sending a test",
        ));
    }
    app.eyecare()
        .send_test_nudge()
        .map_err(platform_unavailable)
}

#[cfg(mobile)]
#[tauri::command]
pub(crate) fn get_watch_status(app: AppHandle) -> ApiResult<WatchStatus> {
    use tauri_plugin_eyecare::EyecareExt;
    app.eyecare().status().map_err(platform_unavailable)
}

/// The OS notification permission for this phone's own reminders.
///
/// Distinct from the watch's permission: with the phone standalone, a denial
/// here means no break is announced at all.
#[cfg(mobile)]
#[tauri::command]
pub(crate) fn get_notification_permission(app: AppHandle) -> ApiResult<WatchPermissionState> {
    use tauri_plugin_eyecare::EyecareExt;
    app.eyecare()
        .local_notification_permission()
        .map_err(platform_unavailable)
}

#[cfg(mobile)]
#[tauri::command]
pub(crate) fn request_notification_permission(app: AppHandle) -> ApiResult<WatchPermissionState> {
    use tauri_plugin_eyecare::EyecareExt;
    app.eyecare()
        .request_local_notification_permission()
        .map_err(platform_unavailable)
}

/// Re-registers the phone's reminder plan and reports what the OS accepted.
///
/// Returning the report rather than `()` is what lets the settings panel say
/// that reminders are degraded — truncated by the iOS pending limit, or
/// downgraded to inexact alarms — instead of a person discovering it when a
/// break fails to arrive.
#[cfg(mobile)]
#[tauri::command]
pub(crate) fn get_reminder_plan_status(
    app: AppHandle,
    engine: State<'_, EngineState>,
) -> ApiResult<ReminderScheduleReport> {
    use tauri_plugin_eyecare::EyecareExt;

    let guard = lock_engine(&engine.0);
    let view = crate::state::EngineView::capture(&guard);
    drop(guard);
    if !view.settings.alert_target.alerts_on_phone() {
        app.eyecare()
            .cancel_local_reminders()
            .map_err(platform_unavailable)?;
        return Ok(ReminderScheduleReport::default());
    }
    let now = chrono::Utc::now();
    let deadline = match &view.snapshot.phase {
        pausio_protocol::TimerPhase::Working
        | pausio_protocol::TimerPhase::PreBreak
        | pausio_protocol::TimerPhase::Breaking { .. } => {
            Some(now + chrono::Duration::seconds(view.snapshot.remaining_seconds.into()))
        }
        _ => None,
    };
    let slots = pausio_core::reminder_plan(
        &view.settings,
        &view.snapshot.phase,
        deadline,
        now,
        crate::events::reminder_plan_limit(),
    );
    app.eyecare()
        .schedule_local_reminders(&slots)
        .map_err(platform_unavailable)
}

#[tauri::command]
pub(crate) fn get_autostart_status(app: AppHandle) -> ApiResult<AutostartStatus> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        Ok(AutostartStatus {
            supported: true,
            enabled: app.autolaunch().is_enabled().map_err(internal_error)?,
        })
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(AutostartStatus {
            supported: false,
            enabled: false,
        })
    }
}

#[tauri::command]
pub(crate) fn set_autostart_enabled(app: AppHandle, enabled: bool) -> ApiResult<AutostartStatus> {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        if enabled {
            manager.enable().map_err(internal_error)?;
        } else {
            manager.disable().map_err(internal_error)?;
        }
        Ok(AutostartStatus {
            supported: true,
            enabled: manager.is_enabled().map_err(internal_error)?,
        })
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Ok(AutostartStatus {
            supported: false,
            enabled: false,
        })
    }
}

#[tauri::command]
pub(crate) fn get_desktop_health(
    app: AppHandle,
    engine: State<'_, EngineState>,
) -> ApiResult<DesktopHealth> {
    #[cfg(desktop)]
    {
        let settings = lock_engine(&engine.0).settings().clone();
        let display_count = app
            .get_webview_window("main")
            .and_then(|window| window.available_monitors().ok())
            .map(|monitors| monitors.len())
            .unwrap_or(0);
        let autostart = get_autostart_status(app.clone())?;
        #[cfg(target_os = "macos")]
        let notification_permission = {
            // Queue a re-probe so a permission or banner change made in System
            // Settings shows up the next time this panel is opened. It cannot be
            // awaited: this command runs inline on the main thread, and every
            // UserNotifications query blocks on XPC.
            crate::mac_notify::refresh();
            crate::events::notification_permission_state()
        };
        #[cfg(not(target_os = "macos"))]
        let notification_permission = {
            use tauri_plugin_notification::NotificationExt;
            app.notification()
                .permission_state()
                .map(|state| format!("{state:?}").to_lowercase())
                .unwrap_or_else(|_| "unknown".into())
        };
        Ok(DesktopHealth {
            platform: std::env::consts::OS.into(),
            notification_permission,
            display_count,
            autostart_supported: autostart.supported,
            autostart_enabled: autostart.enabled,
            history_enabled: settings.history_enabled,
            history_retention_days: settings.history_retention_days,
            display_target: settings.display_target,
            auto_context_fullscreen_supported: cfg!(any(
                target_os = "windows",
                target_os = "macos"
            )),
            auto_context_dnd_supported: cfg!(target_os = "windows"),
        })
    }
    #[cfg(not(desktop))]
    {
        let settings = lock_engine(&engine.0).settings().clone();
        // Report the phone's real permission. It used to be hardcoded
        // "unavailable", which was accurate when the phone posted nothing at
        // all, but now hides the one failure that silences a standalone
        // install entirely.
        #[cfg(mobile)]
        let notification_permission = {
            use tauri_plugin_eyecare::EyecareExt;
            app.eyecare()
                .local_notification_permission()
                .map(|state| {
                    serde_json::to_value(state)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "unknown".into())
                })
                .unwrap_or_else(|_| "unknown".into())
        };
        #[cfg(not(mobile))]
        let notification_permission = {
            let _ = &app;
            String::from("unavailable")
        };
        let _ = app;
        Ok(DesktopHealth {
            platform: std::env::consts::OS.into(),
            notification_permission,
            display_count: 0,
            autostart_supported: false,
            autostart_enabled: false,
            history_enabled: settings.history_enabled,
            history_retention_days: settings.history_retention_days,
            display_target: settings.display_target,
            auto_context_fullscreen_supported: false,
            auto_context_dnd_supported: false,
        })
    }
}

/// Whether this build has any watch-companion capability at all -- desktop
/// builds don't register `sync_watch_settings`/`send_test_nudge`/`get_watch_status`
/// (see the `#[cfg(mobile)]` gating in lib.rs), so the frontend needs an
/// unconditional command to feature-detect rather than probing one that may
/// not exist. Not a substitute for `get_watch_status`'s richer state once a
/// companion is actually available.
#[tauri::command]
pub(crate) fn watch_sync_available() -> bool {
    cfg!(mobile)
}

#[tauri::command]
pub(crate) fn get_health_report(
    app: AppHandle,
    engine: State<'_, EngineState>,
) -> ApiResult<String> {
    serde_json::to_string_pretty(&get_desktop_health(app, engine)?).map_err(internal_error)
}

#[tauri::command]
pub(crate) fn test_reminder(app: AppHandle, engine: State<'_, EngineState>) -> ApiResult<()> {
    #[cfg(desktop)]
    {
        let (locale, sound) = {
            let guard = lock_engine(&engine.0);
            (
                guard.settings().locale,
                crate::events::reminder_cue(guard.settings()),
            )
        };
        crate::events::show_local_notification(
            &app,
            crate::i18n::notification_test_title(locale),
            crate::i18n::notification_test_body(locale),
            sound,
        )
        .map_err(internal_error)
    }
    // A phone must be able to prove its own delivery works, since a
    // standalone install has no wearable to fall back on. This used to be a
    // silent no-op, which made a broken setup indistinguishable from a
    // working one.
    #[cfg(mobile)]
    {
        use tauri_plugin_eyecare::EyecareExt;
        let _ = engine;
        match app.eyecare().post_test_reminder() {
            Ok(pausio_protocol::NudgeResult::Unavailable) => Err(internal_error(
                "notifications are not permitted, so breaks cannot be announced",
            )),
            Ok(_) => Ok(()),
            Err(error) => Err(platform_unavailable(error)),
        }
    }
    #[cfg(all(not(desktop), not(mobile)))]
    {
        let _ = (app, engine);
        Ok(())
    }
}

/// Lets the settings UI play a system sound on demand so a person can hear
/// an option before picking it, without waiting for a real break.
#[tauri::command]
pub(crate) fn preview_system_sound(sound: pausio_core::SystemSound) -> ApiResult<()> {
    #[cfg(desktop)]
    if !crate::sound_player::play_system_sound(sound) {
        return Err(internal_error(
            "the operating system could not play this sound",
        ));
    }
    #[cfg(not(desktop))]
    let _ = sound;
    Ok(())
}

#[cfg(debug_assertions)]
#[tauri::command]
pub(crate) async fn e2e_simulate_screen_lock(
    app: AppHandle,
    locked_seconds: u32,
) -> ApiResult<Snapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        e2e_simulate_screen_lock_blocking(&app, locked_seconds)
    })
    .await
    .map_err(internal_error)?
}

#[cfg(debug_assertions)]
fn e2e_simulate_screen_lock_blocking(app: &AppHandle, locked_seconds: u32) -> ApiResult<Snapshot> {
    if !crate::is_e2e() {
        return Err(internal_error(
            "screen-lock simulation is only available to E2E",
        ));
    }
    let engine = app.state::<EngineState>();
    let mut guard = lock_engine(&engine.0);
    let mut events = guard.screen_locked();
    events.extend(guard.screen_unlocked(locked_seconds, Local::now()));
    let snapshot = guard.snapshot();
    drain_and_emit(app, guard, events);
    Ok(snapshot)
}

/// Registers exactly the shortcuts currently configured, replacing whatever
/// was registered before. Safe to call repeatedly (e.g. on every settings
/// save) since `unregister_all` makes this idempotent rather than additive.
#[cfg(desktop)]
pub(crate) fn sync_global_shortcuts(app: &AppHandle, settings: &Settings) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    for accelerator in [
        settings.end_break_shortcut.as_deref(),
        settings.pause_toggle_shortcut.as_deref(),
        settings.take_break_shortcut.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        // A conflicting or malformed accelerator fails silently by design,
        // matching every other OS-level accelerator registration in PausIO:
        // operating systems refuse to let applications fight over global
        // shortcuts, and there is no user-facing channel for that refusal.
        let _ = shortcuts.register(accelerator);
    }
}

/// Applies opt-in, per-signal automatic context detection. Deliberately acts
/// only from a clean slate (`context.is_none()`): a person's own tray
/// selection — of any reason, any duration — is never overridden or
/// refreshed by this. Each detected signal is applied for exactly one
/// minute (the shortest `set_context_for` allows) and re-armed on the next
/// poll if the OS still reports it, rather than continuously extended, so a
/// person's manual choice can never be silently shortened by this running
/// underneath it. The one accepted tradeoff: at each one-minute boundary
/// there is a roughly one-second gap where a break that became due during
/// that window could briefly surface before being deferred again — judged
/// preferable to any risk of clobbering a manually chosen duration.
#[cfg(desktop)]
pub(crate) fn sync_auto_context(engine: &mut TimerEngine, events: &mut Vec<EngineEvent>) {
    let settings = engine.settings();
    if !settings.auto_detect_fullscreen && !settings.auto_detect_do_not_disturb {
        return;
    }
    let signal = platform_context_signal().filter(|reason| match reason {
        ContextReason::Fullscreen => settings.auto_detect_fullscreen,
        ContextReason::DoNotDisturb => settings.auto_detect_do_not_disturb,
        _ => false,
    });
    let Some(reason) = signal else {
        return;
    };
    if engine.snapshot().context.is_none()
        && let Ok(mut context_events) = engine.set_context_for(reason, 1)
    {
        events.append(&mut context_events);
    }
}

#[cfg(target_os = "macos")]
fn platform_context_signal() -> Option<ContextReason> {
    crate::platform::macos::platform_context_signal()
}
#[cfg(target_os = "linux")]
fn platform_context_signal() -> Option<ContextReason> {
    crate::platform::linux::platform_context_signal()
}
#[cfg(target_os = "windows")]
fn platform_context_signal() -> Option<ContextReason> {
    crate::platform::windows::platform_context_signal()
}

#[cfg(test)]
mod format_history_export_tests {
    use chrono::{TimeZone, Utc};
    use pausio_protocol::{BreakKind, ContextReason};

    use super::{format_history_export, history_events_to_csv};
    use crate::store::{HistoryEvent, HistoryEventKind};

    fn sample_event() -> HistoryEvent {
        HistoryEvent {
            schema_version: 4,
            break_id: Some("break-1".into()),
            occurred_at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
            kind: HistoryEventKind::Completed,
            break_kind: Some(BreakKind::Short),
            context: Some(ContextReason::Meeting),
            target_break_seconds: Some(20),
            work_interval_seconds: Some(1200),
            schedule_fingerprint: Some("fp-1".into()),
        }
    }

    #[test]
    fn unrecognized_format_returns_none() {
        assert!(format_history_export(&[], "xml").is_none());
    }

    #[test]
    fn json_format_round_trips_through_serde() {
        let events = vec![sample_event()];
        let result = format_history_export(&events, "json").expect("json is a known format");
        let json = result.expect("serializing a valid event must not fail");
        assert!(json.contains("\"break_id\": \"break-1\""));
        assert!(json.contains("\"kind\": \"completed\""));
    }

    #[test]
    fn csv_header_lists_every_column_in_order() {
        let csv = history_events_to_csv(&[]);
        assert_eq!(
            csv,
            "schema_version,break_id,occurred_at,kind,break_kind,context,target_break_seconds,work_interval_seconds,schedule_fingerprint\n"
        );
    }

    #[test]
    fn csv_row_matches_the_header_column_order_and_lowercases_enum_variants() {
        let csv = history_events_to_csv(&[sample_event()]);
        let expected_row = "4,\"break-1\",\"2026-01-02T03:04:05+00:00\",\"completed\",\"short\",\"meeting\",20,1200,\"fp-1\"\n";
        assert_eq!(
            csv,
            format!(
                "schema_version,break_id,occurred_at,kind,break_kind,context,target_break_seconds,work_interval_seconds,schedule_fingerprint\n{expected_row}"
            )
        );
    }

    #[test]
    fn csv_handles_none_fields_as_empty_columns_not_the_literal_word_none() {
        let event = HistoryEvent {
            schema_version: 4,
            break_id: None,
            occurred_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            kind: HistoryEventKind::Skipped,
            break_kind: None,
            context: None,
            target_break_seconds: None,
            work_interval_seconds: None,
            schedule_fingerprint: None,
        };
        let csv = history_events_to_csv(&[event]);
        let row = csv.lines().nth(1).expect("one data row");
        assert_eq!(
            row,
            "4,\"\",\"2026-01-01T00:00:00+00:00\",\"skipped\",\"\",\"\",,,\"\""
        );
    }

    #[test]
    fn csv_escapes_embedded_double_quotes_in_string_fields() {
        // break_id and schedule_fingerprint are free-form strings; a value
        // containing a double quote must be escaped per RFC 4180 (doubled),
        // not passed through raw, or the row would corrupt the CSV column
        // boundary for every reader.
        let mut event = sample_event();
        event.break_id = Some("weird\"id".into());
        let csv = history_events_to_csv(&[event]);
        assert!(csv.contains("\"weird\"\"id\""));
    }

    #[test]
    fn csv_emits_one_row_per_event_in_input_order() {
        let mut second = sample_event();
        second.break_id = Some("break-2".into());
        let csv = history_events_to_csv(&[sample_event(), second]);
        let rows: Vec<&str> = csv.lines().skip(1).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("break-1"));
        assert!(rows[1].contains("break-2"));
    }
}
