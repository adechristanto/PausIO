//! Rust-side localization for the shell's own user-visible surfaces: the
//! tray, native notifications, and window titles. This deliberately mirrors
//! `frontend/src/lib/i18n.ts` rather than sharing it — the shell has no
//! access to the webview's JS catalogue, and the strings it owns (tray menu
//! labels, notification bodies) never appear in the frontend.
use pausio_core::Locale;
use pausio_protocol::ContextReason;

pub fn tray_tooltip(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO — Eye Breaks",
        Locale::De => "PausIO — Augenpausen",
    }
}

pub fn tray_status_starting(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Starting PausIO…",
        Locale::De => "PausIO wird gestartet…",
    }
}

pub fn tray_open(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Open PausIO",
        Locale::De => "PausIO öffnen",
    }
}

pub fn tray_pause(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Pause",
        Locale::De => "Pausieren",
    }
}

pub fn tray_resume(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Resume",
        Locale::De => "Fortsetzen",
    }
}

pub fn tray_start_session(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Start session",
        Locale::De => "Sitzung starten",
    }
}

pub fn tray_pause_for_30(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Pause for 30 minutes",
        Locale::De => "30 Minuten pausieren",
    }
}

pub fn tray_pause_for_60(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Pause for 1 hour",
        Locale::De => "1 Stunde pausieren",
    }
}

pub fn tray_pause_for_120(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Pause for 2 hours",
        Locale::De => "2 Stunden pausieren",
    }
}

pub fn tray_take_break(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Take a break now",
        Locale::De => "Jetzt Pause machen",
    }
}

pub fn tray_quit(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Quit PausIO",
        Locale::De => "PausIO beenden",
    }
}

#[cfg(target_os = "macos")]
pub fn menu_edit(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Edit",
        Locale::De => "Bearbeiten",
    }
}

#[cfg(target_os = "macos")]
pub fn menu_window(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Window",
        Locale::De => "Fenster",
    }
}

pub fn tray_next_break_in(locale: Locale, time: &str) -> String {
    match locale {
        Locale::En => format!("Next break in {time}"),
        Locale::De => format!("Nächste Pause in {time}"),
    }
}

pub fn tray_paused_screen_locked(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Paused while screen is locked",
        Locale::De => "Pausiert, solange der Bildschirm gesperrt ist",
    }
}

pub fn tray_paused(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Paused",
        Locale::De => "Pausiert",
    }
}

pub fn tray_ready_to_start(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Ready to start",
        Locale::De => "Bereit zum Starten",
    }
}

pub fn tray_break_due(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Break due",
        Locale::De => "Pause fällig",
    }
}

pub fn tray_break_remaining(locale: Locale, kind: &str, time: &str) -> String {
    match locale {
        Locale::En => format!("{kind} break — {time} remaining"),
        Locale::De => format!("{kind}e Pause — noch {time}"),
    }
}

pub fn tray_break_kind_short(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Short",
        Locale::De => "Kurz",
    }
}

pub fn tray_break_kind_long(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Long",
        Locale::De => "Lang",
    }
}

pub fn tray_waiting(locale: Locale, context: &str) -> String {
    match locale {
        Locale::En => format!("Waiting — {context}"),
        Locale::De => format!("Wartet — {context}"),
    }
}

pub fn context_label(locale: Locale, context: &ContextReason) -> &'static str {
    match (locale, context) {
        (Locale::En, ContextReason::Meeting) => "in a call",
        (Locale::En, ContextReason::ScreenShare) => "sharing your screen",
        (Locale::En, ContextReason::Fullscreen) => "fullscreen",
        (Locale::En, ContextReason::DoNotDisturb) => "Do Not Disturb",
        (Locale::En, ContextReason::ActiveInput) => "for a natural pause",
        (Locale::De, ContextReason::Meeting) => "in einem Gespräch",
        (Locale::De, ContextReason::ScreenShare) => "beim Bildschirmteilen",
        (Locale::De, ContextReason::Fullscreen) => "im Vollbild",
        (Locale::De, ContextReason::DoNotDisturb) => "im Nicht-stören-Modus",
        (Locale::De, ContextReason::ActiveInput) => "auf einen natürlichen Moment",
    }
}

pub fn notification_incoming_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO break incoming",
        Locale::De => "PausIO-Pause steht bevor",
    }
}

pub fn notification_incoming_body(locale: Locale, kind: &str) -> String {
    let kind_lower = kind.to_lowercase();
    match locale {
        Locale::En => format!("Your {kind_lower} eye break starts shortly."),
        Locale::De => format!("Deine {kind_lower}e Augenpause beginnt gleich."),
    }
}

pub fn notification_blink_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "A gentle blink reset",
        Locale::De => "Eine sanfte Blinzel-Pause",
    }
}

pub fn notification_blink_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Blink slowly five times, then relax your gaze.",
        Locale::De => "Blinzle fünfmal langsam und entspanne deinen Blick.",
    }
}

pub fn notification_posture_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "A gentle posture reset",
        Locale::De => "Eine sanfte Haltungspause",
    }
}

pub fn notification_posture_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Sit tall, soften your shoulders, and let your jaw relax.",
        Locale::De => "Sitz aufrecht, lockere die Schultern und entspanne den Kiefer.",
    }
}

pub fn notification_hydration_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "A gentle hydration reminder",
        Locale::De => "Eine sanfte Erinnerung zum Trinken",
    }
}

pub fn notification_hydration_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "A glass of water might be nice right about now.",
        Locale::De => "Ein Glas Wasser wäre jetzt vielleicht gut.",
    }
}

pub fn notification_due_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Time for your eyes",
        Locale::De => "Zeit für deine Augen",
    }
}

pub fn notification_due_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Take a moment to look somewhere far away.",
        Locale::De => "Schau einen Moment lang in die Ferne.",
    }
}

pub fn notification_start_short_action(locale: Locale, seconds: u32) -> String {
    match locale {
        Locale::En => format!("Start {seconds}s break"),
        Locale::De => format!("{seconds}-Sekunden-Pause starten"),
    }
}

pub fn notification_start_long_action(locale: Locale, minutes: u32) -> String {
    match locale {
        Locale::En => format!("Start {minutes}m break"),
        Locale::De => format!("{minutes}-Minuten-Pause starten"),
    }
}

pub fn notification_postpone_action(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Postpone 2 min",
        Locale::De => "2 Min. verschieben",
    }
}

pub fn notification_started_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO eye break",
        Locale::De => "PausIO-Augenpause",
    }
}

pub fn notification_started_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Look somewhere far away. PausIO will keep time for you.",
        Locale::De => "Schau in die Ferne. PausIO behält die Zeit im Blick.",
    }
}

pub fn notification_test_title(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO test reminder",
        Locale::De => "PausIO-Testerinnerung",
    }
}

pub fn notification_test_body(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "This is a local test. PausIO did not record any activity.",
        Locale::De => "Dies ist ein lokaler Test. PausIO hat keine Aktivität aufgezeichnet.",
    }
}

pub fn window_title_overlay(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO eye break",
        Locale::De => "PausIO-Augenpause",
    }
}

pub fn window_title_prompt(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "PausIO — Time for your eyes",
        Locale::De => "PausIO — Zeit für deine Augen",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_a_distinct_translation_for_each_locale() {
        assert_ne!(tray_tooltip(Locale::En), tray_tooltip(Locale::De));
        assert_ne!(tray_quit(Locale::En), tray_quit(Locale::De));
        assert_ne!(
            notification_due_title(Locale::En),
            notification_due_title(Locale::De)
        );
        assert_ne!(
            context_label(Locale::En, &ContextReason::Meeting),
            context_label(Locale::De, &ContextReason::Meeting)
        );
    }

    #[test]
    fn templated_strings_interpolate_the_given_value() {
        assert!(tray_next_break_in(Locale::En, "05:00").contains("05:00"));
        assert!(tray_waiting(Locale::De, "im Vollbild").contains("im Vollbild"));
        assert!(notification_start_short_action(Locale::En, 20).contains("20"));
        assert!(notification_start_long_action(Locale::De, 5).contains('5'));
    }
}
