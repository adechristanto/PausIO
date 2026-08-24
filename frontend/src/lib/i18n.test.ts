import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { pauseLabel, setLocale, t, tCount } from './i18n'

afterEach(() => setLocale('en'))

describe('M1 localization and accessibility baseline', () => {
  it('uses stable keys and interpolates English values', () => {
    expect(t('action_take_break')).toBe('Eye break now')
    expect(t('break_postpone')).toBe('Postpone 2 min')
  })

  it('switches the complete interface catalogue to German', () => {
    setLocale('de')
    expect(t('settings_heading')).toBe('Einstellungen')
    expect(t('break_start_short', { seconds: 20 })).toBe('20-Sekunden-Pause starten')
    expect(pauseLabel('screen_lock')).toBe('Pausiert · Bildschirm gesperrt')
  })

  it('humanizes pause reasons without leaking raw enum values into the UI', () => {
    expect(pauseLabel('manual')).toBe('Paused')
    expect(pauseLabel('screen_lock')).toBe('Paused · screen locked')
  })

  it('labels diagnostic delivery without claiming haptics', () => {
    expect(t('watch_nudge_result', { value: 'queued' })).toContain('not a haptic confirmation')
  })

  it('pluralizes a streak count per the active language, never "1 Tage"', () => {
    expect(tCount('history_streak', 1)).toBe('1-day rhythm')
    expect(tCount('history_streak', 2)).toBe('2-day rhythm')
    setLocale('de')
    expect(tCount('history_streak', 1)).toBe('1 Tag im Rhythmus')
    expect(tCount('history_streak', 2)).toBe('2 Tage im Rhythmus')
  })

  it('keeps keyboard focus and reduced-motion protections in the stylesheet', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8')
    expect(css).toContain(':focus-visible')
    expect(css).toContain('@media (prefers-reduced-motion: reduce)')
    expect(css).toContain('safe-area-inset-top')
    expect(css).not.toContain('fonts.googleapis.com')
  })

  it('shortens, but never removes, the break overlay entrance fade under Reduce Motion', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8')
    expect(css).toMatch(/\.break-overlay\s*\{[^}]*opacity:\s*0[^}]*transition:\s*opacity/)
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: reduce\)[\s\S]*?\.break-overlay\s*\{\s*transition-duration:\s*150ms/
    )
  })

  it('guarantees complete translation parity and placeholder matching between English and German', () => {
    const file = readFileSync(resolve(process.cwd(), 'src/lib/i18n.ts'), 'utf8')
    const enStart = file.indexOf('const english = {')
    const enEnd = file.indexOf('} as const')
    const enStr = file.substring(enStart + 'const english = '.length, enEnd + 1)

    const deStart = file.indexOf('const german: Record<LocalizationKey, string> = {')
    const deEnd = file.indexOf('\nexport function setLocale')
    const deStr = file.substring(
      deStart + 'const german: Record<LocalizationKey, string> = '.length,
      deEnd
    )

    const enCatalogue = eval(`(${enStr})`) as Record<string, string>
    const deCatalogue = eval(`(${deStr})`) as Record<string, string>

    const enKeys = Object.keys(enCatalogue).sort()
    const deKeys = Object.keys(deCatalogue).sort()

    expect(deKeys).toEqual(enKeys)

    for (const key of enKeys) {
      const enVal = enCatalogue[key]
      const deVal = deCatalogue[key]
      expect(typeof enVal).toBe('string')
      expect(typeof deVal).toBe('string')
      expect(enVal.trim().length).toBeGreaterThan(0)
      expect(deVal.trim().length).toBeGreaterThan(0)

      const enPlaceholders = (enVal.match(/\{[a-zA-Z0-9_]+\}/g) ?? []).sort()
      const dePlaceholders = (deVal.match(/\{[a-zA-Z0-9_]+\}/g) ?? []).sort()
      expect(dePlaceholders).toEqual(enPlaceholders)
    }
  })

  it('keeps forced-colors and prefers-contrast support in the stylesheet', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8')
    expect(css).toContain('@media (forced-colors: active)')
    expect(css).toContain('@media (prefers-contrast: more)')
    // The selected day chip and the timer ring must survive forced colors distinctly —
    // regressions here are exactly what shipped before this fix (E11/A2).
    expect(css).toMatch(/\.day-picker button\.chosen\s*\{[^}]*border-color:\s*Highlight/)
    expect(css).toMatch(
      /\.timer-ring,\s*\.break-overlay,\s*\.break-prompt\s*\{\s*forced-color-adjust:\s*none/
    )
  })
})
