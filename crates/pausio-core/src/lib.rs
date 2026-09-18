//! Product timing logic. This crate deliberately knows nothing about Tauri or a UI.

mod engine;
mod reminders;
mod settings;
mod types;

pub use engine::{TimerClock, TimerDriver, TimerEngine, due_grace_seconds};
pub use pausio_protocol::{AlertTarget, ReminderKind, ReminderScheduleReport, ReminderSlot};
pub use reminders::reminder_plan;
pub use settings::{
    Accent, BreakRoutine, DisplayTarget, Locale, Settings, SettingsError, SoundTiming, Strictness,
    SystemSound, Theme,
};
pub use types::{EngineError, EngineEvent, SESSION_SCHEMA_VERSION, SessionCheckpoint, Snapshot};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::GENTLE_DUE_GRACE_SECONDS;
    use chrono::{DateTime, Datelike, Local, Timelike, Utc};
    use pausio_protocol::{BreakKind, ContextReason, PauseReason, TimerPhase};

    #[derive(Debug)]
    struct FakeClock {
        monotonic: u64,
        wall: DateTime<Local>,
    }
    impl TimerClock for FakeClock {
        fn monotonic_seconds(&self) -> u64 {
            self.monotonic
        }
        fn wall_now(&self) -> DateTime<Local> {
            self.wall
        }
    }
    fn active_now() -> chrono::DateTime<Local> {
        Local::now()
    }
    fn engine() -> TimerEngine {
        let settings = Settings {
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        TimerEngine::new(settings, active_now()).unwrap()
    }
    #[test]
    fn defaults_are_valid() {
        Settings::default().validate().unwrap();
    }

    /// A fresh install must alert on the device it is installed on. Defaulting
    /// to the watch, or to an implicit connection, would make an unpaired
    /// phone silently do nothing at all.
    #[test]
    fn a_new_install_is_standalone_and_connects_no_wearable() {
        let settings = Settings::default();
        assert_eq!(settings.alert_target, AlertTarget::Phone);
        assert!(!settings.watch_enabled);
        assert!(settings.alert_target.alerts_on_phone());
        assert!(!settings.alert_target.alerts_on_watch());
    }

    #[test]
    fn alert_target_routing_is_explicit_for_each_choice() {
        assert!(AlertTarget::Phone.alerts_on_phone());
        assert!(!AlertTarget::Phone.alerts_on_watch());
        // Choosing the watch keeps the phone quiet on purpose: the point is a
        // private haptic, not a banner on a screen that may be shared.
        assert!(!AlertTarget::Watch.alerts_on_phone());
        assert!(AlertTarget::Watch.alerts_on_watch());
        assert!(AlertTarget::Both.alerts_on_phone());
        assert!(AlertTarget::Both.alerts_on_watch());
    }

    /// Both fields are additive, so settings written by an earlier build must
    /// still load — and must land on the standalone defaults when they do.
    #[test]
    fn settings_saved_before_these_fields_existed_still_load() {
        let mut stored = serde_json::to_value(Settings::default()).unwrap();
        let object = stored.as_object_mut().unwrap();
        object.remove("alert_target");
        object.remove("watch_enabled");
        let decoded: Settings = serde_json::from_value(stored).unwrap();
        assert_eq!(decoded.alert_target, AlertTarget::Phone);
        assert!(!decoded.watch_enabled);
        decoded.validate().unwrap();
    }

    /// The phone is suspended constantly; that must read as ordinary elapsed
    /// time, never as the person having walked away.
    #[test]
    fn reconciling_after_suspension_makes_the_break_due_instead_of_pausing() {
        let mut engine = engine();
        let settings = engine.settings().clone();
        let events = engine.reconcile_to(settings.work_seconds + 90, active_now());
        assert!(matches!(
            engine.snapshot().phase,
            TimerPhase::BreakDue { .. }
        ));
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Due(_))));
        // The desktop's woke_after() would have produced this; on a phone it
        // would stop the timer every time the screen turned off.
        assert!(!matches!(
            engine.snapshot().phase,
            TimerPhase::Paused { .. }
        ));
    }

    /// A phone left closed overnight must not replay every interval it missed,
    /// which would inflate break counters and fire a burst of transitions.
    #[test]
    fn a_long_suspension_yields_one_transition_not_a_replay() {
        let mut engine = engine();
        engine.reconcile_to(12 * 60 * 60, active_now());
        let snapshot = engine.snapshot();
        assert!(matches!(snapshot.phase, TimerPhase::BreakDue { .. }));
        assert_eq!(snapshot.completed_short_breaks, 0);
    }

    /// A break whose duration elapsed while the app was suspended was taken:
    /// the person was demonstrably not looking at the phone.
    #[test]
    fn a_break_that_elapsed_while_suspended_counts_as_taken() {
        let mut engine = engine();
        engine.take_break_now().unwrap();
        let events = engine.reconcile_to(600, active_now());
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Ended(_))));
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Skipped(_))));
        assert!(matches!(engine.snapshot().phase, TimerPhase::Working));
    }

    /// Elapsed time is not consent to resume. A deliberate hold has to survive
    /// any amount of suspension.
    #[test]
    fn reconciling_never_overrides_a_manual_pause() {
        let mut engine = engine();
        engine.pause(PauseReason::Manual).unwrap();
        engine.reconcile_to(6 * 60 * 60, active_now());
        assert!(matches!(
            engine.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Manual
            }
        ));
    }

    #[test]
    fn reconciling_an_already_due_break_keeps_it_due() {
        let mut engine = engine();
        let settings = engine.settings().clone();
        engine.reconcile_to(settings.work_seconds, active_now());
        assert!(matches!(
            engine.snapshot().phase,
            TimerPhase::BreakDue { .. }
        ));
        engine.reconcile_to(3_600, active_now());
        assert!(matches!(
            engine.snapshot().phase,
            TimerPhase::BreakDue { .. }
        ));
    }

    /// Reopening the app a few seconds later is the common case and must not
    /// jump the timer forward.
    #[test]
    fn a_brief_suspension_only_advances_the_countdown() {
        let mut engine = engine();
        let before = engine.snapshot().remaining_seconds;
        engine.reconcile_to(30, active_now());
        let after = engine.snapshot().remaining_seconds;
        assert_eq!(before - after, 30);
        assert!(matches!(engine.snapshot().phase, TimerPhase::Working));
    }

    #[test]
    fn new_install_records_no_history_until_asked() {
        let settings = Settings::default();
        // The README promises "optional local history: off by default".
        assert!(!settings.history_enabled);
        // Retention still has a sane value for the moment recording is switched on.
        assert_eq!(settings.history_retention_days, Some(365));
    }

    /// Defaults are the only configuration most people ever run, so each of these is
    /// a deliberate product decision rather than an incidental value.
    #[test]
    fn new_install_defaults_are_audible_calm_and_context_aware() {
        let settings = Settings::default();
        // A completed break is audible out of the box: a silent default plus an
        // unsigned macOS build (where notifications never register) left a
        // first-run user with no perceptible cue whatsoever.
        assert_eq!(settings.sound_timing, SoundTiming::End);
        // A blink nudge every 10 minutes on top of a break every 20 is roughly four
        // interruptions an hour before the person has chosen anything.
        assert_eq!(settings.blink_nudge_minutes, None);
        assert_eq!(settings.posture_nudge_minutes, None);
        assert_eq!(settings.hydration_nudge_minutes, None);
        // 20-20-20 should not quietly ship a Pomodoro long break.
        assert_eq!(settings.long_break_every, None);
        // Unlimited postponing lets the timer never actually happen.
        assert_eq!(settings.postpone_limit, Some(3));
        // Deferring a break during a presentation matters more than either flag costs.
        assert!(settings.auto_detect_fullscreen);
        assert!(settings.auto_detect_do_not_disturb);
        // CmdOrCtrl+X is Cut; registering it globally on first launch swallowed it.
        assert_eq!(
            settings.end_break_shortcut.as_deref(),
            Some("CmdOrCtrl+Shift+P")
        );
    }
    #[test]
    fn invalid_durations_rejected() {
        let s = Settings {
            work_seconds: 1,
            ..Default::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::WorkDuration));
    }
    #[test]
    fn validates_local_break_messages_without_accepting_empty_or_unbounded_content() {
        let mut settings = Settings {
            break_messages: vec![
                "Look out the window.".into(),
                "Relax your shoulders.".into(),
            ],
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        settings.break_messages = vec!["   ".into()];
        assert_eq!(settings.validate(), Err(SettingsError::BreakMessages));
        settings.break_messages = vec!["x".repeat(121)];
        assert_eq!(settings.validate(), Err(SettingsError::BreakMessages));
    }
    #[test]
    fn blink_nudge_occurs_only_during_work_and_respects_the_selected_interval() {
        let settings = Settings {
            blink_nudge_minutes: Some(5),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        let events = e.advance(5 * 60, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::BlinkNudge))
        );
        e.take_break_now().unwrap();
        assert!(
            !e.advance(5 * 60, active_now())
                .iter()
                .any(|event| matches!(event, EngineEvent::BlinkNudge))
        );
    }
    #[test]
    fn hydration_nudge_occurs_only_during_work_and_respects_the_selected_interval() {
        let settings = Settings {
            hydration_nudge_minutes: Some(15),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        let events = e.advance(15 * 60, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::HydrationNudge))
        );
        e.take_break_now().unwrap();
        assert!(
            !e.advance(15 * 60, active_now())
                .iter()
                .any(|event| matches!(event, EngineEvent::HydrationNudge))
        );
    }
    #[test]
    fn hydration_nudge_bound_is_validated() {
        assert_eq!(
            Settings {
                hydration_nudge_minutes: Some(14),
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::HydrationNudge)
        );
        assert!(
            Settings {
                hydration_nudge_minutes: Some(15),
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
    }
    #[test]
    fn validates_each_independent_settings_bound() {
        assert_eq!(
            Settings {
                short_break_seconds: 4,
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::ShortBreak)
        );
        assert_eq!(
            Settings {
                long_break_seconds: 59,
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::LongBreak)
        );
        assert_eq!(
            Settings {
                long_break_every: Some(1),
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::Cadence)
        );
        assert_eq!(
            Settings {
                pre_break_seconds: 7,
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::Warning)
        );
        assert_eq!(
            Settings {
                active_days_mask: 0,
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::WorkingHours)
        );
    }
    #[test]
    fn global_shortcuts_reject_blank_or_overlong_accelerators_but_allow_disabling() {
        assert!(
            Settings {
                end_break_shortcut: None,
                pause_toggle_shortcut: None,
                take_break_shortcut: None,
                ..Default::default()
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            Settings {
                end_break_shortcut: Some("   ".into()),
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::GlobalShortcut)
        );
        assert_eq!(
            Settings {
                pause_toggle_shortcut: Some("x".repeat(41)),
                ..Default::default()
            }
            .validate(),
            Err(SettingsError::GlobalShortcut)
        );
    }
    #[test]
    fn warning_then_short_break() {
        let mut e = engine();
        e.remaining = 31;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::PreBreak));
        e.advance(30, active_now());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::BreakDue {
                kind: BreakKind::Short
            }
        ));
    }
    #[test]
    fn fourth_break_is_long() {
        let mut e = engine();
        // The cadence is opt-in now, so a test of the cadence must enable it.
        e.settings.long_break_every = Some(4);
        e.completed_breaks = 3;
        e.completed_short_breaks = 3;
        e.take_break_now().unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Long
            }
        ));
    }
    #[test]
    fn manual_break_is_available_outside_working_hours() {
        let settings = Settings::default();
        let mut engine = TimerEngine {
            settings,
            phase: TimerPhase::Dormant,
            remaining: 0,
            completed_breaks: 0,
            completed_short_breaks: 0,
            postpones_today: 0,
            manual_session: false,
            lock_context: None,
            context: None,
            context_expires_at: None,
            local_day: active_now().date_naive(),
            paused_until: None,
            automatic_deferrals_today: 0,
            fixed_breaks_seen_today: vec![],
            work_seconds_today: 0,
            blink_remaining: None,
            posture_remaining: None,
            hydration_remaining: None,
            due_grace_remaining: None,
        };
        engine.take_break_now().unwrap();
        assert!(matches!(
            engine.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Short
            }
        ));
        engine.advance(1, active_now());
        assert_eq!(engine.snapshot().remaining_seconds, 19);
    }
    #[test]
    fn postpone_is_two_minutes_and_only_available_when_a_break_is_due() {
        let mut e = engine();
        assert_eq!(e.postpone(), Err(EngineError::InvalidTransition));
        e.remaining = 1;
        e.advance(1, active_now());
        e.postpone().unwrap();
        assert_eq!(e.snapshot().remaining_seconds, 120);
    }
    #[test]
    fn due_prompt_can_postpone_without_crediting_a_break() {
        let mut e = engine();
        e.remaining = 1;
        e.advance(1, active_now());
        let events = e.postpone().unwrap();
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, 2 * 60);
        assert_eq!(e.snapshot().completed_short_breaks, 0);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Tick(120)))
        );
        e.advance(120, active_now());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::BreakDue {
                kind: BreakKind::Short
            }
        ));
    }
    #[test]
    fn gentle_delivery_auto_starts_a_due_break_after_a_bounded_grace_period() {
        let settings = Settings {
            strictness: Strictness::Gentle,
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        // Well within the grace period: still waiting, not started.
        e.advance(GENTLE_DUE_GRACE_SECONDS - 1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        // Crossing the grace threshold starts the break without a person
        // ever acting on the notification.
        let events = e.advance(1, active_now());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Short
            }
        ));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Started(BreakKind::Short)))
        );
    }
    #[test]
    fn gentle_delivery_never_auto_starts_a_break_deferred_by_an_active_context() {
        let settings = Settings {
            strictness: Strictness::Gentle,
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.set_context_for(ContextReason::Meeting, 60).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        e.advance(GENTLE_DUE_GRACE_SECONDS * 10, active_now());
        // A context-deferred break stays deferred no matter how long the
        // grace window would otherwise allow — Gentle delivery must never
        // override a person's own quiet-context choice.
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
    }
    #[test]
    fn balanced_delivery_waits_for_the_persistent_prompt_to_be_answered() {
        let mut e = engine();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        // Balanced raises PausIO's own prompt, a window that stays on screen
        // until answered — so the due state itself waits indefinitely rather
        // than handing over to the overlay on a timeout.
        e.advance(GENTLE_DUE_GRACE_SECONDS * 10, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        assert_eq!(e.due_grace_remaining, None);
        // Answering the prompt is what starts the break.
        let events = e.start_due_break().unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Short
            }
        ));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Started(BreakKind::Short)))
        );
    }
    /// Firm and Strict advertise a fullscreen reminder, so a due break must start
    /// on its own rather than waiting for a notification to be acknowledged: if the
    /// OS refuses to deliver the notification at all, the break must still happen.
    #[test]
    fn assertive_delivery_starts_a_due_break_without_waiting_to_be_acknowledged() {
        for strictness in [Strictness::Firm, Strictness::Strict] {
            let settings = Settings {
                strictness,
                active_days_mask: 0b0111_1111,
                active_start_minutes: 0,
                active_end_minutes: 0,
                ..Default::default()
            };
            let mut e = TimerEngine::new(settings, active_now()).unwrap();
            e.remaining = 1;
            e.advance(1, active_now());
            assert!(
                matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }),
                "{strictness:?} should still record the break as due"
            );
            let events = e.advance(1, active_now());
            assert!(
                matches!(
                    e.snapshot().phase,
                    TimerPhase::Breaking {
                        kind: BreakKind::Short
                    }
                ),
                "{strictness:?} should raise the overlay on the next tick"
            );
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, EngineEvent::Started(BreakKind::Short))),
                "{strictness:?} should emit Started"
            );
        }
    }
    /// The guarantee must not override a person's own quiet context, whatever
    /// their reminder style — this is the one thing allowed to hold a break.
    #[test]
    fn assertive_delivery_still_respects_an_active_context() {
        let settings = Settings {
            strictness: Strictness::Strict,
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.set_context_for(ContextReason::Meeting, 60).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        e.advance(GENTLE_DUE_GRACE_SECONDS * 10, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
    }
    /// Pins the whole table in one place: every reminder style whose surface
    /// can disappear unnoticed (a dismissible OS notification, or nothing at
    /// all under the assertive styles) resolves to a *bounded* grace — an
    /// unbounded one is how a break stopped happening at all when the OS
    /// refused to deliver its notification. Balanced is the one exception:
    /// its prompt is persistent by design, so the due state waits to be
    /// answered instead of timing out.
    #[test]
    fn every_delivery_style_resolves_to_its_due_grace() {
        let of = |strictness, display_target| {
            due_grace_seconds(&Settings {
                strictness,
                display_target,
                ..Default::default()
            })
        };
        for strictness in [
            Strictness::Gentle,
            Strictness::Balanced,
            Strictness::Firm,
            Strictness::Strict,
        ] {
            // Notification-only has no persistent window to lean on, so it
            // keeps the unhurried bounded grace whatever the style says.
            assert_eq!(
                of(strictness, DisplayTarget::NotificationOnly),
                Some(GENTLE_DUE_GRACE_SECONDS),
                "{strictness:?} + notification-only"
            );
        }
        assert_eq!(
            of(Strictness::Gentle, DisplayTarget::All),
            Some(GENTLE_DUE_GRACE_SECONDS)
        );
        assert_eq!(of(Strictness::Balanced, DisplayTarget::All), None);
        assert_eq!(of(Strictness::Firm, DisplayTarget::All), Some(0));
        assert_eq!(of(Strictness::Strict, DisplayTarget::All), Some(0));
    }
    /// Notification-only delivery has no overlay to fall through to, so it
    /// keeps the unhurried grace even under an assertive reminder style.
    #[test]
    fn notification_only_delivery_keeps_the_long_grace_under_strict() {
        let settings = Settings {
            strictness: Strictness::Strict,
            display_target: DisplayTarget::NotificationOnly,
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        e.advance(GENTLE_DUE_GRACE_SECONDS - 1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Breaking { .. }));
    }
    #[test]
    fn legacy_settings_without_a_sound_timing_default_to_break_end() {
        let value: serde_json::Value = serde_json::json!({
            "work_seconds": 1200,
            "short_break_seconds": 20,
            "long_break_seconds": 300,
            "long_break_every": null,
            "pre_break_seconds": 30,
            "active_days_mask": 62,
            "active_start_minutes": 540,
            "active_end_minutes": 1080,
            "postpone_limit": 3
        });
        let settings: Settings = serde_json::from_value(value).unwrap();
        assert_eq!(settings.sound_timing, SoundTiming::End);
        assert_eq!(settings.notification_sound_name, SystemSound::Default);
    }
    #[test]
    fn idle_reset_pauses_cleanly() {
        let mut e = engine();
        e.report_idle(901).unwrap();
        assert_eq!(e.snapshot().remaining_seconds, 1200);
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Idle
            }
        ));
    }
    #[test]
    fn natural_idle_break_credits_a_short_break() {
        let mut e = engine();
        e.take_break_now().unwrap();
        e.report_idle(20).unwrap();
        assert_eq!(e.snapshot().completed_short_breaks, 1);
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
    }
    #[test]
    fn pause_resume_retains_remaining_work_time() {
        let mut e = engine();
        e.advance(17, active_now());
        let expected = e.snapshot().remaining_seconds;
        e.pause(PauseReason::Manual).unwrap();
        e.resume().unwrap();
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, expected);
    }
    #[test]
    fn invalid_transitions_are_rejected() {
        let mut e = engine();
        assert_eq!(e.resume(), Err(EngineError::InvalidTransition));
        assert_eq!(e.skip_break(), Err(EngineError::InvalidTransition));
        e.pause(PauseReason::Manual).unwrap();
        assert_eq!(e.postpone(), Err(EngineError::InvalidTransition));
        assert_eq!(e.start_due_break(), Err(EngineError::InvalidTransition));
    }
    #[test]
    fn postpone_limit_is_enforced() {
        let mut settings = engine().settings().clone();
        settings.postpone_limit = Some(1);
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        e.postpone().unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        assert_eq!(e.postpone(), Err(EngineError::InvalidTransition));
    }
    #[test]
    fn active_input_deferral_is_short_and_bounded_without_consuming_user_postpones() {
        let mut e = engine();
        for _ in 0..4 {
            e.remaining = 1;
            e.advance(1, active_now());
            let events = e.defer_due_for_active_input(15).unwrap();
            assert!(events.iter().any(|event| matches!(
                event,
                EngineEvent::ContextDeferred {
                    reason: ContextReason::ActiveInput,
                    ..
                }
            )));
            assert_eq!(e.snapshot().postpones_today, 0);
            assert_eq!(e.snapshot().remaining_seconds, 15);
        }
        e.remaining = 1;
        e.advance(1, active_now());
        assert_eq!(
            e.defer_due_for_active_input(15),
            Err(EngineError::InvalidTransition)
        );
    }
    #[test]
    fn dormant_timer_enters_working_only_inside_active_schedule() {
        let settings = Settings {
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.phase = TimerPhase::Dormant;
        e.remaining = 0;
        let events = e.advance(1, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::StateChanged(TimerPhase::Working)))
        );
    }
    #[test]
    fn sleep_does_not_trigger_a_break() {
        let mut e = engine();
        e.woke_after(30).unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Sleep
            }
        ));
    }
    #[test]
    fn exact_deadline_waits_for_the_user_to_start_the_break() {
        let mut e = engine();
        e.remaining = 1;
        let events = e.advance(1, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(BreakKind::Short)))
        );
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::BreakDue {
                kind: BreakKind::Short
            }
        ));
        e.start_due_break().unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Short
            }
        ));
    }
    #[test]
    fn long_break_cadence_resets_after_a_long_break() {
        let mut e = engine();
        e.completed_breaks = 3;
        e.completed_short_breaks = 3;
        e.take_break_now().unwrap();
        e.skip_break().unwrap();
        e.take_break_now().unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Breaking {
                kind: BreakKind::Short
            }
        ));
    }
    #[test]
    fn skip_break_emits_skipped_not_ended() {
        let mut e = engine();
        e.take_break_now().unwrap();
        let events = e.skip_break().unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Skipped(BreakKind::Short)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Ended(_)))
        );
    }
    #[test]
    fn a_break_that_runs_its_course_emits_ended_not_skipped() {
        let mut e = engine();
        e.take_break_now().unwrap();
        e.remaining = 1;
        let events = e.advance(1, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Ended(BreakKind::Short)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Skipped(_)))
        );
    }
    #[test]
    fn a_break_finished_by_prolonged_idle_absence_emits_ended_not_skipped() {
        let mut e = engine();
        e.take_break_now().unwrap();
        let events = e.report_idle(e.settings().short_break_seconds).unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Ended(BreakKind::Short)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Skipped(_)))
        );
    }
    #[test]
    fn a_break_finished_by_waking_after_sleep_emits_ended_not_skipped() {
        let mut e = engine();
        e.take_break_now().unwrap();
        let events = e.woke_after(e.settings().short_break_seconds).unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Ended(BreakKind::Short)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Skipped(_)))
        );
    }
    #[test]
    fn a_locked_break_that_completes_from_the_lock_duration_emits_ended_not_skipped() {
        let mut e = engine();
        e.take_break_now().unwrap();
        let short_break_seconds = e.settings().short_break_seconds;
        e.screen_locked();
        let events = e.screen_unlocked(short_break_seconds, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Ended(BreakKind::Short)))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Skipped(_)))
        );
    }
    #[test]
    fn driver_uses_monotonic_elapsed_time_and_does_not_emit_without_progress() {
        let now = active_now();
        let mut e = engine();
        let mut driver = TimerDriver::new(FakeClock {
            monotonic: 100,
            wall: now,
        });
        assert!(driver.poll(&mut e).is_empty());
        driver.clock_mut().monotonic += 2;
        let events = driver.poll(&mut e);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, EngineEvent::Tick(_)))
                .count(),
            1
        );
        assert_eq!(e.snapshot().remaining_seconds, 1198);
    }
    #[test]
    fn dormant_session_is_ready_and_can_be_started_manually() {
        let today = active_now().weekday().num_days_from_sunday();
        let settings = Settings {
            active_days_mask: 1 << ((today + 1) % 7),
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        assert!(matches!(e.snapshot().phase, TimerPhase::Dormant));
        assert_eq!(e.snapshot().remaining_seconds, 1200);
        e.start_session().unwrap();
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, 1200);
        e.advance(1, active_now());
        assert_eq!(e.snapshot().remaining_seconds, 1199);
    }
    #[test]
    fn pause_never_freezes_a_break_or_break_prompt() {
        let mut e = engine();
        e.take_break_now().unwrap();
        assert_eq!(
            e.pause(PauseReason::Manual),
            Err(EngineError::InvalidTransition)
        );
        e.skip_break().unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        assert_eq!(
            e.pause(PauseReason::Manual),
            Err(EngineError::InvalidTransition)
        );
    }
    #[test]
    fn screen_lock_credits_locked_time_back_to_the_work_countdown() {
        let mut e = engine();
        e.advance(19, active_now());
        let remaining = e.snapshot().remaining_seconds;
        e.screen_locked();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::ScreenLock
            }
        ));
        e.screen_unlocked(17, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        // Time behind a locked screen is time away from the display, so it winds
        // the countdown back up rather than consuming it.
        assert_eq!(e.snapshot().remaining_seconds, remaining + 17);
        assert_eq!(e.snapshot().completed_short_breaks, 0);
    }
    /// The product example: 20 minutes on the clock, five left when the screen
    /// locks, away for half an hour — come back to a full interval, not a break
    /// prompt and not a stale five minutes.
    #[test]
    fn a_lock_longer_than_the_interval_returns_to_a_full_countdown() {
        let mut e = engine();
        let work_seconds = e.settings.work_seconds;
        e.advance(work_seconds - 5 * 60, active_now());
        assert_eq!(e.snapshot().remaining_seconds, 5 * 60);
        e.screen_locked();
        let events = e.screen_unlocked(30 * 60, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, work_seconds);
        // Credited back, never over-credited: the cap is a full interval.
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_) | EngineEvent::Started(_)))
        );
        assert_eq!(e.completed_breaks, 0);
        assert_eq!(e.snapshot().completed_short_breaks, 0);
    }
    /// The counterpart: a brief lock gives back exactly what it took, with no
    /// minimum-duration threshold to fall under.
    #[test]
    fn a_short_lock_credits_back_exactly_its_own_duration() {
        let mut e = engine();
        let work_seconds = e.settings.work_seconds;
        e.advance(work_seconds - 5 * 60, active_now());
        e.screen_locked();
        e.screen_unlocked(2 * 60, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, 7 * 60);
    }
    #[test]
    fn a_lock_that_reaches_the_cap_leaves_break_cadence_untouched() {
        let mut e = engine();
        e.settings.long_break_every = Some(4);
        e.completed_breaks = 3;
        e.advance(19, active_now());
        e.screen_locked();
        let events = e.screen_unlocked(e.settings.work_seconds * 2, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, e.settings.work_seconds);
        assert_eq!(e.snapshot().completed_short_breaks, 0);
        // A rewind is not a break: it must not advance the cadence counter, so
        // the long break still lands on the interval it was already due on.
        assert_eq!(e.completed_breaks, 3);
        assert_eq!(e.next_kind(), BreakKind::Long);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_) | EngineEvent::Started(_)))
        );
    }
    #[test]
    fn lock_clears_an_already_due_break_without_reshowing_the_prompt() {
        let mut e = engine();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        e.screen_locked();
        let events = e.screen_unlocked(1, active_now());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_) | EngineEvent::Started(_)))
        );
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, e.settings.work_seconds);
        assert_eq!(e.completed_breaks, 0);
    }
    /// Locking inside the pre-break warning window lifts the countdown back out
    /// of it: the heads-up already shown is no longer imminent, so the phase has
    /// to fall back to Working rather than leaving a stale warning on screen.
    #[test]
    fn lock_that_leaves_warning_range_restores_working_without_due_event() {
        let mut e = engine();
        e.remaining = e.settings.pre_break_seconds;
        e.phase = TimerPhase::PreBreak;
        e.screen_locked();
        let events = e.screen_unlocked(10, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(
            e.snapshot().remaining_seconds,
            e.settings.pre_break_seconds + 10
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_) | EngineEvent::Incoming(_)))
        );
    }
    /// A lock too short to clear the warning window keeps the phase in PreBreak
    /// rather than flapping it back to Working for a handful of seconds.
    #[test]
    fn a_lock_that_stays_inside_warning_range_remains_pre_break() {
        let mut e = engine();
        e.remaining = e.settings.pre_break_seconds - 10;
        e.phase = TimerPhase::PreBreak;
        e.screen_locked();
        let events = e.screen_unlocked(5, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::PreBreak));
        assert_eq!(
            e.snapshot().remaining_seconds,
            e.settings.pre_break_seconds - 5
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_) | EngineEvent::Incoming(_)))
        );
    }
    #[test]
    fn lock_reduces_an_active_break_and_restores_its_overlay_signal() {
        let mut e = engine();
        e.take_break_now().unwrap();
        let events = e.screen_locked();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::StateChanged(_)))
        );
        let events = e.screen_unlocked(5, active_now());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Started(BreakKind::Short)))
        );
        assert_eq!(
            e.snapshot().remaining_seconds,
            e.settings.short_break_seconds - 5
        );

        e.screen_locked();
        e.screen_unlocked(e.settings.short_break_seconds, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.completed_breaks, 1);
    }
    /// A locked screen looks idle to every platform's idle counter. The idle
    /// path must not act on that: its fifteen-minute branch resets `remaining`
    /// and replaces the pause reason, which would discard the lock context and
    /// silently cancel the rewind for exactly the long absences the rewind is
    /// for.
    #[test]
    fn idle_reports_never_disturb_a_screen_lock() {
        let mut e = engine();
        let work_seconds = e.settings.work_seconds;
        e.advance(work_seconds - 5 * 60, active_now());
        e.screen_locked();

        assert!(e.report_idle(20 * 60).unwrap().is_empty());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::ScreenLock
            }
        ));

        // The lock context survived, so unlocking still credits the time back.
        e.screen_unlocked(20 * 60, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, work_seconds);
    }
    #[test]
    fn lock_preserves_manual_pause_and_unmatched_unlock_is_a_no_op() {
        let mut e = engine();
        e.advance(19, active_now());
        let remaining = e.snapshot().remaining_seconds;
        e.pause(PauseReason::Manual).unwrap();
        e.screen_locked();
        assert!(e.screen_locked().is_empty());
        e.screen_unlocked(e.settings.short_break_seconds * 2, active_now());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Manual
            }
        ));
        assert_eq!(e.snapshot().remaining_seconds, remaining);
        assert!(e.screen_unlocked(999, active_now()).is_empty());
    }
    #[test]
    fn unlock_stays_ready_when_no_scheduled_or_manual_session_is_active() {
        let today = active_now().weekday().num_days_from_sunday();
        let settings = Settings {
            active_days_mask: 1u8 << ((today + 1) % 7),
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.screen_locked();
        e.screen_unlocked(e.settings.short_break_seconds, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Dormant));
        assert_eq!(e.snapshot().remaining_seconds, 1200);
    }
    #[test]
    fn context_defers_a_due_break_and_surfaces_it_when_the_context_clears() {
        let mut e = engine();
        e.set_context(Some(ContextReason::ScreenShare));
        let events = e.advance(e.settings.work_seconds, active_now());
        assert!(events.iter().any(|event| matches!(
            event,
            EngineEvent::ContextDeferred {
                kind: BreakKind::Short,
                reason: ContextReason::ScreenShare
            }
        )));
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        assert_eq!(e.snapshot().context, Some(ContextReason::ScreenShare));
        let events = e.set_context(None);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(BreakKind::Short)))
        );
    }
    #[test]
    fn timed_context_clears_without_retaining_a_stale_quiet_mode() {
        let mut e = engine();
        e.set_context_for(ContextReason::Meeting, 15).unwrap();
        assert!(e.snapshot().context.is_some());
        let events = e.advance(1, active_now() + chrono::Duration::minutes(16));
        assert!(e.snapshot().context.is_none());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::StateChanged(_)))
        );
    }
    #[test]
    fn fixed_break_time_is_due_once_per_local_day() {
        let now = active_now();
        let minute = (now.hour() * 60 + now.minute()) as u16;
        let settings = Settings {
            fixed_break_minutes: vec![minute],
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, now).unwrap();
        let first = e.advance(1, now);
        assert!(
            first
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(BreakKind::Short)))
        );
        e.postpone().unwrap();
        assert!(
            !e.advance(1, now)
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_)))
        );
    }
    #[test]
    fn daily_focus_limit_pauses_the_local_timer_without_screen_surveillance() {
        let settings = Settings {
            daily_focus_limit_minutes: Some(30),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.work_seconds_today = 30 * 60 - 1;
        e.advance(1, active_now());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::DailyLimit
            }
        ));
    }
    /// The daily focus limit parks the phase in `Paused { DailyLimit }`, and a day
    /// rollover must release it: `activity_resumed`, `paused_until`, `start_session`,
    /// and `take_break_now` are each gated on a different reason or phase, so none of
    /// them can release this specific pause. `rollover_day` clearing
    /// `work_seconds_today` is the only path back to `Working`.
    #[test]
    fn daily_focus_limit_releases_on_the_next_local_day() {
        let now = active_now();
        let settings = Settings {
            daily_focus_limit_minutes: Some(30),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, now).unwrap();
        e.work_seconds_today = 30 * 60 - 1;
        e.advance(1, now);
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::DailyLimit
            }
        ));

        // Same day: the pause is still correct, because the allowance is spent.
        e.advance(60, now);
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::DailyLimit
            }
        ));

        let tomorrow = now + chrono::Duration::days(1);
        let events = e.advance(1, tomorrow);
        assert!(
            matches!(e.snapshot().phase, TimerPhase::Working),
            "a new day has a fresh allowance, so the timer must run again"
        );
        assert_eq!(e.work_seconds_today, 0);
        assert_eq!(e.snapshot().remaining_seconds, e.settings.work_seconds);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::StateChanged(TimerPhase::Working))),
            "the shell only learns about the release through an event"
        );
    }

    /// The same door, reached through a restart: quitting while the limit was spent
    /// wrote a checkpoint whose phase `from_checkpoint` did not reset, and by then
    /// `rollover_day` had already advanced `local_day`, so the handling in `advance`
    /// could no longer see a rollover.
    #[test]
    fn a_checkpoint_at_the_daily_limit_does_not_carry_the_pause_into_a_new_day() {
        let now = active_now();
        let settings = Settings {
            daily_focus_limit_minutes: Some(30),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings.clone(), now).unwrap();
        e.work_seconds_today = 30 * 60 - 1;
        e.advance(1, now);
        let checkpoint = e.checkpoint();

        let tomorrow = now + chrono::Duration::days(1);
        let (restored, _) =
            TimerEngine::restore(settings.clone(), checkpoint.clone(), tomorrow).unwrap();
        assert!(
            matches!(restored.snapshot().phase, TimerPhase::Working),
            "relaunching on a later day must not reinstate yesterday's limit"
        );

        // Relaunching on the same day keeps the pause, which is the correct behaviour.
        let (same_day, _) = TimerEngine::restore(settings, checkpoint, now).unwrap();
        assert!(matches!(
            same_day.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::DailyLimit
            }
        ));
    }

    /// Saving any setting recomputed all three nudge countdowns, so adjusting an
    /// unrelated field — or just sitting in Settings while autosave fired — restarted
    /// them. With a 10-minute blink interval, 10 minutes of adjusting meant no nudge.
    #[test]
    fn an_unrelated_settings_save_does_not_restart_the_nudge_countdowns() {
        let now = active_now();
        let settings = Settings {
            blink_nudge_minutes: Some(10),
            posture_nudge_minutes: Some(30),
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings.clone(), now).unwrap();
        e.advance(5 * 60, now);
        assert_eq!(e.blink_remaining, Some(5 * 60));

        // An unrelated change must leave the progress alone.
        let unrelated = Settings {
            accent: crate::settings::Accent::Sage,
            ..settings.clone()
        };
        e.replace_settings(unrelated, now).unwrap();
        assert_eq!(e.blink_remaining, Some(5 * 60));
        assert_eq!(e.posture_remaining, Some(25 * 60));

        // Editing the interval itself does re-arm, which is what a person expects.
        let retimed = Settings {
            blink_nudge_minutes: Some(20),
            ..settings.clone()
        };
        e.replace_settings(retimed, now).unwrap();
        assert_eq!(e.blink_remaining, Some(20 * 60));
        assert_eq!(
            e.posture_remaining,
            Some(25 * 60),
            "untouched, so unchanged"
        );

        // Turning one off clears only its own countdown.
        let blink_off = Settings {
            blink_nudge_minutes: None,
            ..settings
        };
        e.replace_settings(blink_off, now).unwrap();
        assert_eq!(e.blink_remaining, None);
        assert_eq!(e.posture_remaining, Some(25 * 60));
    }

    /// Shortening the work interval must take effect on the interval already in
    /// progress, not only on the next one. Lengthening it is deliberately left
    /// alone; see the comment at the call site in `replace_settings`.
    #[test]
    fn shortening_the_interval_mid_interval_clamps_the_countdown_down() {
        let now = active_now();
        let mut e = engine();
        e.advance(5 * 60, now); // 5 minutes into the default 20-minute interval

        let shorter = Settings {
            work_seconds: 8 * 60,
            ..e.settings.clone()
        };
        let events = e.replace_settings(shorter, now).unwrap();

        // 5 minutes already spent, 8-minute interval -> 3 minutes left, not the
        // roughly-15-minutes-left the unclamped countdown would have kept showing.
        assert_eq!(e.snapshot().remaining_seconds, 3 * 60);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Tick(180))),
            "the display must not wait for the next ordinary poll to catch up"
        );
    }

    #[test]
    fn shortening_below_elapsed_time_makes_the_break_due_almost_at_once() {
        let now = active_now();
        let mut e = engine();
        e.advance(10 * 60, now); // 10 minutes in

        let shorter = Settings {
            work_seconds: 5 * 60, // less than the 10 minutes already spent
            ..e.settings.clone()
        };
        e.replace_settings(shorter, now).unwrap();

        assert_eq!(e.snapshot().remaining_seconds, 0);
        let events = e.advance(1, now);
        assert!(events.iter().any(|event| matches!(
            event,
            EngineEvent::Due(_) | EngineEvent::StateChanged(TimerPhase::BreakDue { .. })
        )));
    }

    #[test]
    fn lengthening_the_interval_mid_interval_keeps_the_current_deadline() {
        let now = active_now();
        let mut e = engine();
        e.advance(5 * 60, now);
        let remaining_before = e.snapshot().remaining_seconds;

        let longer = Settings {
            work_seconds: 40 * 60,
            ..e.settings.clone()
        };
        let events = e.replace_settings(longer, now).unwrap();

        // The break already in progress keeps the deadline it promised; the
        // longer interval takes effect starting the next one, not this one.
        assert_eq!(e.snapshot().remaining_seconds, remaining_before);
        assert!(events.is_empty());
    }

    #[test]
    fn activity_resumes_system_pause_but_not_a_manual_pause() {
        let mut e = engine();
        e.report_idle(5 * 60).unwrap();
        e.activity_resumed().unwrap();
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        e.pause(PauseReason::Manual).unwrap();
        assert!(e.activity_resumed().unwrap().is_empty());
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Manual
            }
        ));
    }
    #[test]
    fn timed_pause_survives_as_state_and_resumes_without_consuming_work_time() {
        let mut e = engine();
        e.advance(10, active_now());
        let remaining = e.snapshot().remaining_seconds;
        e.pause_for(30).unwrap();
        assert!(e.snapshot().paused_until.is_some());
        e.paused_until = Some(Utc::now() - chrono::Duration::seconds(1));
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, remaining);
        assert!(e.snapshot().paused_until.is_none());
    }
    #[test]
    fn timed_pause_dismisses_a_due_break_until_the_pause_expires() {
        let mut e = engine();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));

        e.pause_for(30).unwrap();
        assert!(matches!(
            e.snapshot().phase,
            TimerPhase::Paused {
                reason: PauseReason::Manual
            }
        ));
        assert!(e.snapshot().paused_until.is_some());
        assert_eq!(e.due_grace_remaining, None);

        // While the pause runs the break stays out of the way entirely.
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Paused { .. }));

        // Once the chosen pause is over the break becomes due again, so the
        // shell's prompt/notification resurfaces instead of being lost.
        e.paused_until = Some(Utc::now() - chrono::Duration::seconds(1));
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        let events = e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_)))
        );
    }
    // The two exit paths from a `BreakDue`-origin timed pause diverge (see
    // `pause_for` and `resume` in engine.rs): expiry resurfaces the due break,
    // but an early manual `resume()` discards it entirely.
    #[test]
    fn early_resume_from_a_due_break_pause_discards_the_due_break() {
        let mut e = engine();
        e.remaining = 1;
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        assert_eq!(e.snapshot().remaining_seconds, 0);

        e.pause_for(30).unwrap();

        // Resuming before the 30 minutes elapse...
        let events = e.resume().unwrap();

        // ...lands on a full fresh work interval: the due break is gone.
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert_eq!(e.snapshot().remaining_seconds, e.settings.work_seconds);
        // No `Postponed` event is emitted and the daily postpone allowance is
        // untouched, unlike the `postpone()` path from the same due break.
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Postponed(_)))
        );
        assert_eq!(e.snapshot().postpones_today, 0);

        // Ticking afterwards does NOT re-enter `BreakDue` — contrast with the
        // expiry path in `timed_pause_dismisses_a_due_break_until_the_pause_expires`.
        let events = e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Working));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, EngineEvent::Due(_)))
        );
    }
    #[test]
    fn gentle_grace_restarts_after_a_timed_pause_from_a_due_break() {
        let settings = Settings {
            strictness: Strictness::Gentle,
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            ..Default::default()
        };
        let mut e = TimerEngine::new(settings, active_now()).unwrap();
        e.remaining = 1;
        e.advance(1, active_now());
        // Burn most of the gentle grace period, then pause from the due state.
        e.advance(GENTLE_DUE_GRACE_SECONDS - 1, active_now());
        assert_eq!(
            e.due_grace_remaining,
            Some(GENTLE_DUE_GRACE_SECONDS - (GENTLE_DUE_GRACE_SECONDS - 1))
        );
        e.pause_for(30).unwrap();
        assert_eq!(e.due_grace_remaining, None);

        e.paused_until = Some(Utc::now() - chrono::Duration::seconds(1));
        e.advance(1, active_now());
        e.advance(1, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        // The grace countdown restarts from the full budget rather than
        // auto-starting the break one second after the pause ended.
        e.advance(2, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::BreakDue { .. }));
        e.advance(GENTLE_DUE_GRACE_SECONDS, active_now());
        assert!(matches!(e.snapshot().phase, TimerPhase::Breaking { .. }));
    }
    #[test]
    fn checkpoint_restores_a_fresh_work_interval_and_resets_daily_counters() {
        let mut e = engine();
        e.advance(10, active_now());
        e.completed_short_breaks = 3;
        e.postpones_today = 2;
        let mut checkpoint = e.checkpoint();
        checkpoint.saved_at = Utc::now() - chrono::Duration::seconds(5);
        checkpoint.local_day = active_now().date_naive() - chrono::Duration::days(1);
        let (restored, _) =
            TimerEngine::restore(e.settings.clone(), checkpoint, active_now()).unwrap();
        // A relaunch never replays time elapsed while the app was closed — it always
        // resumes with a full, untouched work interval instead of landing on a break.
        assert_eq!(
            restored.snapshot().remaining_seconds,
            e.settings.work_seconds
        );
        assert!(matches!(restored.snapshot().phase, TimerPhase::Working));
        assert_eq!(restored.snapshot().completed_short_breaks, 0);
        assert_eq!(restored.snapshot().postpones_today, 0);
    }
    #[test]
    fn checkpoint_saved_while_a_break_was_due_restores_a_fresh_work_interval() {
        let mut e = engine();
        e.phase = TimerPhase::BreakDue {
            kind: BreakKind::Short,
        };
        e.remaining = 0;
        let checkpoint = e.checkpoint();
        let (restored, _) =
            TimerEngine::restore(e.settings.clone(), checkpoint, active_now()).unwrap();
        // Quitting mid-prompt must not resurrect a stale break-due state on the
        // next launch — the same symptom the fresh-interval reset exists to stop.
        assert!(matches!(restored.snapshot().phase, TimerPhase::Working));
        assert_eq!(
            restored.snapshot().remaining_seconds,
            e.settings.work_seconds
        );
    }
}
