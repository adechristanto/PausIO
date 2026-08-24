import { describe, expect, it } from 'vitest'
import { clockToMinutes, formatClock, minutesToClock } from './format'

describe('format', () => {
  it('formats seconds as a zero-padded clock', () => {
    expect(formatClock(0)).toBe('00:00')
    expect(formatClock(59)).toBe('00:59')
    expect(formatClock(1188)).toBe('19:48')
    expect(formatClock(3600)).toBe('60:00')
  })

  it('formats minutes-of-day as a zero-padded clock', () => {
    expect(minutesToClock(0)).toBe('00:00')
    expect(minutesToClock(540)).toBe('09:00')
    expect(minutesToClock(1080)).toBe('18:00')
  })

  it('parses a clock string back to minutes-of-day', () => {
    expect(clockToMinutes('09:00')).toBe(540)
    expect(clockToMinutes('18:00')).toBe(1080)
    expect(clockToMinutes('00:00')).toBe(0)
  })

  it('yields NaN for malformed input, which callers must guard against', () => {
    expect(Number.isFinite(clockToMinutes(''))).toBe(false)
    expect(Number.isFinite(clockToMinutes('bad'))).toBe(false)
  })
})
