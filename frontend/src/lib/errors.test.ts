import { afterEach, describe, expect, it } from 'vitest'
import { errorMessage } from './errors'
import { setLocale } from './i18n'

describe('Tauri error formatting', () => {
  afterEach(() => setLocale('en'))

  it('uses the message from structured command errors', () => {
    expect(errorMessage({ code: 'invalid_transition', message: 'invalid transition' })).toBe(
      'invalid transition'
    )
  })

  it('does not stringify unknown objects as object Object', () => {
    expect(errorMessage({ unexpected: true })).toBe('Something went wrong. Please try again.')
  })

  it("resolves a settings validation error to a translated message, never the engine's English text", () => {
    const raw = {
      code: 'invalid_settings',
      message: 'fixed break times must be unique local minutes within a day',
      field: 'fixed_breaks',
    }
    expect(errorMessage(raw)).toBe('Fixed break times must be unique times within a day.')
    setLocale('de')
    expect(errorMessage(raw)).toBe(
      'Feste Pausenzeiten müssen innerhalb eines Tages eindeutig sein.'
    )
  })

  it('falls back to the raw message for an unrecognized settings field, rather than throwing', () => {
    expect(
      errorMessage({ code: 'invalid_settings', message: 'future field', field: 'not_a_real_field' })
    ).toBe('future field')
  })
})
