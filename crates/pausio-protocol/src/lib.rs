//! The intentionally small, permissively licensed contract shared by PausIO clients.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u16 = 1;
pub const WATCH_RUNTIME_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakKind {
    Short,
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    Manual,
    Idle,
    Sleep,
    ScreenLock,
    Meeting,
    Dnd,
    OutsideHours,
    DailyLimit,
}

/// A transient, privacy-preserving reason why PausIO is withholding an
/// interruption. These values describe device state only; they never contain
/// an application name, window title, media title, or captured content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextReason {
    Meeting,
    ScreenShare,
    Fullscreen,
    DoNotDisturb,
    ActiveInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerPhase {
    Dormant,
    Working,
    PreBreak,
    BreakDue { kind: BreakKind },
    Breaking { kind: BreakKind },
    Paused { reason: PauseReason },
}

/// A flat, watch-friendly projection of the richer timer phase. Watches use
/// this only to render a deadline and choose their bounded local schedule;
/// timer decisions stay in the phone/desktop engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchPhase {
    Dormant,
    Working,
    PreBreak,
    BreakDue,
    Breaking,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchSettingsEnvelopeV1 {
    pub schema_version: u16,
    pub revision: u64,
    pub timezone: String,
    pub work_interval_seconds: u32,
    pub short_break_seconds: u32,
    pub long_break_seconds: u32,
    pub pre_break_seconds: u32,
    pub active_days_mask: u8,
    pub active_start_minutes: u16,
    pub active_end_minutes: u16,
    pub paused: bool,
    pub updated_at: DateTime<Utc>,
    /// The phone-authoritative deadline for the current working interval.
    /// Optional so v1 readers and already-paired watches remain compatible.
    #[serde(default)]
    pub next_break_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub break_active: bool,
    #[serde(default)]
    pub break_kind: Option<BreakKind>,
    /// Additive v1 fields. Older watches infer these from the legacy fields;
    /// newer watches use them to avoid treating a break end as a new break.
    #[serde(default)]
    pub phase: Option<WatchPhase>,
    #[serde(default)]
    pub phase_deadline_at: Option<DateTime<Utc>>,
}

impl WatchSettingsEnvelopeV1 {
    pub fn new(revision: u64, updated_at: DateTime<Utc>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            revision,
            timezone: "UTC".into(),
            work_interval_seconds: 1200,
            short_break_seconds: 20,
            long_break_seconds: 300,
            pre_break_seconds: 30,
            active_days_mask: 0b0011_1110,
            active_start_minutes: 540,
            active_end_minutes: 1080,
            paused: false,
            updated_at,
            next_break_at: None,
            break_active: false,
            break_kind: None,
            phase: None,
            phase_deadline_at: None,
        }
    }

    /// Keep validation in the permissively licensed protocol so every native
    /// bridge can reject a malformed context before it changes a local watch
    /// schedule. Time-zone identifiers are resolved by each platform because
    /// Foundation and java.time own the authoritative IANA databases.
    pub fn is_valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && !self.timezone.trim().is_empty()
            && (300..=7_200).contains(&self.work_interval_seconds)
            && (5..=120).contains(&self.short_break_seconds)
            && (5..=3_600).contains(&self.long_break_seconds)
            && [0, 10, 30, 60].contains(&self.pre_break_seconds)
            && self.active_days_mask != 0
            && self.active_start_minutes < 1_440
            && self.active_end_minutes < 1_440
    }
}

/// A watch-originated runtime control. Configuration remains phone-owned;
/// this command only lets a watch mirror a local pause or break immediately
/// while its paired phone is reachable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchRuntimeActionV1 {
    pub schema_version: u16,
    pub action_id: String,
    pub action: WatchRuntimeAction,
    pub base_revision: u64,
    pub occurred_at: DateTime<Utc>,
}

impl WatchRuntimeActionV1 {
    pub fn is_valid(&self) -> bool {
        self.schema_version == WATCH_RUNTIME_SCHEMA_VERSION
            && !self.action_id.trim().is_empty()
            && self.action_id.len() <= 128
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchRuntimeAction {
    Pause,
    Resume,
    TakeBreakNow,
    SkipBreak,
}

/// A durable acknowledgement for settings and health reports. The transport
/// may deliver the same receipt more than once; consumers compare IDs and
/// revisions rather than treating it as a new action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchReceiptV1 {
    pub schema_version: u16,
    pub kind: WatchReceiptKind,
    pub result: WatchReceiptResult,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub revision: Option<u64>,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchReceiptKind {
    Settings,
    RuntimeAction,
    TestNudge,
    Health,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchReceiptResult {
    Applied,
    Delivered,
    Stale,
    Invalid,
    Unavailable,
    StorageFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchHealthV1 {
    pub schema_version: u16,
    pub app_version: String,
    pub notification_permission: WatchPermissionState,
    pub reminder_precision: WatchReminderPrecision,
    #[serde(default)]
    pub schedule_horizon_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_successful_sync_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchPermissionState {
    Unknown,
    NotDetermined,
    Granted,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchReminderPrecision {
    Exact,
    Inexact,
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NudgeResult {
    Delivered,
    Queued,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchStatus {
    pub platform: String,
    pub available: bool,
    pub paired: bool,
    pub app_installed: bool,
    pub reachable: bool,
    pub last_synced_revision: Option<u64>,
    pub last_error: Option<String>,
    /// Latest revision handed to the platform transport. This is intentionally
    /// separate from `last_synced_revision`, which means watch-side receipt.
    #[serde(default)]
    pub last_queued_revision: Option<u64>,
    #[serde(default)]
    pub connection_state: Option<WatchConnectionState>,
    #[serde(default)]
    pub capabilities: WatchCapabilities,
    #[serde(default)]
    pub notification_permission: Option<WatchPermissionState>,
    #[serde(default)]
    pub reminder_precision: Option<WatchReminderPrecision>,
    #[serde(default)]
    pub schedule_horizon_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_successful_sync_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchCapabilities {
    #[serde(default)]
    pub timer_display: bool,
    #[serde(default)]
    pub local_reminders: bool,
    #[serde(default)]
    pub test_haptic: bool,
    #[serde(default)]
    pub remote_actions: bool,
    #[serde(default)]
    pub standalone: bool,
    #[serde(default)]
    pub complication: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchConnectionState {
    Unavailable,
    Unpaired,
    AppNotInstalled,
    Activating,
    Disconnected,
    Connected,
    Degraded,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_contract_round_trips() {
        let value = WatchSettingsEnvelopeV1::new(3, Utc::now());
        let json = serde_json::to_string(&value).unwrap();
        let decoded: WatchSettingsEnvelopeV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.schema_version, SCHEMA_VERSION);
        assert_eq!(decoded.revision, 3);
    }
    #[test]
    fn fixture_tolerates_future_fields_and_preserves_contract_fields() {
        let fixture = include_str!("../../../tests/fixtures/watch-settings-v1.json");
        let decoded: WatchSettingsEnvelopeV1 = serde_json::from_str(fixture).unwrap();
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.revision, 7);
        let encoded = serde_json::to_value(decoded).unwrap();
        assert_eq!(encoded["timezone"], "Europe/Berlin");
        assert_eq!(encoded["next_break_at"], "2026-07-23T08:20:00Z");
        assert!(encoded.get("future_field").is_none());
    }
    #[test]
    fn malformed_contract_is_rejected() {
        assert!(serde_json::from_str::<WatchSettingsEnvelopeV1>("{\"revision\": 1}").is_err());
    }

    #[test]
    fn additive_phase_fields_remain_optional_for_v1_readers() {
        let mut value = WatchSettingsEnvelopeV1::new(8, Utc::now());
        value.phase = Some(WatchPhase::Breaking);
        value.phase_deadline_at = Some(Utc::now());
        let encoded = serde_json::to_string(&value).unwrap();
        let decoded: WatchSettingsEnvelopeV1 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.phase, Some(WatchPhase::Breaking));
        assert!(decoded.phase_deadline_at.is_some());
    }

    #[test]
    fn watch_settings_validation_matches_the_timer_contract() {
        let mut value = WatchSettingsEnvelopeV1::new(1, Utc::now());
        assert!(value.is_valid());
        value.active_days_mask = 0;
        assert!(!value.is_valid());
    }

    #[test]
    fn runtime_actions_require_an_id_and_current_schema() {
        let action = WatchRuntimeActionV1 {
            schema_version: WATCH_RUNTIME_SCHEMA_VERSION,
            action_id: "action-1".into(),
            action: WatchRuntimeAction::Pause,
            base_revision: 3,
            occurred_at: Utc::now(),
        };
        assert!(action.is_valid());
        assert!(serde_json::from_str::<WatchRuntimeActionV1>(
            r#"{"schema_version":1,"action_id":"a","action":"resume","base_revision":2,"occurred_at":"2026-08-08T10:00:00Z"}"#
        )
        .unwrap()
        .is_valid());
    }
}
