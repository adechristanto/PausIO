import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  AutostartStatus,
  BreakKind,
  ContextReason,
  DesktopHealth,
  HistoryEvent,
  NudgeResult,
  Settings,
  SettingsProfiles,
  Snapshot,
  SystemSound,
  WatchSettingsEnvelopeV1,
  WatchStatus,
} from './types'

export const api = {
  getState: () => invoke<Snapshot>('get_state'),
  getSettings: () => invoke<Settings>('get_settings'),
  getSettingsProfiles: () => invoke<SettingsProfiles>('get_settings_profiles'),
  getOnboardingState: () => invoke<boolean>('get_onboarding_state'),
  completeOnboarding: () => invoke<void>('complete_onboarding'),
  saveSettingsProfile: (name: 'work' | 'home') =>
    invoke<SettingsProfiles>('save_settings_profile', { name }),
  applySettingsProfile: (name: 'work' | 'home') =>
    invoke<Settings>('apply_settings_profile', { name }),
  setSettings: (settings: Settings) => invoke<Settings>('set_settings', { settings }),
  setContext: (context: ContextReason | null, durationMinutes?: number) =>
    invoke<Snapshot>('set_context', { context, durationMinutes }),
  getHistory: () => invoke<HistoryEvent[]>('get_history'),
  clearHistory: () => invoke<void>('clear_history'),
  exportHistory: (format: 'json' | 'csv') => invoke<string>('export_history', { format }),
  resetLocalData: () => invoke<Snapshot>('reset_local_data'),
  startSession: () => invoke<Snapshot>('start_session'),
  startDueBreak: () => invoke<Snapshot>('start_due_break'),
  pause: () => invoke<Snapshot>('pause'),
  pauseForMinutes: (minutes: number) => invoke<Snapshot>('pause_for_minutes', { minutes }),
  resume: () => invoke<Snapshot>('resume'),
  takeBreakNow: () => invoke<Snapshot>('take_break_now'),
  skipBreak: () => invoke<Snapshot>('skip_break'),
  postponeBreak: () => invoke<Snapshot>('postpone_break'),
  syncWatchSettings: () => invoke<WatchSettingsEnvelopeV1>('sync_watch_settings'),
  sendTestNudge: () => invoke<NudgeResult>('send_test_nudge'),
  getWatchStatus: () => invoke<WatchStatus>('get_watch_status'),
  getAutostartStatus: () => invoke<AutostartStatus>('get_autostart_status'),
  setAutostartEnabled: (enabled: boolean) =>
    invoke<AutostartStatus>('set_autostart_enabled', { enabled }),
  getDesktopHealth: () => invoke<DesktopHealth>('get_desktop_health'),
  getHealthReport: () => invoke<string>('get_health_report'),
  testReminder: () => invoke<void>('test_reminder'),
  previewSystemSound: (sound: SystemSound) => invoke<void>('preview_system_sound', { sound }),
  e2eSimulateScreenLock: (lockedSeconds: number) =>
    invoke<Snapshot>('e2e_simulate_screen_lock', { lockedSeconds }),
  onTick: (handler: (remaining: number) => void) =>
    listen<number>('timer:tick', (event) => handler(event.payload)),
  onState: (handler: (state: Snapshot) => void) =>
    listen<Snapshot>('state:changed', (event) => handler(event.payload)),
  onBreakEnded: (handler: (kind: BreakKind) => void) =>
    listen<BreakKind>('break:ended', (event) => handler(event.payload)),
  onBlinkNudge: (handler: () => void) => listen('nudge:blink', () => handler()),
  onPostureNudge: (handler: () => void) => listen('nudge:posture', () => handler()),
  onHydrationNudge: (handler: () => void) => listen('nudge:hydration', () => handler()),
}
