import { coveredDisplaysOf, deliveryModeOf } from './delivery'
import { minutesToClock } from './format'
import { t, tCount } from './i18n'
import type { Settings } from './types'

/**
 * Renders what a Settings object will actually *do*, in plain language.
 *
 * The settings panes ask a person to simulate the app in their head: "Balanced",
 * a 30-second notice, a long-break cadence and a postpone limit together describe
 * a behaviour that no single control states. Showing the outcome removes most of
 * the need to explain the inputs one by one.
 *
 * Pure and driven entirely by `t`, so it is directly unit-testable per locale.
 */

/** Whole minutes read as minutes; anything else stays in seconds. */
const duration = (seconds: number): string =>
  seconds >= 60 && seconds % 60 === 0
    ? tCount('unit_minutes', seconds / 60)
    : tCount('unit_seconds', seconds)

const ALL_DAYS = 0b111_1111

/**
 * Bit 0 of `active_days_mask` is Sunday (1 Jan 2023 was a Sunday, which is why the
 * Intl lookup below starts there), but people read a work week starting Monday —
 * so contiguity is measured in Monday-first order, not bit order.
 */
const MONDAY_FIRST = [1, 2, 3, 4, 5, 6, 0]

const describeDays = (mask: number, locale: string): string => {
  if ((mask & ALL_DAYS) === ALL_DAYS) return t('summary_days_all')
  const names = Array.from({ length: 7 }, (_, index) =>
    new Intl.DateTimeFormat(locale, { weekday: 'short' }).format(new Date(2023, 0, 1 + index))
  )
  const active = MONDAY_FIRST.map((day, position) => ({ day, position })).filter(
    ({ day }) => (mask & (1 << day)) !== 0
  )
  const contiguous = active.every(
    (entry, index) => index === 0 || entry.position === active[index - 1].position + 1
  )
  // A two-day range reads worse as "Mon–Tue" than as "Mon, Tue".
  return contiguous && active.length > 2
    ? `${names[active[0].day]}–${names[active[active.length - 1].day]}`
    : active.map(({ day }) => names[day]).join(', ')
}

export function describeSettings(settings: Settings): string[] {
  const locale = settings.locale === 'de' ? 'de-DE' : 'en-US'
  const sentences: string[] = []

  const days = settings.active_days_mask & ALL_DAYS
  // The engine rejects an empty mask, so an empty schedule sentence is skipped
  // rather than rendered as a falsehood like "active every day".
  if (days !== 0) {
    const dayLabel = describeDays(days, locale)
    sentences.push(
      // active_at() treats equal start and end as always-on (settings.rs:404) —
      // it looks like a zero-length window but means round the clock.
      settings.active_start_minutes === settings.active_end_minutes
        ? t('summary_schedule_all_day', { days: dayLabel })
        : t('summary_schedule', {
            days: dayLabel,
            start: minutesToClock(settings.active_start_minutes),
            // active_end_minutes is exclusive, hence "until" rather than an en-dash
            // range, which would read as inclusive.
            end: minutesToClock(settings.active_end_minutes),
          })
    )
  }

  sentences.push(
    t('summary_rhythm', {
      work: duration(settings.work_seconds),
      rest: duration(settings.short_break_seconds),
    })
  )

  if (settings.pre_break_seconds > 0) {
    sentences.push(t('summary_notice', { notice: duration(settings.pre_break_seconds) }))
  }

  if (settings.long_break_every) {
    sentences.push(
      t('summary_long_break', {
        count: settings.long_break_every,
        duration: duration(settings.long_break_seconds),
      })
    )
  }

  const mode = deliveryModeOf(settings)
  if (mode === 'notify') {
    sentences.push(t('summary_delivery_notify'))
  } else {
    const displays = t(
      `summary_displays_${coveredDisplaysOf(settings)}` as
        'summary_displays_all' | 'summary_displays_active' | 'summary_displays_primary'
    )
    sentences.push(
      t(
        `summary_delivery_${mode}` as
          'summary_delivery_ask' | 'summary_delivery_cover' | 'summary_delivery_hold',
        { displays }
      )
    )
    // Postponing is only reachable in Balanced: every pointer path to postpone()
    // is gated on it, so promising a limit under Firm/Strict would be misleading.
    if (mode === 'ask') {
      sentences.push(
        settings.postpone_limit == null
          ? t('summary_postpone_unlimited')
          : tCount('summary_postpone', settings.postpone_limit)
      )
    }
  }

  return sentences
}
