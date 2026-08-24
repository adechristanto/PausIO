import type { Settings } from './types'

/**
 * The Work/Home "profiles" feature snapshots and restores every field, including
 * `locale`, `theme`, `accent`, and keyboard shortcuts — right for a person's own
 * saved setup, wrong for a starting point offered to everyone. A preset only ever
 * touches timing, delivery, and the nudge extras, so applying one cannot silently
 * change your language or re-bind a shortcut.
 */
export type PresetId = 'classic' | 'gentle' | 'focus_blocks' | 'eye_strain_recovery'

export type PresetPatch = Pick<
  Settings,
  | 'work_seconds'
  | 'short_break_seconds'
  | 'long_break_seconds'
  | 'long_break_every'
  | 'pre_break_seconds'
  | 'postpone_limit'
  | 'strictness'
  | 'display_target'
  | 'blink_nudge_minutes'
  | 'posture_nudge_minutes'
  | 'hydration_nudge_minutes'
>

export const PRESET_IDS: readonly PresetId[] = [
  'classic',
  'gentle',
  'focus_blocks',
  'eye_strain_recovery',
]

export const PRESETS: Record<PresetId, PresetPatch> = {
  // The shipped default rhythm, delivered as "ask first" rather than left silent.
  classic: {
    work_seconds: 20 * 60,
    short_break_seconds: 20,
    long_break_seconds: 5 * 60,
    long_break_every: null,
    pre_break_seconds: 30,
    postpone_limit: 3,
    strictness: 'balanced',
    display_target: 'all',
    blink_nudge_minutes: null,
    posture_nudge_minutes: null,
    hydration_nudge_minutes: null,
  },
  // Never covers the screen, never runs out of postpones -- for someone who wants
  // the reminder and nothing that can interrupt a call or a deploy.
  gentle: {
    work_seconds: 20 * 60,
    short_break_seconds: 20,
    long_break_seconds: 5 * 60,
    long_break_every: null,
    pre_break_seconds: 30,
    postpone_limit: null,
    strictness: 'gentle',
    display_target: 'notification_only',
    blink_nudge_minutes: null,
    posture_nudge_minutes: null,
    hydration_nudge_minutes: null,
  },
  // A Pomodoro-shaped rhythm: longer stretches, a real break every fourth round.
  focus_blocks: {
    work_seconds: 25 * 60,
    short_break_seconds: 30,
    long_break_seconds: 5 * 60,
    long_break_every: 4,
    pre_break_seconds: 30,
    postpone_limit: 3,
    strictness: 'balanced',
    display_target: 'all',
    blink_nudge_minutes: null,
    posture_nudge_minutes: null,
    hydration_nudge_minutes: null,
  },
  // For someone whose eyes are already the problem: breaks arrive without asking,
  // postponing is capped hard, and a blink reminder runs between them.
  eye_strain_recovery: {
    work_seconds: 20 * 60,
    short_break_seconds: 20,
    long_break_seconds: 5 * 60,
    long_break_every: null,
    pre_break_seconds: 10,
    postpone_limit: 1,
    strictness: 'firm',
    display_target: 'all',
    blink_nudge_minutes: 10,
    posture_nudge_minutes: null,
    hydration_nudge_minutes: null,
  },
}

export const applyPreset = (settings: Settings, id: PresetId): Settings => ({
  ...settings,
  ...PRESETS[id],
})
