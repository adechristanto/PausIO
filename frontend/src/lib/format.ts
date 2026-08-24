const pad = (n: number) => n.toString().padStart(2, '0')

export const formatClock = (seconds: number) =>
  `${pad(Math.floor(seconds / 60))}:${pad(seconds % 60)}`

export const minutesToClock = (minutes: number) =>
  `${pad(Math.floor(minutes / 60))}:${pad(minutes % 60)}`

/** Callers must guard the result with Number.isFinite — an empty or malformed input yields NaN. */
export const clockToMinutes = (value: string) => {
  const [hours, minutes] = value.split(':').map(Number)
  return hours * 60 + minutes
}

/**
 * The one place a wall-clock time is rendered from the app's own language
 * setting rather than the OS/browser locale — every other call site
 * (`toLocaleTimeString([], ...)`, `Intl.DateTimeFormat(undefined, ...)`)
 * quietly follows whatever locale the OS happens to be in, which can show a
 * mismatched 12h/24h format next to an otherwise fully German or English UI.
 */
export const formatTimeOfDay = (date: Date, locale: 'en' | 'de' | null | undefined) => {
  const bcp47 = locale === 'de' ? 'de-DE' : 'en-US'
  return new Intl.DateTimeFormat(bcp47, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: bcp47 !== 'de-DE',
  }).format(date)
}
