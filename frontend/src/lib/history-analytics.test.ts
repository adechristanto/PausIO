import { describe, expect, it } from 'vitest'
import { analyzeHistory, bucketByPeriod } from './history-analytics'
import type { HistoryEvent } from './types'

const at = (day: string, kind: HistoryEvent['kind']): HistoryEvent => ({
  occurred_at: `${day}T12:00:00Z`,
  kind,
})

describe('analyzeHistory', () => {
  it('keeps repeated postponements and the final completion in one opportunity', () => {
    const analytics = analyzeHistory(
      [
        { ...at('2026-07-26', 'due'), break_id: 'break-1' },
        { ...at('2026-07-26', 'postponed'), break_id: 'break-1' },
        { ...at('2026-07-26', 'due'), break_id: 'break-1' },
        { ...at('2026-07-26', 'postponed'), break_id: 'break-1' },
        { ...at('2026-07-26', 'started'), break_id: 'break-1' },
        { ...at('2026-07-26', 'completed'), break_id: 'break-1' },
      ],
      new Date('2026-07-26T14:00:00Z'),
      1
    )

    expect(analytics.opportunities).toHaveLength(1)
    expect(analytics.opportunities[0]).toMatchObject({ status: 'completed', postpones: 2 })
    expect(analytics.today).toMatchObject({ due: 1, completed: 1, postponed: 1 })
  })

  it('does not count an extra manual break as a recommended opportunity', () => {
    const analytics = analyzeHistory(
      [
        { ...at('2026-07-26', 'started'), break_id: 'manual-1' },
        { ...at('2026-07-26', 'completed'), break_id: 'manual-1' },
      ],
      new Date('2026-07-26T14:00:00Z'),
      1
    )

    expect(analytics.opportunities[0].status).toBe('completed')
    expect(analytics.opportunities[0].recommended).toBe(false)
    expect(analytics.today.due).toBe(0)
  })

  it('reveals the Routine Score only after enough resolved opportunities and workdays', () => {
    const events: HistoryEvent[] = []
    for (const [index, day] of ['2026-07-24', '2026-07-25', '2026-07-26'].entries()) {
      for (let opportunity = 0; opportunity < 2; opportunity += 1) {
        const id = `${index}-${opportunity}`
        events.push({
          break_id: id,
          occurred_at: `${day}T10:00:00`,
          kind: 'due',
          work_interval_seconds: 1200,
          target_break_seconds: 20,
        })
        events.push({ break_id: id, occurred_at: `${day}T10:02:00`, kind: 'started' })
        events.push({ break_id: id, occurred_at: `${day}T10:02:20`, kind: 'completed' })
      }
    }

    const analytics = analyzeHistory(events, new Date('2026-07-26T14:00:00'), 7)

    expect(analytics.routineScore.sufficient).toBe(true)
    expect(analytics.routineScore.value).toBe(100)
    expect(analytics.routineScore).toMatchObject({ followThrough: 70, timing: 20, consistency: 10 })
  })

  it('detects a time-block opportunity only after both blocks have enough samples', () => {
    const events: HistoryEvent[] = []
    for (let index = 0; index < 5; index += 1) {
      const morning = `morning-${index}`
      const afternoon = `afternoon-${index}`
      events.push({ break_id: morning, occurred_at: `2026-07-${20 + index}T09:00:00`, kind: 'due' })
      events.push({
        break_id: morning,
        occurred_at: `2026-07-${20 + index}T09:01:00`,
        kind: 'completed',
      })
      events.push({
        break_id: afternoon,
        occurred_at: `2026-07-${20 + index}T15:00:00`,
        kind: 'due',
      })
      events.push({
        break_id: afternoon,
        occurred_at: `2026-07-${20 + index}T15:02:00`,
        kind: 'skipped',
      })
    }

    const analytics = analyzeHistory(events, new Date('2026-07-26T18:00:00'), 7)

    expect(analytics.patterns).toContainEqual(
      expect.objectContaining({ kind: 'time_block', key: 'afternoon', percent: 0 })
    )
  })

  it('notes schedule changes when captured opportunity settings differ', () => {
    const analytics = analyzeHistory(
      [
        { ...at('2026-07-25', 'due'), break_id: 'one', schedule_fingerprint: '1200:20' },
        { ...at('2026-07-25', 'completed'), break_id: 'one' },
        { ...at('2026-07-26', 'due'), break_id: 'two', schedule_fingerprint: '1500:20' },
        { ...at('2026-07-26', 'completed'), break_id: 'two' },
      ],
      new Date('2026-07-26T18:00:00'),
      7
    )

    expect(analytics.scheduleChanged).toBe(true)
    expect(analytics.insights).toContainEqual(expect.objectContaining({ kind: 'schedule_change' }))
  })

  it('uses only prompted and completed timer events and never treats empty days as success', () => {
    const analytics = analyzeHistory(
      [
        at('2026-07-24', 'due'),
        at('2026-07-24', 'completed'),
        at('2026-07-25', 'due'),
        at('2026-07-25', 'completed'),
        at('2026-07-25', 'due'),
      ],
      new Date('2026-07-26T14:00:00Z'),
      3
    )
    expect(analytics.days.map((day) => day.percent)).toEqual([100, 50, null])
    expect(analytics.streakDays).toBe(0)
  })

  it('counts an 80 percent consecutive-day streak without inferring unseen breaks', () => {
    const analytics = analyzeHistory(
      [
        at('2026-07-25', 'due'),
        at('2026-07-25', 'completed'),
        at('2026-07-26', 'due'),
        at('2026-07-26', 'completed'),
        at('2026-07-26', 'due'),
        at('2026-07-26', 'completed'),
        at('2026-07-26', 'due'),
        at('2026-07-26', 'completed'),
        at('2026-07-26', 'due'),
        at('2026-07-26', 'completed'),
        at('2026-07-26', 'due'),
      ],
      new Date('2026-07-26T14:00:00Z'),
      2
    )
    expect(analytics.today.percent).toBe(80)
    expect(analytics.streakDays).toBe(2)
  })

  it('never credits a manual or duplicate completion beyond its scheduled break instance', () => {
    const analytics = analyzeHistory(
      [
        { ...at('2026-07-26', 'due'), break_id: 'scheduled-1', schema_version: 2 },
        { ...at('2026-07-26', 'completed'), break_id: 'scheduled-1', schema_version: 2 },
        { ...at('2026-07-26', 'completed'), break_id: 'manual-1', schema_version: 2 },
        { ...at('2026-07-26', 'completed'), break_id: 'scheduled-1', schema_version: 2 },
      ],
      new Date('2026-07-26T14:00:00Z'),
      1
    )
    expect(analytics.today).toMatchObject({ due: 1, completed: 1, percent: 100 })
  })

  it('never credits a skipped break as completed, and reports it separately', () => {
    const analytics = analyzeHistory(
      [
        { ...at('2026-07-26', 'due'), break_id: 'break-1', schema_version: 3 },
        { ...at('2026-07-26', 'skipped'), break_id: 'break-1', schema_version: 3 },
      ],
      new Date('2026-07-26T14:00:00Z'),
      1
    )
    expect(analytics.today).toMatchObject({ due: 1, completed: 0, skipped: 1, percent: 0 })
  })
})

describe('bucketByPeriod', () => {
  it('passes day granularity through unchanged', () => {
    const analytics = analyzeHistory(
      [at('2026-07-26', 'due'), at('2026-07-26', 'completed')],
      new Date('2026-07-26T14:00:00Z'),
      2
    )
    expect(bucketByPeriod(analytics.days, 'day')).toEqual(analytics.days)
  })

  it('aggregates multiple days into one month bucket without losing totals', () => {
    const analytics = analyzeHistory(
      [
        at('2026-07-01', 'due'),
        at('2026-07-01', 'completed'),
        at('2026-07-15', 'due'),
        at('2026-07-15', 'completed'),
        at('2026-07-15', 'due'),
        at('2026-07-30', 'due'),
      ],
      new Date('2026-07-30T14:00:00Z'),
      30
    )
    const months = bucketByPeriod(analytics.days, 'month')
    expect(months).toHaveLength(1)
    expect(months[0]).toMatchObject({ key: '2026-07', due: 4, completed: 2, percent: 50 })
  })

  it('splits a 30-day span across multiple week buckets', () => {
    const analytics = analyzeHistory(
      [at('2026-07-01', 'due'), at('2026-07-01', 'completed'), at('2026-07-28', 'due')],
      new Date('2026-07-30T14:00:00Z'),
      30
    )
    const weeks = bucketByPeriod(analytics.days, 'week')
    expect(weeks.length).toBeGreaterThan(1)
    const totalDue = weeks.reduce((sum, week) => sum + week.due, 0)
    expect(totalDue).toBe(2)
  })
})
