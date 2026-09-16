use pausio_core::EngineEvent;
#[cfg(desktop)]
use pausio_core::{Settings, SystemSound};
#[cfg(desktop)]
use pausio_protocol::BreakKind;
#[cfg(mobile)]
use pausio_protocol::{WatchPhase, WatchSettingsEnvelopeV1};
#[cfg(desktop)]
use tauri::Manager;
use tauri::{AppHandle, Emitter};

use crate::state::EngineView;
#[cfg(mobile)]
use crate::state::{ApiResult, internal_error, platform_unavailable};
use crate::store::{append_history, history_event, persist_session};

#[cfg(mobile)]
use pausio_protocol::NudgeResult;

#[cfg(desktop)]
use crate::break_windows::{
    close_break_overlays, close_break_prompt, show_break_overlays, show_break_prompt,
};
#[cfg(desktop)]
use crate::main_window::set_quit_enabled;
#[cfg(desktop)]
use crate::tray_menu::update_tray_state;

#[cfg(mobile)]
pub(crate) fn next_watch_settings_envelope(
    app: &AppHandle,
    snapshot: &pausio_core::Snapshot,
    settings: &pausio_core::Settings,
) -> ApiResult<WatchSettingsEnvelopeV1> {
    use tauri_plugin_store::StoreExt;

    let store = app
        .store(crate::store::settings_store_name())
        .map_err(internal_error)?;
    // The engine publishes whole-second countdowns. Keeping the projection at
    // that precision prevents an explicit retry milliseconds later from
    // manufacturing a distinct revision for the same timer state.
    let now = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
        .unwrap_or_else(chrono::Utc::now);
    let next_revision = store
        .get("watch_revision")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        .saturating_add(1);
    let envelope = build_watch_settings_envelope(snapshot, settings, next_revision, now);
    let previous = store
        .get("watch_last_envelope")
        .and_then(|value| serde_json::from_value::<WatchSettingsEnvelopeV1>(value).ok());
    if let Some(previous) = previous
        && same_watch_state(&previous, &envelope)
    {
        return Ok(previous);
    }
    store.set("watch_revision", next_revision);
    store.set(
        "watch_last_envelope",
        serde_json::to_value(&envelope).map_err(internal_error)?,
    );
    store.save().map_err(internal_error)?;
    Ok(envelope)
}

/// Projects canonical engine state into the latest watch context without
/// touching persistence or transport, keeping watch behavior deterministic in
/// tests and preventing per-tick messages.
#[cfg(mobile)]
pub(crate) fn build_watch_settings_envelope(
    snapshot: &pausio_core::Snapshot,
    settings: &pausio_core::Settings,
    revision: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> WatchSettingsEnvelopeV1 {
    let mut envelope = WatchSettingsEnvelopeV1::new(revision, now);
    envelope.work_interval_seconds = settings.work_seconds;
    envelope.short_break_seconds = settings.short_break_seconds;
    envelope.long_break_seconds = settings.long_break_seconds;
    envelope.pre_break_seconds = settings.pre_break_seconds;
    envelope.active_days_mask = settings.active_days_mask;
    envelope.active_start_minutes = settings.active_start_minutes;
    envelope.active_end_minutes = settings.active_end_minutes;
    // IANA names are portable to Foundation and java.time. Abbreviations such
    // as CEST are ambiguous and made Wear scheduling fail silently.
    envelope.timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
    envelope.paused = matches!(snapshot.phase, pausio_protocol::TimerPhase::Paused { .. });
    envelope.phase = Some(match &snapshot.phase {
        pausio_protocol::TimerPhase::Dormant => WatchPhase::Dormant,
        pausio_protocol::TimerPhase::Working => WatchPhase::Working,
        pausio_protocol::TimerPhase::PreBreak => WatchPhase::PreBreak,
        pausio_protocol::TimerPhase::BreakDue { .. } => WatchPhase::BreakDue,
        pausio_protocol::TimerPhase::Breaking { .. } => WatchPhase::Breaking,
        pausio_protocol::TimerPhase::Paused { .. } => WatchPhase::Paused,
    });
    let deadline = match &snapshot.phase {
        pausio_protocol::TimerPhase::Working | pausio_protocol::TimerPhase::PreBreak => {
            Some(now + chrono::Duration::seconds(snapshot.remaining_seconds.into()))
        }
        pausio_protocol::TimerPhase::Breaking { kind } => {
            envelope.break_active = true;
            envelope.break_kind = Some(kind.clone());
            Some(now + chrono::Duration::seconds(snapshot.remaining_seconds.into()))
        }
        _ => None,
    };
    envelope.next_break_at = deadline;
    envelope.phase_deadline_at = deadline;
    envelope
}

#[cfg(mobile)]
fn same_watch_state(previous: &WatchSettingsEnvelopeV1, current: &WatchSettingsEnvelopeV1) -> bool {
    let mut previous = previous.clone();
    previous.revision = current.revision;
    previous.updated_at = current.updated_at;
    previous == *current
}

#[cfg(mobile)]
pub(crate) fn deliver_watch_settings(
    app: &AppHandle,
    envelope: &WatchSettingsEnvelopeV1,
) -> ApiResult<NudgeResult> {
    use tauri_plugin_eyecare::EyecareExt;
    app.eyecare()
        .sync_settings(envelope)
        .map_err(platform_unavailable)
}

/// State changes are sent as a new revision; timer ticks are deliberately not.
#[cfg(mobile)]
pub(crate) fn sync_watch_state(app: &AppHandle, view: &EngineView) {
    if let Ok(envelope) = next_watch_settings_envelope(app, &view.snapshot, &view.settings) {
        let _ = deliver_watch_settings(app, &envelope);
    }
}

/// Posts a notification and reports whether a person will actually see it.
///
/// On macOS the post is queued rather than awaited — see [`crate::mac_notify`]
/// for why nothing on the publisher thread may wait for UserNotifications — so
/// `Ok` means "macOS is configured to draw this", not "this specific banner has
/// appeared". That is the only question callers need answered: it decides
/// whether PausIO must raise a surface of its own as well.
#[cfg(desktop)]
pub(crate) fn show_local_notification(
    app: &AppHandle,
    title: &str,
    body: &str,
    sound: Option<SystemSound>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        let capability = crate::mac_notify::post(title, body);
        // The cue is a PausIO setting, independent of the system's per-app
        // notification sound, and the queued notification is deliberately
        // silent so it happens exactly once. It is played whenever PausIO is
        // raising *some* surface for this event — the caller falls back to its
        // own window when the banner will not be seen, and that window deserves
        // the same cue.
        crate::mac_notify::play_cue(sound);
        if capability.will_be_seen() {
            Ok(())
        } else {
            Err(format!("macOS will not display it: {capability:?}"))
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let notification = app.notification().builder().title(title).body(body);
        let notification = match sound {
            Some(sound) => notification.sound(crate::sound_player::notification_sound_name(sound)),
            None => notification,
        };
        notification.show().map_err(|error| error.to_string())
    }
}

/// Reports the notification state precisely enough to be actionable.
///
/// Each value has a different remedy — banners turned off is a System Settings
/// toggle, an unregisterable build needs a Team ID, a denial needs the person to
/// change their mind — so they are deliberately not collapsed into one flag.
/// Breaks no longer depend on any of this, because they fall through to PausIO's
/// own surfaces, so this is diagnostics rather than a health gate.
#[cfg(target_os = "macos")]
pub(crate) fn notification_permission_state() -> String {
    crate::mac_notify::capability().as_health_state().into()
}

/// The configured cue for a reminder surfacing — Balanced's persistent
/// prompt, or a native notification in the quieter styles — or `None` when
/// the chosen sound timing does not include that moment.
#[cfg(desktop)]
pub(crate) fn reminder_cue(settings: &Settings) -> Option<SystemSound> {
    matches!(
        settings.sound_timing,
        pausio_core::SoundTiming::Banner | pausio_core::SoundTiming::Both
    )
    .then_some(settings.notification_sound_name)
}

/// Delivers an advisory nudge, falling back to PausIO's own toast when macOS
/// will not draw a banner.
///
/// Without the fallback these reminders were invisible: the only other surface
/// was a screen-reader-only announcement inside the main window, which is
/// normally hidden in the tray, so a sighted person saw nothing at all.
#[cfg(desktop)]
fn show_nudge(app: &AppHandle, settings: &Settings, nudge: &str, title: &str, body: &str) {
    if crate::is_e2e() {
        return;
    }
    if show_local_notification(app, title, body, reminder_cue(settings)).is_err() {
        crate::break_windows::show_nudge_toast(app, settings.locale, nudge);
    }
}

/// Sends the once-a-second countdown only to windows someone can actually
/// see. Previously every webview — including a hidden main window tucked
/// away in the tray — was invalidated every second forever; break-prompt and
/// break-overlay windows always receive ticks since they only exist for the
/// duration of an actual break.
pub(crate) fn emit_tick(app: &AppHandle, remaining: u32) {
    #[cfg(desktop)]
    {
        use crate::main_window::MAIN_WINDOW_VISIBLE;
        for label in app.webview_windows().keys() {
            let visible = match label.as_str() {
                "main" => MAIN_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed),
                _ => true,
            };
            if visible {
                let _ = app.emit_to(label.as_str(), "timer:tick", remaining);
            }
        }
    }
    #[cfg(not(desktop))]
    {
        let _ = app.emit("timer:tick", remaining);
    }
}

pub(crate) fn emit(app: &AppHandle, events: Vec<EngineEvent>, view: &EngineView) {
    #[cfg(desktop)]
    use pausio_core::DisplayTarget;
    #[cfg(desktop)]
    use pausio_core::Strictness;

    let mut should_persist = false;
    let mut history = vec![];
    for event in events {
        if view.settings.history_enabled
            && let Some(entry) = history_event(app, &event, &view.settings)
        {
            history.push(entry);
        }
        match event {
            EngineEvent::Tick(value) => {
                emit_tick(app, value);
            }
            EngineEvent::StateChanged(_) => {
                should_persist = true;
                #[cfg(desktop)]
                {
                    let phase = &view.snapshot.phase;
                    if !matches!(phase, pausio_protocol::TimerPhase::BreakDue { .. }) {
                        close_break_prompt(app);
                    }
                    if !matches!(phase, pausio_protocol::TimerPhase::Breaking { .. }) {
                        close_break_overlays(app);
                    }
                }
                let _ = app.emit("state:changed", &view.snapshot);
            }
            EngineEvent::Incoming(kind) => {
                #[cfg(desktop)]
                {
                    let locale = view.settings.locale;
                    let kind_label = match kind {
                        BreakKind::Short => crate::i18n::tray_break_kind_short(locale),
                        BreakKind::Long => crate::i18n::tray_break_kind_long(locale),
                    };
                    let _ = show_local_notification(
                        app,
                        crate::i18n::notification_incoming_title(locale),
                        &crate::i18n::notification_incoming_body(locale, kind_label),
                        reminder_cue(&view.settings),
                    );
                }
                let _ = app.emit("break:incoming", kind);
            }
            EngineEvent::BlinkNudge => {
                #[cfg(desktop)]
                {
                    let locale = view.settings.locale;
                    show_nudge(
                        app,
                        &view.settings,
                        "blink",
                        crate::i18n::notification_blink_title(locale),
                        crate::i18n::notification_blink_body(locale),
                    );
                }
                let _ = app.emit("nudge:blink", ());
            }
            EngineEvent::PostureNudge => {
                #[cfg(desktop)]
                {
                    let locale = view.settings.locale;
                    show_nudge(
                        app,
                        &view.settings,
                        "posture",
                        crate::i18n::notification_posture_title(locale),
                        crate::i18n::notification_posture_body(locale),
                    );
                }
                let _ = app.emit("nudge:posture", ());
            }
            EngineEvent::HydrationNudge => {
                #[cfg(desktop)]
                {
                    let locale = view.settings.locale;
                    show_nudge(
                        app,
                        &view.settings,
                        "hydration",
                        crate::i18n::notification_hydration_title(locale),
                        crate::i18n::notification_hydration_body(locale),
                    );
                }
                let _ = app.emit("nudge:hydration", ());
            }
            EngineEvent::Due(kind) => {
                #[cfg(desktop)]
                if !crate::is_e2e() {
                    close_break_prompt(app);
                    let locale = view.settings.locale;
                    // Firm and Strict promise a fullscreen reminder, and the
                    // engine raises it on the very next tick — a decision
                    // notification here would go stale before it could be
                    // read, so those styles get no interim surface at all.
                    let overlay_is_imminent = view.settings.display_target
                        != DisplayTarget::NotificationOnly
                        && matches!(
                            view.settings.strictness,
                            Strictness::Firm | Strictness::Strict
                        );
                    if !overlay_is_imminent {
                        if view.settings.strictness == Strictness::Balanced {
                            // Balanced asks through PausIO's own prompt: a
                            // bottom-right window that stays until it is
                            // answered, on every platform. The actionable OS
                            // notification it replaced could be dismissed,
                            // silenced, or never delivered at all — and the
                            // engine's due state now waits for the answer
                            // rather than timing out, so the surface has to
                            // be one that cannot vanish. A plain notification
                            // is the last resort if the window cannot be
                            // built at all.
                            if show_break_prompt(app, locale) {
                                if let Some(sound) = reminder_cue(&view.settings) {
                                    crate::sound_player::play_system_sound(sound);
                                }
                            } else {
                                let _ = show_local_notification(
                                    app,
                                    crate::i18n::notification_due_title(locale),
                                    crate::i18n::notification_due_body(locale),
                                    reminder_cue(&view.settings),
                                );
                            }
                        } else {
                            // The quieter styles just get told. This is an
                            // invitation, never a gate: the engine starts the
                            // break on its own once the grace period is up.
                            let delivered = show_local_notification(
                                app,
                                crate::i18n::notification_due_title(locale),
                                crate::i18n::notification_due_body(locale),
                                reminder_cue(&view.settings),
                            )
                            .is_ok();
                            // PausIO's own prompt is the fallback whenever macOS
                            // will not draw a banner — an unregistered app, a
                            // denied permission, or an alert style of "None".
                            if !delivered {
                                show_break_prompt(app, locale);
                            }
                        }
                    }
                }
                let _ = app.emit("break:due", kind);
            }
            EngineEvent::Started(kind) => {
                // start_break sets `remaining` to the break length before pushing
                // Started, so the snapshot here is the full break duration.
                #[cfg(desktop)]
                if view.settings.strictness == Strictness::Gentle
                    || view.settings.display_target == DisplayTarget::NotificationOnly
                {
                    let locale = view.settings.locale;
                    let delivered = show_local_notification(
                        app,
                        crate::i18n::notification_started_title(locale),
                        crate::i18n::notification_started_body(locale),
                        reminder_cue(&view.settings),
                    )
                    .is_ok();
                    // Gentle asks for a calm cue, not for the break to pass
                    // unnoticed. When macOS will not draw that cue the overlay
                    // is the only thing left — the one exception being
                    // notification-only delivery, where a window on screen is
                    // precisely what the person ruled out.
                    if !delivered
                        && view.settings.display_target != DisplayTarget::NotificationOnly
                        && !crate::is_e2e()
                    {
                        show_break_overlays(
                            app,
                            locale,
                            view.snapshot.remaining_seconds,
                            view.settings.display_target,
                        );
                    }
                } else if !crate::is_e2e() {
                    show_break_overlays(
                        app,
                        view.settings.locale,
                        view.snapshot.remaining_seconds,
                        view.settings.display_target,
                    );
                }
                #[cfg(desktop)]
                set_quit_enabled(view.settings.strictness != Strictness::Strict);
                let _ = app.emit("break:started", kind);
            }
            EngineEvent::Ended(kind) => {
                #[cfg(desktop)]
                {
                    crate::break_windows::close_break_windows(app);
                    // A break can end with only a full-screen overlay on
                    // screen and no notification popup to carry a sound, so
                    // the cue is played directly rather than left to a
                    // notification — but only once, and only here: the pause
                    // must have actually run its course, never at its start
                    // and never when it is skipped early.
                    if matches!(
                        view.settings.sound_timing,
                        pausio_core::SoundTiming::End | pausio_core::SoundTiming::Both
                    ) {
                        crate::sound_player::play_system_sound(
                            view.settings.notification_sound_name,
                        );
                    }
                }
                #[cfg(desktop)]
                set_quit_enabled(true);
                let _ = app.emit("break:ended", kind);
            }
            EngineEvent::Skipped(kind) => {
                #[cfg(desktop)]
                crate::break_windows::close_break_windows(app);
                #[cfg(desktop)]
                set_quit_enabled(true);
                let _ = app.emit("break:skipped", kind);
            }
            EngineEvent::Postponed(kind) => {
                let _ = app.emit("break:postponed", kind);
            }
            EngineEvent::ContextDeferred { kind, reason } => {
                let _ = app.emit(
                    "break:deferred",
                    serde_json::json!({ "kind": kind, "reason": reason }),
                );
            }
        }
    }
    if should_persist {
        // A persistence failure must never prevent a break from ending or
        // leave an overlay on screen. The next successful state transition
        // repairs the checkpoint; the UI surfaces durable settings failures.
        let _ = persist_session(app, &view.checkpoint);
    }
    if !history.is_empty() {
        let _ = append_history(app, history, view.settings.history_retention_days);
    }
    #[cfg(desktop)]
    update_tray_state(view);
}

#[cfg(test)]
mod tests {
    #[cfg(mobile)]
    use chrono::{TimeZone, Utc};
    #[cfg(mobile)]
    use pausio_protocol::{BreakKind, TimerPhase, WatchPhase};

    #[cfg(mobile)]
    use super::build_watch_settings_envelope;

    #[cfg(mobile)]
    fn snapshot(phase: TimerPhase, remaining_seconds: u32) -> pausio_core::Snapshot {
        pausio_core::Snapshot {
            phase,
            remaining_seconds,
            completed_short_breaks: 0,
            postpones_today: 0,
            context: None,
            context_expires_at: None,
            paused_until: None,
        }
    }

    #[cfg(mobile)]
    #[test]
    fn watch_projection_uses_one_deadline_for_an_active_break() {
        let now = Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap();
        let envelope = build_watch_settings_envelope(
            &snapshot(
                TimerPhase::Breaking {
                    kind: BreakKind::Long,
                },
                300,
            ),
            &pausio_core::Settings::default(),
            9,
            now,
        );

        assert_eq!(envelope.phase, Some(WatchPhase::Breaking));
        assert_eq!(envelope.break_kind, Some(BreakKind::Long));
        assert!(envelope.break_active);
        assert_eq!(envelope.next_break_at, envelope.phase_deadline_at);
        assert_eq!(
            envelope.phase_deadline_at,
            Some(now + chrono::Duration::seconds(300))
        );
    }

    #[cfg(mobile)]
    #[test]
    fn paused_watch_projection_has_no_stale_deadline() {
        let envelope = build_watch_settings_envelope(
            &snapshot(
                TimerPhase::Paused {
                    reason: pausio_protocol::PauseReason::Manual,
                },
                20,
            ),
            &pausio_core::Settings::default(),
            10,
            Utc::now(),
        );

        assert_eq!(envelope.phase, Some(WatchPhase::Paused));
        assert!(envelope.paused);
        assert_eq!(envelope.phase_deadline_at, None);
    }
}
