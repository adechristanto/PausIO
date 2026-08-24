import { describe, expect, it } from 'vitest'
import { isMinuteInActiveWindow } from './schedule'

describe('isMinuteInActiveWindow', () => {
  it('matches the ordinary same-day window, end exclusive', () => {
    expect(isMinuteInActiveWindow(540, 540, 1080)).toBe(true) // 09:00
    expect(isMinuteInActiveWindow(1079, 540, 1080)).toBe(true) // 17:59
    expect(isMinuteInActiveWindow(1080, 540, 1080)).toBe(false) // 18:00, exclusive
    expect(isMinuteInActiveWindow(539, 540, 1080)).toBe(false) // 08:59
  })

  it('treats an equal start and end as round the clock', () => {
    expect(isMinuteInActiveWindow(0, 600, 600)).toBe(true)
    expect(isMinuteInActiveWindow(1439, 600, 600)).toBe(true)
  })

  it('wraps past midnight when start is after end', () => {
    // 22:00 - 06:00
    expect(isMinuteInActiveWindow(23 * 60, 22 * 60, 6 * 60)).toBe(true)
    expect(isMinuteInActiveWindow(0, 22 * 60, 6 * 60)).toBe(true)
    expect(isMinuteInActiveWindow(5 * 60 + 59, 22 * 60, 6 * 60)).toBe(true)
    expect(isMinuteInActiveWindow(6 * 60, 22 * 60, 6 * 60)).toBe(false)
    expect(isMinuteInActiveWindow(12 * 60, 22 * 60, 6 * 60)).toBe(false)
  })
})
