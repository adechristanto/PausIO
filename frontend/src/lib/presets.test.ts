import { describe, expect, it } from 'vitest'
import { applyPreset, PRESET_IDS, PRESETS } from './presets'
import type { Settings } from './types'

const base: Settings = {
  work_seconds: 999,
  short_break_seconds: 999,
  long_break_seconds: 999,
  long_break_every: 7,
  pre_break_seconds: 999,
  active_days_mask: 0b1,
  active_start_minutes: 1,
  active_end_minutes: 2,
  postpone_limit: 99,
  locale: 'de',
  theme: 'dark',
  accent: 'lilac',
  end_break_shortcut: 'CmdOrCtrl+Z',
}

describe('applyPreset', () => {
  it('never touches locale, theme, accent, or shortcuts', () => {
    for (const id of PRESET_IDS) {
      const next = applyPreset(base, id)
      expect(next.locale).toBe('de')
      expect(next.theme).toBe('dark')
      expect(next.accent).toBe('lilac')
      expect(next.end_break_shortcut).toBe('CmdOrCtrl+Z')
    }
  })

  it('never touches the schedule (days, hours)', () => {
    for (const id of PRESET_IDS) {
      const next = applyPreset(base, id)
      expect(next.active_days_mask).toBe(0b1)
      expect(next.active_start_minutes).toBe(1)
      expect(next.active_end_minutes).toBe(2)
    }
  })

  it('overwrites every timing and delivery field the preset defines', () => {
    const next = applyPreset(base, 'classic')
    expect(next.work_seconds).toBe(PRESETS.classic.work_seconds)
    expect(next.strictness).toBe('balanced')
  })

  it('applies a fully consistent delivery pair for every preset', () => {
    // A preset that ships an inconsistent strictness/display_target pair would
    // reintroduce exactly the contradiction the merged delivery-mode picker exists
    // to make unrepresentable (see lib/delivery.ts).
    for (const id of PRESET_IDS) {
      const preset = PRESETS[id]
      const isNotify =
        preset.strictness === 'gentle' || preset.display_target === 'notification_only'
      if (isNotify) {
        expect(preset.strictness).toBe('gentle')
        expect(preset.display_target).toBe('notification_only')
      } else {
        expect(preset.display_target).not.toBe('notification_only')
      }
    }
  })

  it('gentle never covers the screen and never runs out of postpones', () => {
    expect(PRESETS.gentle.strictness).toBe('gentle')
    expect(PRESETS.gentle.postpone_limit).toBeNull()
  })

  it('eye_strain_recovery caps postpones hard and adds a blink reminder', () => {
    expect(PRESETS.eye_strain_recovery.postpone_limit).toBe(1)
    expect(PRESETS.eye_strain_recovery.blink_nudge_minutes).toBe(10)
  })
})
