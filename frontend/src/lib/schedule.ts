/**
 * Mirrors the time-of-day portion of `Settings::active_at` (settings.rs:405) so the
 * UI can warn about a fixed break time before saving it, not after it silently never
 * fires. Deliberately excludes the day-of-week check: a fixed break is gated by
 * `active_days_mask` separately and independently of this clock-time window, so this
 * helper answers only "is this minute inside the window", the same way the engine's
 * `Working | PreBreak` gate does for a fixed break specifically.
 */
export const isMinuteInActiveWindow = (
  minute: number,
  activeStartMinutes: number,
  activeEndMinutes: number
): boolean => {
  if (activeStartMinutes === activeEndMinutes) return true // round the clock
  if (activeStartMinutes < activeEndMinutes) {
    return minute >= activeStartMinutes && minute < activeEndMinutes
  }
  // The window wraps past midnight, e.g. 22:00-06:00.
  return minute >= activeStartMinutes || minute < activeEndMinutes
}

/** The schedule Settings::default() ships: Mon-Fri, 09:00-18:00. */
export const DEFAULT_ACTIVE_START_MINUTES = 9 * 60
export const DEFAULT_ACTIVE_END_MINUTES = 18 * 60

/** Equal start and end is round-the-clock (see isMinuteInActiveWindow above). */
export const isRoundTheClock = (startMinutes: number, endMinutes: number): boolean =>
  startMinutes === endMinutes

/**
 * The settings patch for toggling round-the-clock. Turning it on collapses the
 * window to a single instant (any value works; 0 is as good as any per
 * `isRoundTheClock`); turning it off restores the shipped default window rather
 * than some remembered value, since none is kept once the window collapses.
 */
export const roundTheClockPatch = (
  enabled: boolean
): { active_start_minutes: number; active_end_minutes: number } =>
  enabled
    ? { active_start_minutes: 0, active_end_minutes: 0 }
    : {
        active_start_minutes: DEFAULT_ACTIVE_START_MINUTES,
        active_end_minutes: DEFAULT_ACTIVE_END_MINUTES,
      }
