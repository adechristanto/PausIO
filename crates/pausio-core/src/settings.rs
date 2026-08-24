use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[allow(dead_code)]
const fn default_history_enabled() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub work_seconds: u32,
    pub short_break_seconds: u32,
    pub long_break_seconds: u32,
    pub long_break_every: Option<u8>,
    pub pre_break_seconds: u32,
    pub active_days_mask: u8,
    pub active_start_minutes: u16,
    pub active_end_minutes: u16,
    pub postpone_limit: Option<u8>,
    #[serde(default = "default_blink_nudge_minutes")]
    pub blink_nudge_minutes: Option<u8>,
    #[serde(default)]
    pub posture_nudge_minutes: Option<u8>,
    #[serde(default)]
    pub hydration_nudge_minutes: Option<u8>,
    /// Purely presentational: shows the current local time on the break
    /// shield. Read only by the frontend; the engine never branches on it.
    #[serde(default)]
    pub show_clock_in_break: bool,
    #[serde(default)]
    pub strictness: Strictness,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub accent: Accent,
    #[serde(default)]
    pub locale: Locale,
    /// Short, locally stored messages shown during a desktop break. They are
    /// never part of the watch bridge or any network payload.
    #[serde(default)]
    pub break_messages: Vec<String>,
    /// Which display surface carries an interrupting break. This is stored in
    /// the shared settings contract so profiles behave identically on every
    /// desktop platform, even though monitor discovery is native.
    #[serde(default)]
    pub display_target: DisplayTarget,
    /// A declarative, local-only break routine. PausIO never executes user
    /// supplied code or remote content in an overlay.
    #[serde(default)]
    pub break_routine: BreakRoutine,
    /// Off for new installations, matching the README's privacy promise. Existing
    /// serialized choices, including an explicit `true`, remain unchanged.
    #[serde(default = "default_history_enabled")]
    pub history_enabled: bool,
    /// `None` means unlimited only when history is enabled. Valid finite
    /// retention periods are intentionally small and predictable.
    #[serde(default)]
    pub history_retention_days: Option<u16>,
    /// Opt-in OS notification sound. Visual/text notifications remain
    /// available even when sound is disabled.
    #[serde(default)]
    pub notification_sound: bool,
    /// Which system sound to use when `notification_sound` is enabled —
    /// covers both the OS notification chime and the short-break-end cue.
    #[serde(default)]
    pub notification_sound_name: SystemSound,
    /// A short, synthesized audio cue at the start and end of a break,
    /// played by the webview — independent of `notification_sound`, which
    /// only covers the OS notification chime.
    #[serde(default)]
    pub sound_theme: SoundTheme,
    /// Percent volume (0-100) for `sound_theme`.
    #[serde(default = "default_sound_volume")]
    pub sound_volume: u8,
    /// Local clock minutes at which a short break becomes due. They are
    /// intentionally simple fixed-time reminders, not calendar data.
    #[serde(default)]
    pub fixed_break_minutes: Vec<u16>,
    /// A gentle cap on PausIO-managed focus time for the local day. It counts
    /// only the timer's active work state; it is not screen-time surveillance.
    #[serde(default)]
    pub daily_focus_limit_minutes: Option<u16>,
    /// Global (system-wide) keyboard accelerators, in Tauri's accelerator
    /// syntax (e.g. `"CmdOrCtrl+X"`). `None` disables the shortcut. These are
    /// the only controls that work while a full-screen break shield has
    /// keyboard focus, and the only ones that work without a pointer at all.
    #[serde(default = "default_end_break_shortcut")]
    pub end_break_shortcut: Option<String>,
    #[serde(default)]
    pub pause_toggle_shortcut: Option<String>,
    #[serde(default)]
    pub take_break_shortcut: Option<String>,
    /// Opt-in, per-signal automatic context detection. Each reads only an
    /// aggregate OS state (never application names, window titles, mic, or
    /// camera) and defers a due break exactly the way a person's own tray
    /// selection would, clearing the instant the OS signal clears. Support
    /// varies by platform; an unsupported platform simply never sets the
    /// signal, which is reported honestly in the desktop health report.
    #[serde(default = "default_auto_detect")]
    pub auto_detect_fullscreen: bool,
    #[serde(default = "default_auto_detect")]
    pub auto_detect_do_not_disturb: bool,
}

const fn default_blink_nudge_minutes() -> Option<u8> {
    None
}

fn default_end_break_shortcut() -> Option<String> {
    Some("CmdOrCtrl+Shift+P".to_string())
}

const fn default_sound_volume() -> u8 {
    70
}

/// On by default: each signal reads only an aggregate OS state, so there is no
/// privacy cost, and a break that fires mid-presentation is the most common
/// reason people abandon this kind of app.
const fn default_auto_detect() -> bool {
    true
}

/// Reminder delivery is deliberately independent from timing. A person can
/// choose a gentler presentation without losing correct natural-break and
/// schedule accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Strictness {
    Gentle,
    #[default]
    Balanced,
    Firm,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    #[default]
    Horizon,
    Sage,
    Amber,
    Lilac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    #[default]
    En,
    De,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayTarget {
    #[default]
    All,
    Active,
    Primary,
    NotificationOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakRoutine {
    #[default]
    Guided,
    Quiet,
    FarGaze,
    Blink,
    Posture,
}

/// A break start/end audio cue. These are synthesized on the fly in the
/// webview (short oscillator tones), never bundled audio files, so there is
/// nothing to license, ship, or fail to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Silence,
    #[default]
    Chime,
    Tone,
    Click,
}

/// A native OS system sound, resolved to a platform-specific resource name
/// or alias by the desktop shell. PausIO ships no bundled audio for this —
/// every option maps to a sound the operating system already owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SystemSound {
    #[default]
    Default,
    Chime,
    Ding,
    Alert,
    Complete,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            work_seconds: 20 * 60,
            short_break_seconds: 20,
            long_break_seconds: 5 * 60,
            long_break_every: None,
            pre_break_seconds: 30,
            active_days_mask: 0b0011_1110,
            active_start_minutes: 9 * 60,
            active_end_minutes: 18 * 60,
            postpone_limit: Some(3),
            blink_nudge_minutes: default_blink_nudge_minutes(),
            posture_nudge_minutes: None,
            hydration_nudge_minutes: None,
            show_clock_in_break: false,
            strictness: Strictness::Balanced,
            theme: Theme::System,
            accent: Accent::Horizon,
            locale: Locale::En,
            break_messages: vec![],
            display_target: DisplayTarget::All,
            break_routine: BreakRoutine::Guided,
            history_enabled: default_history_enabled(),
            history_retention_days: Some(365),
            notification_sound: false,
            notification_sound_name: SystemSound::Default,
            fixed_break_minutes: vec![],
            daily_focus_limit_minutes: None,
            end_break_shortcut: default_end_break_shortcut(),
            pause_toggle_shortcut: None,
            take_break_shortcut: None,
            sound_theme: SoundTheme::default(),
            sound_volume: default_sound_volume(),
            auto_detect_fullscreen: default_auto_detect(),
            auto_detect_do_not_disturb: default_auto_detect(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SettingsError {
    #[error("work duration must be between 5 and 120 minutes")]
    WorkDuration,
    #[error("short break must be between 5 and 120 seconds")]
    ShortBreak,
    #[error("long break must be between 1 and 30 minutes")]
    LongBreak,
    #[error("long break cadence must be 2 through 8 or disabled")]
    Cadence,
    #[error("pre-break warning must be 0, 10, 30, or 60 seconds")]
    Warning,
    #[error("working hours are invalid")]
    WorkingHours,
    #[error("break messages must contain at most 12 non-empty messages of 120 characters or fewer")]
    BreakMessages,
    #[error("blink nudge must be every 5 through 60 minutes or disabled")]
    BlinkNudge,
    #[error("posture nudge must be every 15 through 120 minutes or disabled")]
    PostureNudge,
    #[error("hydration nudge must be every 15 through 120 minutes or disabled")]
    HydrationNudge,
    #[error("history retention must be 30, 90, or 365 days when set")]
    HistoryRetention,
    #[error("fixed break times must be unique local minutes within a day")]
    FixedBreaks,
    #[error("daily focus limit must be 30 through 1440 minutes or disabled")]
    DailyFocusLimit,
    #[error(
        "a global shortcut must be a non-empty accelerator of 40 characters or fewer, or disabled"
    )]
    GlobalShortcut,
    #[error("sound volume must be between 0 and 100")]
    SoundVolume,
}

impl SettingsError {
    /// A stable, locale-independent identifier for which field failed validation. UI clients
    /// use this — never `to_string()`'s English message — to show a translated error, so a
    /// German user editing settings never sees this crate's English validation text.
    pub fn field(&self) -> &'static str {
        match self {
            SettingsError::WorkDuration => "work_duration",
            SettingsError::ShortBreak => "short_break",
            SettingsError::LongBreak => "long_break",
            SettingsError::Cadence => "cadence",
            SettingsError::Warning => "warning",
            SettingsError::WorkingHours => "working_hours",
            SettingsError::BreakMessages => "break_messages",
            SettingsError::BlinkNudge => "blink_nudge",
            SettingsError::PostureNudge => "posture_nudge",
            SettingsError::HydrationNudge => "hydration_nudge",
            SettingsError::HistoryRetention => "history_retention",
            SettingsError::FixedBreaks => "fixed_breaks",
            SettingsError::DailyFocusLimit => "daily_focus_limit",
            SettingsError::GlobalShortcut => "global_shortcut",
            SettingsError::SoundVolume => "sound_volume",
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if !(300..=7200).contains(&self.work_seconds) {
            return Err(SettingsError::WorkDuration);
        }
        if !(5..=120).contains(&self.short_break_seconds) {
            return Err(SettingsError::ShortBreak);
        }
        if !(60..=1800).contains(&self.long_break_seconds) {
            return Err(SettingsError::LongBreak);
        }
        if self.long_break_every.is_some_and(|v| !(2..=8).contains(&v)) {
            return Err(SettingsError::Cadence);
        }
        if ![0, 10, 30, 60].contains(&self.pre_break_seconds) {
            return Err(SettingsError::Warning);
        }
        if self
            .blink_nudge_minutes
            .is_some_and(|minutes| !(5..=60).contains(&minutes))
        {
            return Err(SettingsError::BlinkNudge);
        }
        if self
            .posture_nudge_minutes
            .is_some_and(|minutes| !(15..=120).contains(&minutes))
        {
            return Err(SettingsError::PostureNudge);
        }
        if self
            .hydration_nudge_minutes
            .is_some_and(|minutes| !(15..=120).contains(&minutes))
        {
            return Err(SettingsError::HydrationNudge);
        }
        if self.active_start_minutes >= 1440
            || self.active_end_minutes >= 1440
            || self.active_days_mask == 0
        {
            return Err(SettingsError::WorkingHours);
        }
        if self.break_messages.len() > 12
            || self.break_messages.iter().any(|message| {
                let trimmed = message.trim();
                trimmed.is_empty() || trimmed.chars().count() > 120
            })
        {
            return Err(SettingsError::BreakMessages);
        }
        if self
            .history_retention_days
            .is_some_and(|days| ![30, 90, 365].contains(&days))
        {
            return Err(SettingsError::HistoryRetention);
        }
        if self.fixed_break_minutes.len() > 12
            || self
                .fixed_break_minutes
                .iter()
                .any(|minute| *minute >= 1440)
            || {
                let mut sorted = self.fixed_break_minutes.clone();
                sorted.sort_unstable();
                sorted.windows(2).any(|pair| pair[0] == pair[1])
            }
        {
            return Err(SettingsError::FixedBreaks);
        }
        if self
            .daily_focus_limit_minutes
            .is_some_and(|minutes| !(30..=1440).contains(&minutes))
        {
            return Err(SettingsError::DailyFocusLimit);
        }
        for shortcut in [
            &self.end_break_shortcut,
            &self.pause_toggle_shortcut,
            &self.take_break_shortcut,
        ] {
            if shortcut
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.chars().count() > 40)
            {
                return Err(SettingsError::GlobalShortcut);
            }
        }
        if self.sound_volume > 100 {
            return Err(SettingsError::SoundVolume);
        }
        Ok(())
    }
    pub fn active_at(&self, now: DateTime<Local>) -> bool {
        let day_bit = 1u8 << now.weekday().num_days_from_sunday();
        if self.active_days_mask & day_bit == 0 {
            return false;
        }
        let minute = (now.hour() * 60 + now.minute()) as u16;
        if self.active_start_minutes == self.active_end_minutes {
            return true;
        }
        if self.active_start_minutes < self.active_end_minutes {
            (self.active_start_minutes..self.active_end_minutes).contains(&minute)
        } else {
            minute >= self.active_start_minutes || minute < self.active_end_minutes
        }
    }
}
