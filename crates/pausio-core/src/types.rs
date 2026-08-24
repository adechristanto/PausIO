use chrono::{DateTime, NaiveDate, Utc};
use pausio_protocol::{BreakKind, ContextReason, TimerPhase};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::settings::SettingsError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub phase: TimerPhase,
    pub remaining_seconds: u32,
    pub completed_short_breaks: u32,
    pub postpones_today: u8,
    #[serde(default)]
    pub context: Option<ContextReason>,
    #[serde(default)]
    pub context_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub paused_until: Option<DateTime<Utc>>,
}

/// Versioned, durable timer state. Settings live separately so a failed
/// settings migration can never discard a recoverable in-progress session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    pub schema_version: u16,
    pub saved_at: DateTime<Utc>,
    pub local_day: NaiveDate,
    pub phase: TimerPhase,
    pub remaining_seconds: u32,
    pub completed_breaks: u32,
    pub completed_short_breaks: u32,
    pub postpones_today: u8,
    pub manual_session: bool,
    #[serde(default)]
    pub context: Option<ContextReason>,
    #[serde(default)]
    pub paused_until: Option<DateTime<Utc>>,
    /// Automatic active-input deferrals are deliberately bounded so a person
    /// is never silently postponed for an entire workday.
    #[serde(default)]
    pub automatic_deferrals_today: u8,
    #[serde(default)]
    pub fixed_breaks_seen_today: Vec<u16>,
    #[serde(default)]
    pub work_seconds_today: u32,
}

pub const SESSION_SCHEMA_VERSION: u16 = 1;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    StateChanged(TimerPhase),
    Incoming(BreakKind),
    Due(BreakKind),
    Started(BreakKind),
    /// A break ran its full course, or a break's own duration elapsed while
    /// the person was away (idle, asleep, or the screen was locked).
    Ended(BreakKind),
    /// A break was ended early by an explicit person action (the overlay's
    /// "I'm back" / emergency-exit control). Distinguishing this from
    /// `Ended` keeps compliance history and streaks honest: cadence
    /// counters still advance the same way for both, since a break was
    /// still taken, but history and analytics must not treat them alike.
    Skipped(BreakKind),
    Postponed(BreakKind),
    ContextDeferred {
        kind: BreakKind,
        reason: ContextReason,
    },
    BlinkNudge,
    PostureNudge,
    HydrationNudge,
    Tick(u32),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EngineError {
    #[error("invalid transition")]
    InvalidTransition,
    #[error(transparent)]
    Settings(#[from] SettingsError),
}
