use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

#[cfg(desktop)]
mod i18n;
mod session_monitor;
#[cfg(desktop)]
mod sound_player;
#[cfg(desktop)]
mod tray_icon;

// New focused modules — pure file moves, no logic changes.
#[cfg(desktop)]
pub(crate) mod break_windows;
pub(crate) mod commands;
pub(crate) mod events;
#[cfg(target_os = "macos")]
pub(crate) mod mac_notify;
#[cfg(desktop)]
pub(crate) mod main_window;
pub(crate) mod platform;
pub(crate) mod state;
pub(crate) mod store;
#[cfg(desktop)]
pub(crate) mod tray_menu;

use chrono::Local;
#[cfg(mobile)]
use pausio_core::EngineEvent;
use pausio_core::{SessionCheckpoint, Settings, TimerEngine};
use pausio_protocol::PauseReason;
#[cfg(mobile)]
use pausio_protocol::WatchRuntimeAction;
#[cfg(desktop)]
use tauri::AppHandle;
use tauri::Manager;
#[cfg(mobile)]
use tauri_plugin_eyecare::EyecareExt;
use tauri_plugin_store::StoreExt;

#[cfg(desktop)]
use commands::{spawn_engine_transition, sync_auto_context, sync_global_shortcuts};
#[cfg(mobile)]
use events::{deliver_watch_settings, next_watch_settings_envelope};
#[cfg(target_os = "macos")]
use main_window::request_quit;
#[cfg(desktop)]
use main_window::{hide_main_window, install_main_window_lifecycle, show_main_window};
#[cfg(desktop)]
use state::SessionLockState;
use state::{EngineState, EngineView, HistoryTracker, drain_and_emit, lock_engine};
use store::{history_store_name, settings_store_name};
#[cfg(desktop)]
use tray_menu::build_tray;

fn has_argument(argument: &str) -> bool {
    std::env::args().any(|value| value == argument)
}

pub(crate) fn is_e2e() -> bool {
    has_argument("--e2e")
}

macro_rules! register_commands {
    ($builder:expr; $($additional:path),* $(,)?) => {
        $builder.invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_settings,
            commands::get_settings_profiles,
            commands::get_onboarding_state,
            commands::complete_onboarding,
            commands::save_settings_profile,
            commands::apply_settings_profile,
            commands::set_settings,
            commands::set_context,
            commands::get_history,
            commands::clear_history,
            commands::export_history,
            commands::reset_local_data,
            commands::start_session,
            commands::start_due_break,
            commands::pause,
            commands::pause_for_minutes,
            commands::resume,
            commands::take_break_now,
            commands::skip_break,
            commands::postpone_break,
            commands::get_autostart_status,
            commands::set_autostart_enabled,
            commands::get_desktop_health,
            commands::get_health_report,
            commands::test_reminder,
            commands::preview_system_sound,
            $($additional),*
        ])
    };
}

#[cfg(desktop)]
pub(crate) fn is_background_autostart() -> bool {
    has_argument("--autostart") && !is_e2e()
}

#[cfg(desktop)]
pub(crate) fn handle_session_event(app: &AppHandle, event: session_monitor::SessionEvent) {
    let Some(lock_state) = app.try_state::<SessionLockState>() else {
        return;
    };
    let Some(state) = app.try_state::<EngineState>() else {
        return;
    };
    let mut engine = lock_engine(&state.0);
    let events = match event {
        session_monitor::SessionEvent::Locked => {
            if lock_state.begin_lock() {
                engine.screen_locked()
            } else {
                vec![]
            }
        }
        session_monitor::SessionEvent::Unlocked => {
            let locked_seconds = lock_state.finish_unlock().unwrap_or(0);
            engine.screen_unlocked(locked_seconds, Local::now())
        }
    };
    drain_and_emit(app, engine, events);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The embedded WebDriver server is compiled in only under the `e2e-webdriver`
    // Cargo feature, and even then is strictly opt-in: the E2E service supplies
    // this port, so ordinary debug and release launches never expose an
    // automation endpoint.
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = {
        // E2E has its own store and WebDriver endpoint. It must not forward to
        // a person's installed tray instance, which would make the test app
        // exit cleanly before the embedded driver becomes reachable.
        let builder = if is_e2e() {
            builder
        } else {
            builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                if !args.iter().any(|argument| argument == "--autostart") {
                    show_main_window(app);
                }
            }))
        };
        let builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ));
        // A single global handler dispatches every configured shortcut. Shortcuts
        // themselves are registered dynamically once settings are loaded (see
        // `sync_global_shortcuts`), since they are person-configurable and must be
        // able to change without restarting PausIO.
        builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let Some(engine_state) = app.try_state::<EngineState>() else {
                        return;
                    };
                    let settings = lock_engine(&engine_state.0).settings().clone();
                    let matches = |value: &Option<String>| {
                        value
                            .as_deref()
                            .and_then(|accelerator| {
                                accelerator
                                    .parse::<tauri_plugin_global_shortcut::Shortcut>()
                                    .ok()
                            })
                            .is_some_and(|parsed| &parsed == shortcut)
                    };
                    if matches(&settings.end_break_shortcut) {
                        spawn_engine_transition(app.clone(), |engine| {
                            match engine.snapshot().phase {
                                pausio_protocol::TimerPhase::Breaking { .. } => engine.skip_break(),
                                pausio_protocol::TimerPhase::BreakDue { .. } => engine.postpone(),
                                _ => Ok(vec![]),
                            }
                        });
                    } else if matches(&settings.pause_toggle_shortcut) {
                        spawn_engine_transition(app.clone(), |engine| {
                            match engine.snapshot().phase {
                                pausio_protocol::TimerPhase::Paused { .. } => engine.resume(),
                                pausio_protocol::TimerPhase::Dormant => engine.start_session(),
                                pausio_protocol::TimerPhase::Working
                                | pausio_protocol::TimerPhase::PreBreak => {
                                    engine.pause(PauseReason::Manual)
                                }
                                _ => Ok(vec![]),
                            }
                        });
                    } else if matches(&settings.take_break_shortcut) {
                        spawn_engine_transition(app.clone(), TimerEngine::take_break_now);
                    }
                })
                .build(),
        )
    };
    // Gated behind a Cargo feature (never on by default), not just the env var:
    // an installed/released build must not contain this capability at all,
    // regardless of what environment variables a launching process sets.
    #[cfg(all(desktop, feature = "e2e-webdriver"))]
    let builder = if std::env::var_os("TAURI_WEBDRIVER_PORT").is_some() {
        builder
            .plugin(tauri_plugin_wdio::init())
            .plugin(tauri_plugin_wdio_webdriver::init())
    } else {
        builder
    };
    let builder = builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build());
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_eyecare::init());
    let builder = builder
        .manage(HistoryTracker::default())
        .manage(EngineState(Mutex::new(
            TimerEngine::new(Settings::default(), Local::now())
                .expect("default settings are valid"),
        )));
    #[cfg(desktop)]
    let builder = builder.manage(SessionLockState::default());
    let builder = builder.setup(|app| {
        let store = app.store(settings_store_name())?;
        // Desktop never connects to a wearable. Earlier desktop builds wrote
        // these local transport remnants despite returning an unavailable
        // status; discard them once so the desktop store has no watch state.
        #[cfg(desktop)]
        if store.delete("watch_revision") | store.delete("watch_last_envelope") {
            store.save()?;
        }
        if let Some(saved) = store.get("settings")
            && let Ok(settings) = serde_json::from_value::<Settings>(saved)
            && let Some(engine) = app.try_state::<EngineState>()
        {
            let mut engine = lock_engine(&engine.0);
            let _ = engine.replace_settings(settings, Local::now());
        }
        // One-time migration: history used to live in the settings store
        // alongside "session"; move it to its own store so the frequent
        // session heartbeat never has to rewrite the (much larger) history
        // array. Idempotent — nothing to do once the key is gone.
        if let Some(legacy_history) = store.get("history") {
            let history_store = app.store(history_store_name())?;
            if history_store.get("history").is_none() {
                history_store.set("history", legacy_history);
                history_store.save()?;
            }
            store.delete("history");
            store.save()?;
        }
        // Each WebDriver process gets a deterministic timer start while
        // retaining settings long enough to test an in-app webview reload.
        // This affects only the explicit --e2e store, never a person's
        // durable timer recovery checkpoint.
        if is_e2e() {
            store.delete("session");
            store.save()?;
            app.store(history_store_name())?.delete("history");
            app.store(history_store_name())?.save()?;
        }
        let restored_events = if !is_e2e()
            && let Some(saved) = store.get("session")
            && let Ok(checkpoint) = serde_json::from_value::<SessionCheckpoint>(saved)
            && let Some(engine_state) = app.try_state::<EngineState>()
        {
            let mut engine = lock_engine(&engine_state.0);
            let settings = engine.settings().clone();
            match TimerEngine::restore(settings, checkpoint, Local::now()) {
                Ok((restored, events)) => {
                    *engine = restored;
                    events
                }
                Err(_) => vec![],
            }
        } else {
            vec![]
        };
        // Context is intentionally idempotent: updateApplicationContext keeps only the
        // newest envelope, so every mobile launch can safely repair a previously missed
        // phone-to-watch transfer without making the web UI the delivery authority.
        #[cfg(mobile)]
        if let Some(engine) = app.try_state::<EngineState>() {
            let engine = lock_engine(&engine.0);
            let snapshot = engine.snapshot();
            let settings = engine.settings().clone();
            drop(engine);
            if let Ok(envelope) = next_watch_settings_envelope(app.handle(), &snapshot, &settings) {
                let _ = deliver_watch_settings(app.handle(), &envelope);
            }
        }
        #[cfg(desktop)]
        {
            build_tray(app.handle())?;
            if let Some(engine) = app.try_state::<EngineState>() {
                sync_global_shortcuts(app.handle(), lock_engine(&engine.0).settings());
            }
            install_main_window_lifecycle(app.handle());
            #[cfg(target_os = "windows")]
            platform::windows::configure_windows_main_window(app.handle());
            #[cfg(target_os = "macos")]
            {
                platform::macos::configure_macos_main_window(app.handle());
                let menu = platform::macos::build_app_menu(app.handle())?;
                app.set_menu(menu)?;
                app.on_menu_event(|app, event| {
                    if event.id().as_ref() == "quit-app" {
                        request_quit(app);
                    }
                });
            }
            session_monitor::install(app.handle().clone());
            if is_background_autostart() {
                hide_main_window(app.handle());
            } else {
                show_main_window(app.handle());
            }
        }
        // Before the publisher, so the first notification never has to wait for
        // a cold capability probe. This also moves the one-time authorization
        // request to launch — where a person is plausibly still at the keyboard
        // — instead of the middle of the first break, and confines a dialog that
        // is never answered to a thread that owns nothing.
        #[cfg(target_os = "macos")]
        mac_notify::install();
        // Start the single publisher before anything can publish. Recovery
        // events below can include a due or in-progress break, and `emit` may
        // therefore build a prompt or overlay window: window creation and menu
        // mutation dispatch to the main event loop, and `setup` itself runs *on*
        // that loop (inside didFinishLaunching), so emitting here would wait for
        // the loop that is currently running this closure — leaving the process
        // alive with no window and no tray icon.
        state::install_publisher(app.handle());
        if !restored_events.is_empty()
            && let Some(engine) = app.try_state::<EngineState>()
        {
            let guard = lock_engine(&engine.0);
            drain_and_emit(app.handle(), guard, restored_events);
        }
        let handle = app.handle().clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_tick = Instant::now();
            #[cfg(desktop)]
            let mut observed_session_transition = handle
                .try_state::<SessionLockState>()
                .map(|state| state.transition_generation())
                .unwrap_or(0);
            loop {
                interval.tick().await;
                #[cfg(target_os = "linux")]
                platform::linux::sync_linux_session_lock(&handle);
                #[cfg(mobile)]
                let pending_watch_action = handle.eyecare().take_pending_action().ok().flatten();
                let Some(state) = handle.try_state::<EngineState>() else {
                    continue;
                };
                let mut engine = lock_engine(&state.0);
                // A session callback accounts for the whole lock duration in
                // screen_unlocked(). If the native loop was suspended or simply
                // did not poll between lock and unlock, last_tick still spans
                // that same duration. Reset it on every observed transition so
                // the lock is never applied a second time as ordinary work.
                #[cfg(desktop)]
                if let Some(lock_state) = handle.try_state::<SessionLockState>() {
                    let generation = lock_state.transition_generation();
                    if generation != observed_session_transition {
                        observed_session_transition = generation;
                        last_tick = Instant::now();
                        continue;
                    }
                }
                let elapsed = last_tick.elapsed().as_secs();
                if elapsed == 0 {
                    continue;
                }
                last_tick += Duration::from_secs(elapsed);
                let mut events = vec![];
                #[cfg(mobile)]
                if let Some(action) = pending_watch_action.as_ref() {
                    let result = match action.action {
                        WatchRuntimeAction::Pause => engine.pause(PauseReason::Manual),
                        WatchRuntimeAction::Resume => engine.resume(),
                        WatchRuntimeAction::TakeBreakNow
                            if matches!(
                                engine.snapshot().phase,
                                pausio_protocol::TimerPhase::BreakDue { .. }
                            ) =>
                        {
                            engine.start_due_break()
                        }
                        WatchRuntimeAction::TakeBreakNow => engine.take_break_now(),
                        WatchRuntimeAction::SkipBreak => engine.skip_break(),
                    };
                    if let Ok(mut watch_events) = result {
                        events.append(&mut watch_events);
                    }
                }
                let elapsed_seconds = elapsed.min(u32::MAX as u64) as u32;
                let mut consumed_by_wake = false;
                #[cfg(desktop)]
                let mut deferred_for_active_input = false;
                #[cfg(not(desktop))]
                let deferred_for_active_input = false;
                if elapsed >= 30
                    && let Ok(mut wake_events) = engine.woke_after(elapsed_seconds)
                    && !wake_events.is_empty()
                {
                    consumed_by_wake = true;
                    events.append(&mut wake_events);
                }
                #[cfg(desktop)]
                {
                    let mut current_idle_seconds = None;
                    // Skip the native idle poll entirely during a break: report_idle's
                    // only effect at this threshold is pause(), which is a no-op while
                    // Breaking, and keeping it out removes a stall vector under the
                    // strict overlay watchdog.
                    if !is_e2e()
                        && !matches!(
                            engine.snapshot().phase,
                            pausio_protocol::TimerPhase::Breaking { .. }
                        )
                        && let Some(idle_seconds) = platform_idle_seconds()
                    {
                        current_idle_seconds = Some(idle_seconds);
                        if idle_seconds >= 5 * 60
                            && !matches!(
                                engine.snapshot().phase,
                                pausio_protocol::TimerPhase::Paused {
                                    reason: PauseReason::Idle
                                }
                            )
                            && let Ok(mut idle_events) = engine.report_idle(idle_seconds)
                        {
                            events.append(&mut idle_events);
                        } else if idle_seconds < 5 * 60
                            && let Ok(mut resume_events) = engine.activity_resumed()
                        {
                            events.append(&mut resume_events);
                        }
                    }
                    // A prompt in the middle of a burst of interaction is a primary
                    // uninstall trigger. This relies only on the OS's aggregate idle
                    // duration (never keys, app names, or input content), waits 15
                    // seconds, and the core enforces a four-per-day ceiling.
                    let snapshot = engine.snapshot();
                    if !is_e2e()
                        && snapshot.context.is_none()
                        && matches!(
                            snapshot.phase,
                            pausio_protocol::TimerPhase::Working
                                | pausio_protocol::TimerPhase::PreBreak
                        )
                        && snapshot.remaining_seconds <= elapsed_seconds
                        && current_idle_seconds.is_some_and(|idle| idle <= 5)
                        && let Ok(mut input_events) = engine.defer_due_for_active_input(15)
                    {
                        events.append(&mut input_events);
                        deferred_for_active_input = true;
                    }
                    if !is_e2e() {
                        sync_auto_context(&mut engine, &mut events);
                    }
                }
                if !consumed_by_wake && !deferred_for_active_input {
                    events.extend(engine.advance(elapsed_seconds, Local::now()));
                }
                #[cfg(mobile)]
                let should_sync_watch = events.iter().any(|event| {
                    matches!(
                        event,
                        EngineEvent::StateChanged(_)
                            | EngineEvent::Started(_)
                            | EngineEvent::Ended(_)
                            | EngineEvent::Skipped(_)
                            | EngineEvent::Due(_)
                    )
                });
                // Capture a lock-free view, queue it, and only then drop the guard.
                // This task must never itself call `emit`: emit dispatches to (and
                // blocks on) the main event loop via tray menu mutation and break
                // window create/destroy, and blocking here — on a tokio worker, with
                // the engine mutex just released — is what let a Windows break
                // transition wedge the whole app. Enqueueing under the guard is
                // what makes publication order equal mutation order.
                let view = EngineView::capture(&engine);
                #[cfg(mobile)]
                let sync_watch = pending_watch_action.is_some() || should_sync_watch;
                #[cfg(mobile)]
                state::publish(&handle, events, view, sync_watch);
                #[cfg(not(mobile))]
                state::publish(&handle, events, view);
                drop(engine);
            }
        });
        Ok(())
    });

    #[cfg(all(debug_assertions, mobile))]
    let builder = register_commands!(builder;
        commands::sync_watch_settings,
        commands::send_test_nudge,
        commands::get_watch_status,
        commands::e2e_simulate_screen_lock,
    );
    #[cfg(all(debug_assertions, not(mobile)))]
    let builder = register_commands!(builder; commands::e2e_simulate_screen_lock);
    #[cfg(all(not(debug_assertions), mobile))]
    let builder = register_commands!(builder;
        commands::sync_watch_settings,
        commands::send_test_nudge,
        commands::get_watch_status,
    );
    #[cfg(all(not(debug_assertions), not(mobile)))]
    let builder = register_commands!(builder;);

    builder
        .build(tauri::generate_context!())
        .expect("error while running PausIO")
        .run(|_, _| {});
}

// platform_idle_seconds dispatcher for the tick loop in lib.rs
#[cfg(target_os = "macos")]
fn platform_idle_seconds() -> Option<u32> {
    platform::macos::platform_idle_seconds()
}
#[cfg(target_os = "linux")]
fn platform_idle_seconds() -> Option<u32> {
    platform::linux::platform_idle_seconds()
}
#[cfg(target_os = "windows")]
fn platform_idle_seconds() -> Option<u32> {
    platform::windows::platform_idle_seconds()
}

#[cfg(all(test, desktop))]
mod tests {
    use crate::break_windows::{bottom_right_position, overlay_watchdog_deadline};
    use crate::platform::linux::{linux_idle_seconds_from, linux_session_locked_from};
    use crate::platform::windows::windows_context_reason_from;
    use pausio_protocol::ContextReason;
    use std::time::Duration;

    #[test]
    fn prompt_sits_at_the_bottom_right_of_the_work_area() {
        assert_eq!(
            bottom_right_position((-1920, 0), (1920, 1040), (460, 300), 20),
            (-480, 720)
        );
    }

    #[test]
    fn bottom_right_position_never_underflows_on_a_small_work_area() {
        assert_eq!(
            bottom_right_position((40, -50), (320, 200), (460, 300), 20),
            (40, -50)
        );
    }

    #[test]
    fn watchdog_deadline_adds_the_grace_window() {
        assert_eq!(overlay_watchdog_deadline(20), Duration::from_secs(30));
        assert_eq!(overlay_watchdog_deadline(300), Duration::from_secs(310));
    }

    #[test]
    fn watchdog_deadline_never_overflows_at_the_u32_ceiling() {
        let deadline = overlay_watchdog_deadline(u32::MAX);
        assert_eq!(deadline, Duration::from_secs(u64::from(u32::MAX) + 10));
    }

    #[test]
    fn linux_session_idle_parser_uses_only_coarse_logind_state() {
        let state = "IdleHint=yes\nIdleSinceHintMonotonic=120000000\n";
        assert_eq!(linux_idle_seconds_from(state, 421.9), Some(301));
        assert_eq!(
            linux_idle_seconds_from("IdleHint=no\nIdleSinceHintMonotonic=0\n", 421.9),
            Some(0)
        );
        assert_eq!(linux_idle_seconds_from("IdleHint=yes\n", 421.9), None);
    }

    #[test]
    fn linux_lock_parser_accepts_only_explicit_logind_booleans() {
        assert_eq!(linux_session_locked_from("LockedHint=yes\n"), Some(true));
        assert_eq!(linux_session_locked_from("LockedHint=no\n"), Some(false));
        assert_eq!(linux_session_locked_from("LockedHint=unknown\n"), None);
    }

    #[test]
    fn windows_notification_state_maps_only_fullscreen_and_quiet_time() {
        assert_eq!(
            windows_context_reason_from(3), // QUNS_RUNNING_D3D_FULL_SCREEN
            Some(ContextReason::Fullscreen)
        );
        assert_eq!(
            windows_context_reason_from(4), // QUNS_PRESENTATION_MODE
            Some(ContextReason::Fullscreen)
        );
        assert_eq!(
            windows_context_reason_from(6), // QUNS_QUIET_TIME
            Some(ContextReason::DoNotDisturb)
        );
        // QUNS_BUSY is deliberately left unmapped: it also covers an ordinary
        // maximized window, which is not a reliable signal to defer a break.
        assert_eq!(windows_context_reason_from(2), None);
        assert_eq!(windows_context_reason_from(5), None); // QUNS_ACCEPTS_NOTIFICATIONS
        assert_eq!(windows_context_reason_from(7), None); // QUNS_APP
    }

    /// `tauri.windows.conf.json` and `tauri.linux.conf.json` each replace the
    /// entire `app.windows[0]` entry wholesale (Tauri merges platform config
    /// via JSON Merge Patch, which does not deep-merge arrays) — the only
    /// fields that should legitimately differ from the base `tauri.conf.json`
    /// are native chrome controls. This guards against a future edit (e.g. a
    /// resize) landing in only one file and silently drifting from the others.
    #[test]
    fn desktop_titlebar_platform_configs_share_the_same_window_geometry() {
        use serde_json::Value;

        // Fields that intentionally differ per platform because they control
        // native window chrome, not the custom titlebar strip.
        const PLATFORM_SPECIFIC_FIELDS: &[&str] =
            &["decorations", "hiddenTitle", "shadow", "titleBarStyle"];

        let read_main_window = |path: &str| -> Value {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
            let config: Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("invalid JSON in {path}: {e}"));
            config["app"]["windows"][0].clone()
        };

        let base = read_main_window("tauri.conf.json");
        let windows = read_main_window("tauri.windows.conf.json");
        let linux = read_main_window("tauri.linux.conf.json");

        let base_obj = base.as_object().expect("base window entry is an object");
        for (name, expected) in base_obj {
            if PLATFORM_SPECIFIC_FIELDS.contains(&name.as_str()) {
                continue;
            }
            assert_eq!(
                windows.get(name),
                Some(expected),
                "tauri.windows.conf.json field {name:?} diverged from tauri.conf.json"
            );
            assert_eq!(
                linux.get(name),
                Some(expected),
                "tauri.linux.conf.json field {name:?} diverged from tauri.conf.json"
            );
        }

        // Both platform overrides must actually disable native decorations —
        // that is the entire point of this override existing.
        assert_eq!(windows["decorations"], Value::Bool(false));
        assert_eq!(linux["decorations"], Value::Bool(false));
        // Tao documents that its native shadow for an undecorated Windows window
        // necessarily paints a thin 1px frame line. Disable that shadow on Windows;
        // the rounded content remains, while macOS and Linux keep their native shadow.
        assert_eq!(windows["shadow"], Value::Bool(false));
        assert_eq!(linux["shadow"], Value::Bool(true));
    }
}
