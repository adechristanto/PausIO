use chrono::Local;
use pausio_core::{EngineError, EngineEvent, Settings, Snapshot, TimerEngine};
use pausio_protocol::{ContextReason, PauseReason};
#[cfg(mobile)]
use pausio_protocol::{NudgeResult, WatchSettingsEnvelopeV1, WatchStatus};
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
    /// Whether this platform can supply automatic context signals at all.
    /// Currently Windows only; macOS and Linux report `false` honestly.
    pub auto_context_supported: bool,
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
        .and_then(|value| serde_json::from_value::<Vec<HistoryEvent>>(value).ok())
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
    match format.as_str() {
        "json" => serde_json::to_string_pretty(&events).map_err(internal_error),
        "csv" => {
            let mut output = String::from(
                "schema_version,break_id,occurred_at,kind,break_kind,context,target_break_seconds,work_interval_seconds,schedule_fingerprint\n",
            );
            for event in events {
                let escape = |value: String| format!("\"{}\"", value.replace('"', "\"\""));
                output.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    event.schema_version,
                    escape(event.break_id.unwrap_or_default()),
                    escape(event.occurred_at.to_rfc3339()),
                    escape(format!("{:?}", event.kind).to_lowercase()),
                    escape(
                        event
                            .break_kind
                            .map(|kind| format!("{kind:?}").to_lowercase())
                            .unwrap_or_default()
                    ),
                    escape(
                        event
                            .context
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
                    escape(event.schedule_fingerprint.unwrap_or_default()),
                ));
            }
            Ok(output)
        }
        _ => Err(internal_error("history export format must be json or csv")),
    }
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
    let envelope = crate::events::next_watch_settings_envelope(&app, &snapshot, &settings)?;
    #[cfg(mobile)]
    crate::events::deliver_watch_settings(&app, &envelope)?;
    Ok(envelope)
}

#[cfg(mobile)]
#[tauri::command]
pub(crate) fn send_test_nudge(app: AppHandle) -> ApiResult<NudgeResult> {
    use tauri_plugin_eyecare::EyecareExt;
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
            auto_context_supported: cfg!(target_os = "windows"),
        })
    }
    #[cfg(not(desktop))]
    {
        let settings = lock_engine(&engine.0).settings().clone();
        let _ = app;
        Ok(DesktopHealth {
            platform: std::env::consts::OS.into(),
            notification_permission: "unavailable".into(),
            display_count: 0,
            autostart_supported: false,
            autostart_enabled: false,
            history_enabled: settings.history_enabled,
            history_retention_days: settings.history_retention_days,
            display_target: settings.display_target,
            auto_context_supported: false,
        })
    }
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
                crate::events::resolved_notification_sound(guard.settings()),
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
    #[cfg(not(desktop))]
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
