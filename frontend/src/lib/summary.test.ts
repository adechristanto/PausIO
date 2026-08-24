import { afterEach, describe, expect, it } from 'vitest'
import { coveredDisplaysOf, deliveryModeOf, deliveryPatch } from './delivery'
import { setLocale } from './i18n'
import { describeSettings } from './summary'
import type { Settings } from './types'

const base: Settings = {
  work_seconds: 1200,
  short_break_seconds: 20,
  long_break_seconds: 300,
  long_break_every: null,
  pre_break_seconds: 30,
  active_days_mask: 0b0111110,
  active_start_minutes: 540,
  active_end_minutes: 1080,
  postpone_limit: null,
}

const say = (overrides: Partial<Settings> = {}) =>
  describeSettings({ ...base, ...overrides }).join(' ')

afterEach(() => setLocale('en'))

describe('describeSettings', () => {
  it('states the schedule, rhythm, notice and delivery in one paragraph', () => {
    expect(say()).toBe(
      'Active Mon–Fri, 09:00 until 18:00. Every 20 minutes: a break of 20 seconds. ' +
        'A heads-up arrives 30 seconds beforehand. A notification asks first; if you do not answer, ' +
        'the break covers all displays. You can postpone as often as you like.'
    )
  })

  it('reads an equal start and end as round the clock, not a zero-length window', () => {
    // active_at() returns true unconditionally when the two are equal (settings.rs:404).
    const text = say({ active_start_minutes: 600, active_end_minutes: 600 })
    expect(text).toContain('Active Mon–Fri, around the clock.')
    expect(text).not.toContain('until')
  })

  it('says "until" rather than a range, because active_end_minutes is exclusive', () => {
    expect(say()).toContain('09:00 until 18:00')
  })

  it('collapses a full week and lists non-contiguous days individually', () => {
    expect(say({ active_days_mask: 0b1111111 })).toContain('Active every day')
    expect(say({ active_days_mask: 0b0010010 })).toContain('Active Mon, Thu')
  })

  it('omits the notice sentence when the pre-break notice is off', () => {
    expect(say({ pre_break_seconds: 0 })).not.toContain('heads-up')
  })

  it('mentions long breaks only when the cadence is set', () => {
    expect(say()).not.toContain('instead')
    expect(say({ long_break_every: 4 })).toContain('One break in every 4 lasts 5 minutes instead')
  })

  describe('per delivery mode', () => {
    it('notify never promises a covered screen', () => {
      const text = say({ strictness: 'gentle' })
      expect(text).toContain('your screen is never covered')
      expect(text).not.toContain('postpone')
    })

    it('notification-only outranks a stored strictness, matching the engine', () => {
      // due_grace_seconds() discards strictness entirely for notification_only.
      expect(say({ strictness: 'strict', display_target: 'notification_only' })).toContain(
        'your screen is never covered'
      )
    })

    it('cover and hold describe the display and drop the postpone promise', () => {
      expect(say({ strictness: 'firm', display_target: 'active' })).toContain(
        'Breaks cover the active display straight away.'
      )
      const hold = say({ strictness: 'strict', display_target: 'primary' })
      expect(hold).toContain('the primary display straight away, and PausIO cannot be quit')
      // Postponing is Balanced-only, so no limit may be promised here.
      expect(hold).not.toContain('postpone')
    })

    it('reports a postpone limit with correct plurals under ask', () => {
      expect(say({ postpone_limit: 1 })).toContain('You can postpone once a day.')
      expect(say({ postpone_limit: 3 })).toContain('You can postpone 3 times a day.')
    })
  })

  it('renders entirely in German when the locale is German', () => {
    setLocale('de')
    const text = say({ locale: 'de', long_break_every: 4, postpone_limit: 2 })
    expect(text).toContain('Aktiv Mo–Fr, 09:00 bis 18:00.')
    expect(text).toContain('Alle 20 Minuten: eine Pause von 20 Sekunden.')
    expect(text).toContain('Du kannst 2-mal pro Tag verschieben.')
    expect(text).not.toMatch(/[A-Za-z]+ (minutes|seconds|displays)/)
  })

  it('uses singular units where the count is one', () => {
    expect(say({ short_break_seconds: 60, work_seconds: 60 })).toContain(
      'Every 1 minute: a break of 1 minute.'
    )
  })
})

describe('deliveryModeOf', () => {
  it('maps every storable pair onto a mode the engine actually delivers', () => {
    // Gentle raises no overlay, so the display choice is inert -> notify.
    expect(deliveryModeOf({ ...base, strictness: 'gentle', display_target: 'all' })).toBe('notify')
    expect(deliveryModeOf({ ...base, strictness: 'balanced' })).toBe('ask')
    expect(deliveryModeOf({ ...base, strictness: 'firm' })).toBe('cover')
    expect(deliveryModeOf({ ...base, strictness: 'strict' })).toBe('hold')
    // notification_only wins over any strictness.
    for (const strictness of ['gentle', 'balanced', 'firm', 'strict'] as const) {
      expect(deliveryModeOf({ ...base, strictness, display_target: 'notification_only' })).toBe(
        'notify'
      )
    }
  })

  it('defaults an absent strictness to ask, matching the Rust default', () => {
    expect(deliveryModeOf(base)).toBe('ask')
  })
})

describe('deliveryPatch', () => {
  it('round-trips every mode back to itself', () => {
    for (const mode of ['notify', 'ask', 'cover', 'hold'] as const) {
      const patched = { ...base, ...deliveryPatch(base, mode) }
      expect(deliveryModeOf(patched)).toBe(mode)
    }
  })

  it('never leaves a contradictory pair stored', () => {
    const notify = { ...base, ...deliveryPatch(base, 'notify') }
    expect(notify.display_target).toBe('notification_only')
    expect(notify.strictness).toBe('gentle')
    // Leaving notify restores a covering target rather than keeping notification_only.
    expect(deliveryPatch(notify, 'hold').display_target).toBe('all')
  })

  it('preserves the chosen display across a trip through notify', () => {
    const onActive: Settings = { ...base, strictness: 'firm', display_target: 'active' }
    const backToCover = { ...onActive, ...deliveryPatch(onActive, 'cover') }
    expect(coveredDisplaysOf(backToCover)).toBe('active')
  })
})
