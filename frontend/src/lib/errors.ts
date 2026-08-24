import { t } from './i18n'
import type { ApiError, SettingsErrorField } from './types'

const settingsErrorKeys: Record<
  SettingsErrorField,
  | 'settings_error_work_duration'
  | 'settings_error_short_break'
  | 'settings_error_long_break'
  | 'settings_error_cadence'
  | 'settings_error_warning'
  | 'settings_error_working_hours'
  | 'settings_error_break_messages'
  | 'settings_error_blink_nudge'
  | 'settings_error_posture_nudge'
  | 'settings_error_hydration_nudge'
  | 'settings_error_history_retention'
  | 'settings_error_fixed_breaks'
  | 'settings_error_daily_focus_limit'
  | 'settings_error_global_shortcut'
  | 'settings_error_sound_volume'
> = {
  work_duration: 'settings_error_work_duration',
  short_break: 'settings_error_short_break',
  long_break: 'settings_error_long_break',
  cadence: 'settings_error_cadence',
  warning: 'settings_error_warning',
  working_hours: 'settings_error_working_hours',
  break_messages: 'settings_error_break_messages',
  blink_nudge: 'settings_error_blink_nudge',
  posture_nudge: 'settings_error_posture_nudge',
  hydration_nudge: 'settings_error_hydration_nudge',
  history_retention: 'settings_error_history_retention',
  fixed_breaks: 'settings_error_fixed_breaks',
  daily_focus_limit: 'settings_error_daily_focus_limit',
  global_shortcut: 'settings_error_global_shortcut',
  sound_volume: 'settings_error_sound_volume',
}

export function errorMessage(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'field' in error &&
    typeof (error as Partial<ApiError>).field === 'string'
  ) {
    // A structured settings-validation error: always resolve the field to a translated
    // message. The engine's own `message` is English-only prose meant for logs, not users.
    const field = (error as ApiError).field as SettingsErrorField
    if (field in settingsErrorKeys) return t(settingsErrorKeys[field])
  }
  if (error instanceof Error) return error.message
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof (error as Partial<ApiError>).message === 'string'
  ) {
    return (error as Partial<ApiError>).message!
  }
  return typeof error === 'string' ? error : 'Something went wrong. Please try again.'
}
