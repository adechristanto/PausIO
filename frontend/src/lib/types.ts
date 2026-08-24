export type BreakKind = 'short' | 'long'
export type PauseReason =
  'manual' | 'idle' | 'sleep' | 'screen_lock' | 'meeting' | 'dnd' | 'outside_hours' | 'daily_limit'
export type Strictness = 'gentle' | 'balanced' | 'firm' | 'strict'
export type Theme = 'light' | 'dark' | 'system'
export type Accent = 'horizon' | 'sage' | 'amber' | 'lilac'
export type Locale = 'en' | 'de'
export type DisplayTarget = 'all' | 'active' | 'primary' | 'notification_only'
export type BreakRoutine = 'guided' | 'quiet' | 'far_gaze' | 'blink' | 'posture'
export type SoundTheme = 'silence' | 'chime' | 'tone' | 'click'
export type SystemSound = 'default' | 'chime' | 'ding' | 'alert' | 'complete'
export type ContextReason =
  'meeting' | 'screen_share' | 'fullscreen' | 'do_not_disturb' | 'active_input'
export type TimerPhase =
  | 'dormant'
  | 'working'
  | 'pre_break'
  | { break_due: { kind: BreakKind } }
  | { breaking: { kind: BreakKind } }
  | { paused: { reason: PauseReason } }
export interface Settings {
  work_seconds: number
  short_break_seconds: number
  long_break_seconds: number
  long_break_every: number | null
  pre_break_seconds: number
  active_days_mask: number
  active_start_minutes: number
  active_end_minutes: number
  postpone_limit: number | null
  /** Optional during the rolling upgrade from M1 clients. */ strictness?: Strictness
  theme?: Theme
  accent?: Accent
  locale?: Locale
  break_messages?: string[]
  blink_nudge_minutes?: number | null
  posture_nudge_minutes?: number | null
  display_target?: DisplayTarget
  break_routine?: BreakRoutine
  history_enabled?: boolean
  history_retention_days?: number | null
  notification_sound?: boolean
  notification_sound_name?: SystemSound
  fixed_break_minutes?: number[]
  daily_focus_limit_minutes?: number | null
  end_break_shortcut?: string | null
  pause_toggle_shortcut?: string | null
  take_break_shortcut?: string | null
  sound_theme?: SoundTheme
  sound_volume?: number
  auto_detect_fullscreen?: boolean
  auto_detect_do_not_disturb?: boolean
  hydration_nudge_minutes?: number | null
  show_clock_in_break?: boolean
}
export interface Snapshot {
  phase: TimerPhase
  remaining_seconds: number
  completed_short_breaks: number
  postpones_today: number
  /** Optional during the rolling upgrade from M1 clients. */ context?: ContextReason | null
  context_expires_at?: string | null
  paused_until?: string | null
}
export type SettingsErrorField =
  | 'work_duration'
  | 'short_break'
  | 'long_break'
  | 'cadence'
  | 'warning'
  | 'working_hours'
  | 'break_messages'
  | 'blink_nudge'
  | 'posture_nudge'
  | 'hydration_nudge'
  | 'history_retention'
  | 'fixed_breaks'
  | 'daily_focus_limit'
  | 'global_shortcut'
  | 'sound_volume'
export interface ApiError {
  code:
    | 'invalid_settings'
    | 'invalid_transition'
    | 'platform_unavailable'
    | 'permission_denied'
    | 'internal'
  message: string
  field?: SettingsErrorField
}
export interface WatchSettingsEnvelopeV1 {
  schema_version: 1
  revision: number
  timezone: string
  work_interval_seconds: number
  short_break_seconds: number
  long_break_seconds: number
  pre_break_seconds: number
  active_days_mask: number
  active_start_minutes: number
  active_end_minutes: number
  paused: boolean
  updated_at: string
  next_break_at?: string | null
  /** Additive v1 fields. Legacy watches continue using `next_break_at`. */
  phase?: 'dormant' | 'working' | 'pre_break' | 'break_due' | 'breaking' | 'paused' | null
  phase_deadline_at?: string | null
  break_active?: boolean
  break_kind?: BreakKind | null
}

/** A bridge result is delivery state, never evidence of a physical haptic. */
export type NudgeResult = 'delivered' | 'queued' | 'unavailable'

export interface WatchStatus {
  platform: string
  available: boolean
  paired: boolean
  app_installed: boolean
  reachable: boolean
  last_synced_revision: number | null
  last_error: string | null
  last_queued_revision?: number | null
  notification_permission?: 'granted' | 'denied' | 'not_determined' | 'unknown' | null
  reminder_precision?: 'exact' | 'inexact' | 'not_available' | null
  schedule_horizon_at?: string | null
  last_successful_sync_at?: string | null
  app_version?: string | null
  connection_state?:
    | 'unavailable'
    | 'unpaired'
    | 'app_not_installed'
    | 'activating'
    | 'disconnected'
    | 'connected'
    | 'degraded'
    | null
  capabilities?: {
    timer_display: boolean
    local_reminders: boolean
    test_haptic: boolean
    remote_actions: boolean
    standalone?: boolean
    complication?: boolean
  }
}

export interface AutostartStatus {
  supported: boolean
  enabled: boolean
}
export interface SettingsProfiles {
  work?: Settings
  home?: Settings
}

export type HistoryEventKind =
  'due' | 'started' | 'completed' | 'skipped' | 'postponed' | 'deferred'
export interface HistoryEvent {
  schema_version?: number
  break_id?: string
  occurred_at: string
  kind: HistoryEventKind
  break_kind?: BreakKind
  context?: ContextReason
  target_break_seconds?: number
  work_interval_seconds?: number
  schedule_fingerprint?: string
}
export interface DesktopHealth {
  platform: string
  session_type: string
  notification_permission: string
  display_count: number
  tray_available: boolean
  autostart_supported: boolean
  autostart_enabled: boolean
  automatic_idle_lock_available: boolean
  strict_overlay_hardening: boolean
  strict_overlay_guaranteed: boolean
  history_enabled: boolean
  history_retention_days: number | null
  display_target: DisplayTarget
  auto_context_supported: boolean
}
