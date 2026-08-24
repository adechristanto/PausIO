import type { LocalizationKey } from './i18n'

export type SettingsCategory = 'breaks' | 'schedule' | 'appearance' | 'shortcuts' | 'privacy'

/**
 * A static map of every setting to where it lives, so a search box can find a
 * control regardless of which pane it is in or whether it is behind "More
 * settings" -- without this, finding "postpone" or "fixed break" means guessing
 * which of five panes, and possibly opening a disclosure, to check first.
 *
 * `hintKey` is included in the search text precisely because most of the
 * project's copy effort went into hints, not labels ("Round the clock" would
 * not otherwise be findable by typing "midnight" or "always on", say) -- and
 * keeping this list honest is itself pressure to keep writing hints.
 */
export interface SettingsIndexEntry {
  category: SettingsCategory
  advanced: boolean
  labelKey: LocalizationKey
  hintKey?: LocalizationKey
}

export const SETTINGS_INDEX: SettingsIndexEntry[] = [
  // Breaks -- default view
  { category: 'breaks', advanced: false, labelKey: 'presets_heading' },
  { category: 'breaks', advanced: false, labelKey: 'setting_work_interval' },
  { category: 'breaks', advanced: false, labelKey: 'setting_delivery_mode' },
  { category: 'breaks', advanced: false, labelKey: 'setting_display_target' },
  { category: 'breaks', advanced: false, labelKey: 'setting_postpone_limit' },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_notification_sound',
    hintKey: 'setting_notification_sound_hint',
  },
  { category: 'breaks', advanced: false, labelKey: 'setting_notification_sound_name' },
  { category: 'breaks', advanced: false, labelKey: 'setting_sound_theme' },
  { category: 'breaks', advanced: false, labelKey: 'setting_sound_volume' },
  // Breaks -- behind "More settings"
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_eye_break',
    hintKey: 'setting_eye_break_hint',
  },
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_longer_breaks',
    hintKey: 'setting_longer_breaks_hint',
  },
  { category: 'breaks', advanced: true, labelKey: 'setting_warning' },
  { category: 'breaks', advanced: true, labelKey: 'setting_blink_nudge' },
  { category: 'breaks', advanced: true, labelKey: 'setting_posture_nudge' },
  { category: 'breaks', advanced: true, labelKey: 'setting_hydration_nudge' },

  // Schedule -- default view
  { category: 'schedule', advanced: false, labelKey: 'setting_active_days' },
  { category: 'schedule', advanced: false, labelKey: 'setting_round_the_clock' },
  { category: 'schedule', advanced: false, labelKey: 'setting_start_time' },
  { category: 'schedule', advanced: false, labelKey: 'setting_end_time' },
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'section_profiles',
    hintKey: 'section_profiles_hint',
  },
  // Schedule -- behind "More settings"
  { category: 'schedule', advanced: true, labelKey: 'setting_context' },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_auto_detect_fullscreen',
    hintKey: 'setting_auto_detect_fullscreen_hint',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_auto_detect_dnd',
    hintKey: 'setting_auto_detect_dnd_hint',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_fixed_breaks',
    hintKey: 'setting_fixed_breaks_hint',
  },
  { category: 'schedule', advanced: true, labelKey: 'setting_daily_focus_limit' },

  // Appearance -- default view
  { category: 'appearance', advanced: false, labelKey: 'setting_language' },
  { category: 'appearance', advanced: false, labelKey: 'setting_theme' },
  { category: 'appearance', advanced: false, labelKey: 'setting_accent' },
  // Appearance -- behind "More settings"
  { category: 'appearance', advanced: true, labelKey: 'setting_routine' },
  {
    category: 'appearance',
    advanced: true,
    labelKey: 'setting_messages',
    hintKey: 'setting_messages_hint',
  },
  {
    category: 'appearance',
    advanced: true,
    labelKey: 'setting_show_clock',
    hintKey: 'setting_show_clock_hint',
  },

  // Shortcuts & startup -- no "More settings" section
  { category: 'shortcuts', advanced: false, labelKey: 'setting_end_break_shortcut' },
  { category: 'shortcuts', advanced: false, labelKey: 'setting_pause_toggle_shortcut' },
  { category: 'shortcuts', advanced: false, labelKey: 'setting_take_break_shortcut' },
  {
    category: 'shortcuts',
    advanced: false,
    labelKey: 'setting_start_at_login',
    hintKey: 'setting_start_at_login_hint',
  },

  // History and privacy -- default view
  { category: 'privacy', advanced: false, labelKey: 'setting_history_enabled' },
  { category: 'privacy', advanced: false, labelKey: 'setting_history_retention' },
  // History and privacy -- behind "More settings"
  { category: 'privacy', advanced: true, labelKey: 'privacy_reset', hintKey: 'privacy_reset_hint' },
  { category: 'privacy', advanced: true, labelKey: 'diagnostics_heading' },
]
