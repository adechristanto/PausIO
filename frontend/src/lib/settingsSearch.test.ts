import { afterEach, describe, expect, it } from 'vitest'
import { t } from './i18n'
import { setLocale } from './i18n'
import { SETTINGS_INDEX } from './settingsIndex'
import { searchSettings } from './settingsSearch'

afterEach(() => setLocale('en'))

describe('SETTINGS_INDEX', () => {
  it('resolves every label and hint key in both catalogues, catching typos', () => {
    for (const locale of ['en', 'de'] as const) {
      setLocale(locale)
      for (const entry of SETTINGS_INDEX) {
        const label = t(entry.labelKey)
        expect(label).not.toBe(entry.labelKey) // t() falls back to the raw key if missing
        expect(label.length).toBeGreaterThan(0)
        if (entry.hintKey) {
          const hint = t(entry.hintKey)
          expect(hint).not.toBe(entry.hintKey)
        }
      }
    }
  })

  it('covers all five settings categories', () => {
    const categories = new Set(SETTINGS_INDEX.map((entry) => entry.category))
    expect(categories).toEqual(
      new Set(['breaks', 'schedule', 'appearance', 'shortcuts', 'privacy'])
    )
  })
})

describe('searchSettings', () => {
  it('returns nothing for an empty or whitespace-only query', () => {
    expect(searchSettings('')).toEqual([])
    expect(searchSettings('   ')).toEqual([])
  })

  it('matches by label, case-insensitively', () => {
    const results = searchSettings('POSTPONE')
    expect(results.some((r) => r.entry.labelKey === 'setting_postpone_limit')).toBe(true)
  })

  it('matches a control only findable by its hint text', () => {
    // "Round the clock" is the label; nothing in it mentions fixed break times,
    // so this specifically exercises the hint-text side of the search.
    const results = searchSettings('12:30')
    expect(results.some((r) => r.entry.labelKey === 'setting_fixed_breaks')).toBe(true)
  })

  it('finds a control that lives behind "More settings", not just the default view', () => {
    const results = searchSettings('blink')
    const hit = results.find((r) => r.entry.labelKey === 'setting_blink_nudge')
    expect(hit?.entry.advanced).toBe(true)
    expect(hit?.entry.category).toBe('breaks')
  })

  it('finds results across different categories for a broad query', () => {
    const results = searchSettings('break')
    const categories = new Set(results.map((r) => r.entry.category))
    expect(categories.size).toBeGreaterThan(1)
  })

  it('follows the active locale', () => {
    setLocale('de')
    const results = searchSettings('Sprache')
    expect(results.some((r) => r.entry.labelKey === 'setting_language')).toBe(true)
    expect(results[0].label).toBe('Sprache')
  })
})
