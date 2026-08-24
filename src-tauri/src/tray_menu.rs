use pausio_core::{Locale, TimerEngine};
use pausio_protocol::{BreakKind, PauseReason};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::state::{EngineState, EngineView, lock_engine};

pub(crate) struct TrayMenuItems {
    pub status: MenuItem<tauri::Wry>,
    pub show: MenuItem<tauri::Wry>,
    pub pause: MenuItem<tauri::Wry>,
    pub pause_30: MenuItem<tauri::Wry>,
    pub pause_60: MenuItem<tauri::Wry>,
    pub pause_120: MenuItem<tauri::Wry>,
    pub break_now: MenuItem<tauri::Wry>,
    pub quit: MenuItem<tauri::Wry>,
}

pub(crate) static TRAY_MENU_ITEMS: std::sync::OnceLock<TrayMenuItems> = std::sync::OnceLock::new();

/// `TrayIcon` is reference-counted and removed the moment its last instance drops (Tauri's
/// own doc comment on the type) — this is the one persistent reference that keeps the tray
/// icon alive for the app's lifetime, not something read from elsewhere.
pub(crate) static TRAY_ICON: std::sync::OnceLock<tauri::tray::TrayIcon<tauri::Wry>> =
    std::sync::OnceLock::new();

pub(crate) fn current_locale(app: &AppHandle) -> Locale {
    app.try_state::<EngineState>()
        .map(|engine| lock_engine(&engine.0).settings().locale)
        .unwrap_or_default()
}

pub(crate) fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let locale = current_locale(app);
    let status = MenuItem::with_id(
        app,
        "status",
        crate::i18n::tray_status_starting(locale),
        false,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(
        app,
        "show",
        crate::i18n::tray_open(locale),
        true,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(
        app,
        "pause",
        crate::i18n::tray_pause(locale),
        true,
        None::<&str>,
    )?;
    let pause_30 = MenuItem::with_id(
        app,
        "pause-30",
        crate::i18n::tray_pause_for_30(locale),
        true,
        None::<&str>,
    )?;
    let pause_60 = MenuItem::with_id(
        app,
        "pause-60",
        crate::i18n::tray_pause_for_60(locale),
        true,
        None::<&str>,
    )?;
    let pause_120 = MenuItem::with_id(
        app,
        "pause-120",
        crate::i18n::tray_pause_for_120(locale),
        true,
        None::<&str>,
    )?;
    let break_now = MenuItem::with_id(
        app,
        "break",
        crate::i18n::tray_take_break(locale),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "quit",
        crate::i18n::tray_quit(locale),
        true,
        None::<&str>,
    )?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status, &separator, &show, &pause, &pause_30, &pause_60, &pause_120, &break_now, &quit,
        ],
    )?;
    let _ = TRAY_MENU_ITEMS.set(TrayMenuItems {
        status,
        show: show.clone(),
        pause: pause.clone(),
        pause_30: pause_30.clone(),
        pause_60: pause_60.clone(),
        pause_120: pause_120.clone(),
        break_now: break_now.clone(),
        quit: quit.clone(),
    });

    let mut tray = TrayIconBuilder::with_id("pausio-tray")
        .menu(&menu)
        .tooltip(crate::i18n::tray_tooltip(locale))
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::main_window::show_main_window(&app)
                });
            }
            "pause" => {
                crate::commands::spawn_engine_transition(app.clone(), |engine| {
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
            }
            "break" => {
                crate::commands::spawn_engine_transition(app.clone(), TimerEngine::take_break_now);
            }
            "pause-30" | "pause-60" | "pause-120" => {
                let minutes = match event.id().as_ref() {
                    "pause-30" => 30,
                    "pause-60" => 60,
                    "pause-120" => 120,
                    _ => unreachable!(),
                };
                crate::commands::spawn_engine_transition(app.clone(), move |engine| {
                    engine.pause_for(minutes)
                });
            }
            "quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::main_window::request_quit(&app)
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    crate::main_window::show_main_window(&app)
                });
            }
        });
    // A small, mostly-transparent white eye glyph — not the full-color app
    // icon, and deliberately not a macOS template image: the mark is fixed
    // white and must not be recolored by the system theme. See `tray_icon`
    // for the rationale on why this stays a single static shape rather than
    // per-state icons.
    tray = tray.icon(crate::tray_icon::render());
    let tray = tray.build(app)?;
    let _ = TRAY_ICON.set(tray);
    if let Some(engine) = app.try_state::<EngineState>() {
        let view = EngineView::capture(&lock_engine(&engine.0));
        update_tray_state(&view);
    }
    Ok(())
}

pub(crate) fn update_tray_state(view: &EngineView) {
    let Some(items) = TRAY_MENU_ITEMS.get() else {
        return;
    };
    let locale = view.settings.locale;
    let snapshot = view.snapshot.clone();
    if let Some(context) = snapshot.context {
        let _ = items.status.set_text(crate::i18n::tray_waiting(
            locale,
            crate::i18n::context_label(locale, &context),
        ));
        let _ = items.pause.set_text(crate::i18n::tray_pause(locale));
        let _ = items.pause.set_enabled(false);
        return;
    }
    let (status, pause_label, pause_enabled) = match snapshot.phase {
        pausio_protocol::TimerPhase::Working | pausio_protocol::TimerPhase::PreBreak => (
            crate::i18n::tray_next_break_in(locale, &format_duration(snapshot.remaining_seconds)),
            crate::i18n::tray_pause(locale),
            true,
        ),
        pausio_protocol::TimerPhase::Paused {
            reason: PauseReason::ScreenLock,
        } => (
            crate::i18n::tray_paused_screen_locked(locale).into(),
            crate::i18n::tray_resume(locale),
            false,
        ),
        pausio_protocol::TimerPhase::Paused { .. } => (
            crate::i18n::tray_paused(locale).into(),
            crate::i18n::tray_resume(locale),
            true,
        ),
        pausio_protocol::TimerPhase::Dormant => (
            crate::i18n::tray_ready_to_start(locale).into(),
            crate::i18n::tray_start_session(locale),
            true,
        ),
        pausio_protocol::TimerPhase::BreakDue { .. } => (
            crate::i18n::tray_break_due(locale).into(),
            crate::i18n::tray_pause(locale),
            false,
        ),
        pausio_protocol::TimerPhase::Breaking { kind } => {
            let kind_label = match kind {
                BreakKind::Short => crate::i18n::tray_break_kind_short(locale),
                BreakKind::Long => crate::i18n::tray_break_kind_long(locale),
            };
            (
                crate::i18n::tray_break_remaining(
                    locale,
                    kind_label,
                    &format_duration(snapshot.remaining_seconds),
                ),
                crate::i18n::tray_pause(locale),
                false,
            )
        }
    };
    let _ = items.status.set_text(status);
    let _ = items.pause.set_text(pause_label);
    let _ = items.pause.set_enabled(pause_enabled);
}

/// Retranslates the tray's static menu labels (the ones `update_tray_state`
/// never touches) after a locale change. `update_tray_state` still owns the
/// dynamic status/pause text and runs after every settings change already.
pub(crate) fn retranslate_tray(locale: Locale) {
    let Some(items) = TRAY_MENU_ITEMS.get() else {
        return;
    };
    let _ = items.show.set_text(crate::i18n::tray_open(locale));
    let _ = items
        .pause_30
        .set_text(crate::i18n::tray_pause_for_30(locale));
    let _ = items
        .pause_60
        .set_text(crate::i18n::tray_pause_for_60(locale));
    let _ = items
        .pause_120
        .set_text(crate::i18n::tray_pause_for_120(locale));
    let _ = items
        .break_now
        .set_text(crate::i18n::tray_take_break(locale));
    let _ = items.quit.set_text(crate::i18n::tray_quit(locale));
    if let Some(tray) = TRAY_ICON.get() {
        let _ = tray.set_tooltip(Some(crate::i18n::tray_tooltip(locale)));
    }
}

fn format_duration(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
