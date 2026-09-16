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
 *
 * `controlId` must match the `id` on (or inside) the corresponding control in
 * SettingsPanel.svelte -- it's how a search result scrolls to and focuses the
 * actual control instead of just switching panes and leaving a person to find
 * it themselves.
 */
export interface SettingsIndexEntry {
  category: SettingsCategory
  advanced: boolean
  labelKey: LocalizationKey
  hintKey?: LocalizationKey
  controlId: string
}

export const SETTINGS_INDEX: SettingsIndexEntry[] = [
  // Breaks -- default view
  { category: 'breaks', advanced: false, labelKey: 'presets_heading', controlId: 'setting-presets' },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_work_interval',
    controlId: 'setting-work-interval',
  },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_delivery_mode',
    controlId: 'delivery-mode',
  },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_display_target',
    controlId: 'setting-display-target',
  },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_postpone_limit',
    controlId: 'setting-postpone-limit',
  },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_sound_timing',
    hintKey: 'setting_sound_timing_hint',
    controlId: 'setting-sound-timing',
  },
  {
    category: 'breaks',
    advanced: false,
    labelKey: 'setting_notification_sound_name',
    controlId: 'setting-notification-sound-name',
  },
  // Breaks -- behind "More settings"
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_eye_break',
    hintKey: 'setting_eye_break_hint',
    controlId: 'setting-eye-break',
  },
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_longer_breaks',
    hintKey: 'setting_longer_breaks_hint',
    controlId: 'setting-longer-breaks',
  },
  { category: 'breaks', advanced: true, labelKey: 'setting_warning', controlId: 'setting-warning' },
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_blink_nudge',
    controlId: 'setting-blink-nudge',
  },
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_posture_nudge',
    controlId: 'setting-posture-nudge',
  },
  {
    category: 'breaks',
    advanced: true,
    labelKey: 'setting_hydration_nudge',
    controlId: 'setting-hydration-nudge',
  },

  // Schedule -- default view
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'setting_active_days',
    controlId: 'setting-active-days',
  },
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'setting_round_the_clock',
    controlId: 'setting-round-the-clock',
  },
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'setting_start_time',
    controlId: 'setting-start-time',
  },
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'setting_end_time',
    controlId: 'setting-end-time',
  },
  {
    category: 'schedule',
    advanced: false,
    labelKey: 'section_profiles',
    hintKey: 'section_profiles_hint',
    controlId: 'setting-profiles',
  },
  // Schedule -- behind "More settings"
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_context',
    controlId: 'setting-context',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_auto_detect_fullscreen',
    hintKey: 'setting_auto_detect_fullscreen_hint',
    controlId: 'setting-auto-detect-fullscreen',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_auto_detect_dnd',
    hintKey: 'setting_auto_detect_dnd_hint',
    controlId: 'setting-auto-detect-dnd',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_fixed_breaks',
    hintKey: 'setting_fixed_breaks_hint',
    controlId: 'setting-fixed-breaks',
  },
  {
    category: 'schedule',
    advanced: true,
    labelKey: 'setting_daily_focus_limit',
    controlId: 'setting-daily-focus-limit',
  },

  // Appearance -- default view
  {
    category: 'appearance',
    advanced: false,
    labelKey: 'setting_language',
    controlId: 'setting-language',
  },
  { category: 'appearance', advanced: false, labelKey: 'setting_theme', controlId: 'setting-theme' },
  {
    category: 'appearance',
    advanced: false,
    labelKey: 'setting_accent',
    controlId: 'setting-accent',
  },
  // Appearance -- behind "More settings"
  {
    category: 'appearance',
    advanced: true,
    labelKey: 'setting_routine',
    controlId: 'setting-routine',
  },
  {
    category: 'appearance',
    advanced: true,
    labelKey: 'setting_messages',
    hintKey: 'setting_messages_hint',
    controlId: 'setting-messages',
  },
  {
    category: 'appearance',
    advanced: true,
    labelKey: 'setting_show_clock',
    hintKey: 'setting_show_clock_hint',
    controlId: 'setting-show-clock',
  },

  // Shortcuts & startup -- no "More settings" section
  {
    category: 'shortcuts',
    advanced: false,
    labelKey: 'setting_end_break_shortcut',
    controlId: 'setting-end-break-shortcut',
  },
  {
    category: 'shortcuts',
    advanced: false,
    labelKey: 'setting_pause_toggle_shortcut',
    controlId: 'setting-pause-toggle-shortcut',
  },
  {
    category: 'shortcuts',
    advanced: false,
    labelKey: 'setting_take_break_shortcut',
    controlId: 'setting-take-break-shortcut',
  },
  {
    category: 'shortcuts',
    advanced: false,
    labelKey: 'setting_start_at_login',
    hintKey: 'setting_start_at_login_hint',
    controlId: 'setting-start-at-login',
  },

  // History and privacy -- default view
  {
    category: 'privacy',
    advanced: false,
    labelKey: 'setting_history_enabled',
    controlId: 'setting-history-enabled',
  },
  {
    category: 'privacy',
    advanced: false,
    labelKey: 'setting_history_retention',
    controlId: 'setting-history-retention',
  },
  {
    category: 'privacy',
    advanced: false,
    labelKey: 'setting_show_routine_score',
    hintKey: 'setting_show_routine_score_hint',
    controlId: 'setting-show-routine-score',
  },
  // History and privacy -- behind "More settings"
  {
    category: 'privacy',
    advanced: true,
    labelKey: 'privacy_reset',
    hintKey: 'privacy_reset_hint',
    controlId: 'setting-privacy-reset',
  },
  {
    category: 'privacy',
    advanced: true,
    labelKey: 'diagnostics_heading',
    controlId: 'setting-diagnostics',
  },
]
