import type { BreakKind, ContextReason, HistoryEvent } from './types'

export type AnalyticsRange = number | 'all'
export type OpportunityStatus =
  'completed' | 'ended_early' | 'missed' | 'postponed' | 'pending' | 'manual'

export interface BreakOpportunity {
  id: string
  occurredAt: string
  dueAt: string | null
  breakKind: BreakKind | null
  status: OpportunityStatus
  recommended: boolean
  postpones: number
  contexts: ContextReason[]
  startedAt: string | null
  completedAt: string | null
  targetBreakSeconds: number | null
  workIntervalSeconds: number | null
  scheduleFingerprint: string | null
  responseSeconds: number | null
  events: HistoryEvent[]
}

export interface DailyCompliance {
  key: string
  due: number
  completed: number
  /** Ended early by explicit action, as opposed to running its course. */
  skipped: number
  missed: number
  postponed: number
  pending: number
  resolved: number
  percent: number | null
}

export interface AnalyticsInsight {
  tone: 'positive' | 'neutral' | 'opportunity'
  kind:
    | 'improvement'
    | 'steady'
    | 'postponement'
    | 'recovery'
    | 'summary'
    | 'first_steps'
    | 'time_block'
    | 'weekday'
    | 'faster_response'
    | 'schedule_change'
    | 'milestone'
  values: Record<string, number>
}

export interface RoutineScore {
  value: number
  followThrough: number
  timing: number
  consistency: number
  sufficient: boolean
  resolved: number
  eligibleDays: number
}

export interface PatternMetric {
  key: string
  completed: number
  resolved: number
  percent: number | null
}

export interface AnalyticsPattern {
  kind: 'time_block' | 'weekday' | 'postponement'
  tone: 'positive' | 'opportunity' | 'neutral'
  key: string
  comparisonKey?: string
  percent: number
  comparisonPercent?: number
  sample: number
}

export interface AnalyticsMilestone {
  kind: 'first_healthy_period' | 'healthy_days' | 'personal_best' | 'recovery'
  value: number
}

export interface HistoryAnalytics {
  today: DailyCompliance
  days: DailyCompliance[]
  opportunities: BreakOpportunity[]
  period: DailyCompliance
  previous: DailyCompliance | null
  change: number | null
  healthyDays: number
  eligibleDays: number
  streakDays: number
  insight: AnalyticsInsight
  insights: AnalyticsInsight[]
  routineScore: RoutineScore
  timeBlocks: PatternMetric[]
  weekdays: PatternMetric[]
  patterns: AnalyticsPattern[]
  milestones: AnalyticsMilestone[]
  averageResponseMinutes: number | null
  previousAverageResponseMinutes: number | null
  repeatedPostponeCompletion: number | null
  scheduleChanged: boolean
  retainedSince: string | null
}

const dayKey = (value: Date) =>
  `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-${String(value.getDate()).padStart(2, '0')}`

const dateAtNoon = (key: string) => new Date(`${key}T12:00:00`)

const daysBetween = (start: Date, end: Date) => {
  const cursor = new Date(start)
  cursor.setHours(12, 0, 0, 0)
  const last = new Date(end)
  last.setHours(12, 0, 0, 0)
  const keys: string[] = []
  while (cursor <= last) {
    keys.push(dayKey(cursor))
    cursor.setDate(cursor.getDate() + 1)
  }
  return keys
}

function emptyDay(key: string): DailyCompliance {
  return {
    key,
    due: 0,
    completed: 0,
    skipped: 0,
    missed: 0,
    postponed: 0,
    pending: 0,
    resolved: 0,
    percent: null,
  }
}

function aggregate(key: string, values: DailyCompliance[]): DailyCompliance {
  const total = values.reduce(
    (result, day) => ({
      due: result.due + day.due,
      completed: result.completed + day.completed,
      skipped: result.skipped + day.skipped,
      missed: result.missed + day.missed,
      postponed: result.postponed + day.postponed,
      pending: result.pending + day.pending,
      resolved: result.resolved + day.resolved,
    }),
    { due: 0, completed: 0, skipped: 0, missed: 0, postponed: 0, pending: 0, resolved: 0 }
  )
  return {
    key,
    ...total,
    percent: total.due === 0 ? null : Math.round((total.completed / total.due) * 100),
  }
}

/**
 * Reconstructs one user-facing opportunity from PausIO's lifecycle events.
 * Legacy records without an ID are paired conservatively in chronological
 * order. A manual break never enters the recommended-break denominator.
 */
export function reconstructOpportunities(
  events: HistoryEvent[],
  now = new Date()
): BreakOpportunity[] {
  const valid = events
    .filter((event) => !Number.isNaN(new Date(event.occurred_at).getTime()))
    .sort((left, right) => Date.parse(left.occurred_at) - Date.parse(right.occurred_at))
  const groups = new Map<string, HistoryEvent[]>()
  const legacyOpen: string[] = []
  const legacyClosed: string[] = []
  let legacySequence = 0

  for (const event of valid) {
    let id = event.break_id
    if (!id) {
      if (event.kind === 'due' || event.kind === 'deferred') {
        id = `legacy-${legacySequence++}`
        legacyOpen.push(id)
      } else if (event.kind === 'started') {
        id = legacyOpen.at(-1) ?? `legacy-${legacySequence++}`
        if (!legacyOpen.includes(id)) legacyOpen.push(id)
      } else if (event.kind === 'completed' || event.kind === 'skipped') {
        id = legacyOpen.shift() ?? `legacy-${legacySequence++}`
        legacyClosed.push(id)
      } else {
        id = legacyOpen.at(-1) ?? `legacy-${legacySequence++}`
      }
    }
    groups.set(id, [...(groups.get(id) ?? []), event])
  }

  const today = dayKey(now)
  return [...groups.entries()]
    .map(([id, lifecycle]): BreakOpportunity => {
      const due = lifecycle.find((event) => event.kind === 'due')
      const completed = [...lifecycle].reverse().find((event) => event.kind === 'completed')
      const endedEarly = [...lifecycle].reverse().find((event) => event.kind === 'skipped')
      const started = lifecycle.find((event) => event.kind === 'started')
      const last = lifecycle.at(-1)!
      // Schema v1 records had no IDs. Pair their due/outcome events by order;
      // a terminal record linked this way is still a recommended opportunity.
      const recommended = Boolean(due) || legacyClosed.includes(id)
      let status: OpportunityStatus
      if (completed) status = 'completed'
      else if (endedEarly) status = 'ended_early'
      else if (!recommended) status = 'manual'
      else if (last.kind === 'postponed') status = 'postponed'
      else if (dayKey(new Date(due!.occurred_at)) < today) status = 'missed'
      else status = 'pending'
      return {
        id,
        occurredAt: (due ?? lifecycle[0]).occurred_at,
        dueAt: due?.occurred_at ?? (recommended ? lifecycle[0].occurred_at : null),
        breakKind:
          due?.break_kind ?? lifecycle.find((event) => event.break_kind)?.break_kind ?? null,
        status,
        recommended,
        postpones: lifecycle.filter((event) => event.kind === 'postponed').length,
        contexts: lifecycle
          .filter((event): event is HistoryEvent & { context: ContextReason } =>
            Boolean(event.context)
          )
          .map((event) => event.context),
        startedAt: started?.occurred_at ?? null,
        completedAt: completed?.occurred_at ?? endedEarly?.occurred_at ?? null,
        targetBreakSeconds:
          due?.target_break_seconds ??
          lifecycle.find((event) => event.target_break_seconds)?.target_break_seconds ??
          null,
        workIntervalSeconds:
          due?.work_interval_seconds ??
          lifecycle.find((event) => event.work_interval_seconds)?.work_interval_seconds ??
          null,
        scheduleFingerprint:
          due?.schedule_fingerprint ??
          lifecycle.find((event) => event.schedule_fingerprint)?.schedule_fingerprint ??
          null,
        responseSeconds:
          due && started
            ? Math.max(
                0,
                Math.round((Date.parse(started.occurred_at) - Date.parse(due.occurred_at)) / 1000)
              )
            : null,
        events: lifecycle,
      }
    })
    .sort((left, right) => Date.parse(right.occurredAt) - Date.parse(left.occurredAt))
}

function dailyFromOpportunities(
  opportunities: BreakOpportunity[],
  keys: string[]
): DailyCompliance[] {
  const byDay = new Map(keys.map((key) => [key, emptyDay(key)]))
  for (const opportunity of opportunities) {
    if (!opportunity.recommended || !opportunity.dueAt) continue
    const key = dayKey(new Date(opportunity.dueAt))
    const day = byDay.get(key)
    if (!day) continue
    day.due += 1
    if (opportunity.status === 'completed') {
      day.completed += 1
      day.resolved += 1
    } else if (opportunity.status === 'ended_early') {
      day.skipped += 1
      day.resolved += 1
    } else if (opportunity.status === 'missed') {
      day.missed += 1
      day.resolved += 1
    } else {
      day.pending += 1
    }
    if (opportunity.postpones > 0) day.postponed += 1
  }
  return [...byDay.values()].map((day) => ({
    ...day,
    percent: day.due === 0 ? null : Math.round((day.completed / day.due) * 100),
  }))
}

function selectInsight(
  period: DailyCompliance,
  previous: DailyCompliance | null,
  change: number | null,
  days: DailyCompliance[]
): AnalyticsInsight {
  if (period.resolved < 3) return { tone: 'neutral', kind: 'first_steps', values: {} }
  if (change !== null && change >= 5)
    return {
      tone: 'positive',
      kind: 'improvement',
      values: { percent: period.percent ?? 0, change },
    }
  const eligible = days.filter((day) => day.resolved > 0)
  if (
    eligible.length >= 2 &&
    (eligible.at(-2)?.percent ?? 100) < 60 &&
    (eligible.at(-1)?.percent ?? 0) >= 80
  )
    return { tone: 'positive', kind: 'recovery', values: {} }
  if (period.postponed >= 3 && period.postponed / period.due >= 0.4)
    return {
      tone: 'opportunity',
      kind: 'postponement',
      values: { count: period.postponed },
    }
  if (
    eligible.length >= 4 &&
    eligible.slice(-5).filter((day) => (day.percent ?? 0) >= 80).length >= 4
  )
    return { tone: 'positive', kind: 'steady', values: { count: 4 } }
  return {
    tone: 'neutral',
    kind: 'summary',
    values: { completed: period.completed, due: period.due, percent: period.percent ?? 0 },
  }
}

const resolvedOpportunity = (opportunity: BreakOpportunity) =>
  opportunity.recommended && ['completed', 'ended_early', 'missed'].includes(opportunity.status)

const average = (values: number[]) =>
  values.length ? values.reduce((total, value) => total + value, 0) / values.length : null

function routineScore(
  opportunities: BreakOpportunity[],
  eligibleDays: number,
  healthyDays: number
): RoutineScore {
  const resolved = opportunities.filter(resolvedOpportunity)
  const sufficient = resolved.length >= 5 && eligibleDays >= 3
  if (!resolved.length)
    return {
      value: 0,
      followThrough: 0,
      timing: 0,
      consistency: 0,
      sufficient: false,
      resolved: 0,
      eligibleDays,
    }
  const followThrough =
    resolved.reduce((total, opportunity) => {
      if (opportunity.status === 'completed') return total + 1
      if (opportunity.status === 'ended_early') return total + 0.5
      return total
    }, 0) / resolved.length
  const timed = resolved.filter((opportunity) => opportunity.responseSeconds !== null)
  const timing = timed.length
    ? timed.reduce((total, opportunity) => {
        if (opportunity.status === 'missed') return total
        const interval = opportunity.workIntervalSeconds ?? 20 * 60
        const grace = Math.min(5 * 60, Math.round(interval * 0.25))
        const delay = opportunity.responseSeconds ?? interval
        if (delay <= grace) return total + 1
        return total + Math.max(0, 1 - (delay - grace) / interval)
      }, 0) / timed.length
    : followThrough
  const consistency = eligibleDays ? healthyDays / eligibleDays : 0
  const followThroughPoints = Math.round(followThrough * 70)
  const timingPoints = Math.round(timing * 20)
  const consistencyPoints = Math.round(consistency * 10)
  return {
    value: followThroughPoints + timingPoints + consistencyPoints,
    followThrough: followThroughPoints,
    timing: timingPoints,
    consistency: consistencyPoints,
    sufficient,
    resolved: resolved.length,
    eligibleDays,
  }
}

function metricGroups(
  opportunities: BreakOpportunity[],
  getKey: (opportunity: BreakOpportunity) => string
): PatternMetric[] {
  const values = new Map<string, { completed: number; resolved: number }>()
  for (const opportunity of opportunities.filter(resolvedOpportunity)) {
    const key = getKey(opportunity)
    const value = values.get(key) ?? { completed: 0, resolved: 0 }
    value.resolved += 1
    if (opportunity.status === 'completed') value.completed += 1
    values.set(key, value)
  }
  return [...values.entries()].map(([key, value]) => ({
    key,
    ...value,
    percent: value.resolved ? Math.round((value.completed / value.resolved) * 100) : null,
  }))
}

function timeBlock(opportunity: BreakOpportunity) {
  const hour = new Date(opportunity.dueAt ?? opportunity.occurredAt).getHours()
  if (hour < 12) return 'morning'
  if (hour < 17) return 'afternoon'
  return 'evening'
}

const weekday = (opportunity: BreakOpportunity) =>
  String(new Date(opportunity.dueAt ?? opportunity.occurredAt).getDay())

function detectPatterns(
  timeBlocks: PatternMetric[],
  weekdays: PatternMetric[],
  opportunities: BreakOpportunity[]
): AnalyticsPattern[] {
  const patterns: AnalyticsPattern[] = []
  const qualifiedBlocks = timeBlocks.filter(
    (metric) => metric.resolved >= 5 && metric.percent !== null
  )
  if (qualifiedBlocks.length >= 2) {
    const sorted = [...qualifiedBlocks].sort(
      (left, right) => (left.percent ?? 0) - (right.percent ?? 0)
    )
    const weakest = sorted[0]
    const strongest = sorted.at(-1)!
    if ((strongest.percent ?? 0) - (weakest.percent ?? 0) >= 20)
      patterns.push({
        kind: 'time_block',
        tone: 'opportunity',
        key: weakest.key,
        comparisonKey: strongest.key,
        percent: weakest.percent ?? 0,
        comparisonPercent: strongest.percent ?? 0,
        sample: weakest.resolved,
      })
  }
  const qualifiedWeekdays = weekdays.filter(
    (metric) => metric.resolved >= 3 && metric.percent !== null
  )
  if (qualifiedWeekdays.length >= 2) {
    const strongest = [...qualifiedWeekdays].sort(
      (left, right) => (right.percent ?? 0) - (left.percent ?? 0)
    )[0]
    patterns.push({
      kind: 'weekday',
      tone: 'positive',
      key: strongest.key,
      percent: strongest.percent ?? 0,
      sample: strongest.resolved,
    })
  }
  const postponed = opportunities.filter(
    (opportunity) => resolvedOpportunity(opportunity) && opportunity.postpones > 0
  )
  const repeated = postponed.filter((opportunity) => opportunity.postpones >= 2)
  if (repeated.length >= 5) {
    const repeatedPercent = Math.round(
      (repeated.filter((opportunity) => opportunity.status === 'completed').length /
        repeated.length) *
        100
    )
    patterns.push({
      kind: 'postponement',
      tone: repeatedPercent < 60 ? 'opportunity' : 'neutral',
      key: 'repeated',
      percent: repeatedPercent,
      sample: repeated.length,
    })
  }
  return patterns.slice(0, 3)
}

function detectMilestones(
  days: DailyCompliance[],
  healthyDays: number,
  period: DailyCompliance,
  previous: DailyCompliance | null
): AnalyticsMilestone[] {
  const milestones: AnalyticsMilestone[] = []
  if (period.resolved >= 5 && (period.percent ?? 0) >= 80)
    milestones.push({ kind: 'first_healthy_period', value: period.percent ?? 0 })
  for (const threshold of [5, 10, 20, 50, 100])
    if (healthyDays >= threshold) milestones.push({ kind: 'healthy_days', value: threshold })
  if (
    previous?.percent !== null &&
    previous?.percent !== undefined &&
    period.resolved >= 5 &&
    previous.resolved >= 5 &&
    (period.percent ?? 0) > previous.percent
  )
    milestones.push({ kind: 'personal_best', value: period.percent ?? 0 })
  const eligible = days.filter((day) => day.resolved > 0)
  if (
    eligible.length >= 2 &&
    (eligible.at(-2)?.percent ?? 100) < 60 &&
    (eligible.at(-1)?.percent ?? 0) >= 80
  )
    milestones.push({ kind: 'recovery', value: eligible.at(-1)?.percent ?? 0 })
  return milestones.slice(-3)
}

export function analyzeHistory(
  events: HistoryEvent[],
  now = new Date(),
  range: AnalyticsRange = 7
): HistoryAnalytics {
  const opportunities = reconstructOpportunities(events, now)
  const validDates = events
    .map((event) => new Date(event.occurred_at))
    .filter((date) => !Number.isNaN(date.getTime()))
  const earliest = validDates.length
    ? new Date(Math.min(...validDates.map((date) => date.getTime())))
    : new Date(now)
  const span = range === 'all' ? Math.max(1, daysBetween(earliest, now).length) : range
  const comparisonSpan = range === 'all' || range === 1 ? 0 : range
  const first = new Date(now)
  first.setHours(12, 0, 0, 0)
  first.setDate(first.getDate() - span + 1)
  const comparisonFirst = new Date(first)
  comparisonFirst.setDate(comparisonFirst.getDate() - comparisonSpan)
  const allKeys = daysBetween(comparisonFirst, now)
  const allDays = dailyFromOpportunities(opportunities, allKeys)
  const days = allDays.slice(-span)
  const previousDays = comparisonSpan ? allDays.slice(0, comparisonSpan) : []
  const period = aggregate('period', days)
  const previous = comparisonSpan ? aggregate('previous', previousDays) : null
  const change =
    period.resolved >= 5 && (previous?.resolved ?? 0) >= 5
      ? (period.percent ?? 0) - (previous?.percent ?? 0)
      : null
  const eligibleDays = days.filter((day) => day.resolved > 0)
  const healthyDays = eligibleDays.filter((day) => (day.percent ?? 0) >= 80).length
  let streakDays = 0
  for (const day of [...eligibleDays].reverse()) {
    if ((day.percent ?? 0) < 80) break
    streakDays += 1
  }
  const selectedOpportunities = opportunities.filter((opportunity) => {
    const date = new Date(opportunity.occurredAt)
    return date >= dateAtNoon(days[0]?.key ?? dayKey(now)) || dayKey(date) === days[0]?.key
  })
  const previousOpportunityStart = previousDays[0]?.key
  const previousOpportunityEnd = previousDays.at(-1)?.key
  const previousOpportunities = opportunities.filter((opportunity) => {
    const key = dayKey(new Date(opportunity.occurredAt))
    return Boolean(
      previousOpportunityStart &&
      previousOpportunityEnd &&
      key >= previousOpportunityStart &&
      key <= previousOpportunityEnd
    )
  })
  const timeBlocks = metricGroups(selectedOpportunities, timeBlock)
  const weekdays = metricGroups(selectedOpportunities, weekday)
  const patterns = detectPatterns(timeBlocks, weekdays, selectedOpportunities)
  const primaryInsight = selectInsight(period, previous, change, days)
  const responseMinutes = average(
    selectedOpportunities
      .filter(
        (opportunity) => resolvedOpportunity(opportunity) && opportunity.responseSeconds !== null
      )
      .map((opportunity) => (opportunity.responseSeconds ?? 0) / 60)
  )
  const previousResponseMinutes = average(
    previousOpportunities
      .filter(
        (opportunity) => resolvedOpportunity(opportunity) && opportunity.responseSeconds !== null
      )
      .map((opportunity) => (opportunity.responseSeconds ?? 0) / 60)
  )
  const extraInsights: AnalyticsInsight[] = []
  const firstPattern = patterns[0]
  if (firstPattern?.kind === 'time_block')
    extraInsights.push({
      tone: 'opportunity',
      kind: 'time_block',
      values: {
        percent: firstPattern.percent,
        comparisonPercent: firstPattern.comparisonPercent ?? 0,
      },
    })
  if (firstPattern?.kind === 'weekday')
    extraInsights.push({
      tone: 'positive',
      kind: 'weekday',
      values: { percent: firstPattern.percent, weekday: Number(firstPattern.key) },
    })
  if (
    responseMinutes !== null &&
    previousResponseMinutes !== null &&
    previousResponseMinutes - responseMinutes >= 2
  )
    extraInsights.push({
      tone: 'positive',
      kind: 'faster_response',
      values: { minutes: Math.round(previousResponseMinutes - responseMinutes) },
    })
  const fingerprints = new Set(
    selectedOpportunities.map((opportunity) => opportunity.scheduleFingerprint).filter(Boolean)
  )
  if (fingerprints.size > 1)
    extraInsights.push({ tone: 'neutral', kind: 'schedule_change', values: {} })
  const milestones = detectMilestones(days, healthyDays, period, previous)
  if (milestones.length)
    extraInsights.push({
      tone: 'positive',
      kind: 'milestone',
      values: { value: milestones.at(-1)?.value ?? 0 },
    })
  const repeated = selectedOpportunities.filter(
    (opportunity) => resolvedOpportunity(opportunity) && opportunity.postpones >= 2
  )
  const repeatedPostponeCompletion = repeated.length
    ? Math.round(
        (repeated.filter((opportunity) => opportunity.status === 'completed').length /
          repeated.length) *
          100
      )
    : null
  return {
    today: days.at(-1) ?? emptyDay(dayKey(now)),
    days,
    opportunities: selectedOpportunities,
    period,
    previous,
    change,
    healthyDays,
    eligibleDays: eligibleDays.length,
    streakDays,
    insight: primaryInsight,
    insights: [primaryInsight, ...extraInsights].slice(0, 3),
    routineScore: routineScore(selectedOpportunities, eligibleDays.length, healthyDays),
    timeBlocks,
    weekdays,
    patterns,
    milestones,
    averageResponseMinutes: responseMinutes,
    previousAverageResponseMinutes: previousResponseMinutes,
    repeatedPostponeCompletion,
    scheduleChanged: fingerprints.size > 1,
    retainedSince: validDates.length ? dayKey(earliest) : null,
  }
}

export type PeriodGranularity = 'day' | 'week' | 'month'

const weekKey = (value: Date) => {
  const start = new Date(value)
  start.setHours(12, 0, 0, 0)
  const mondayOffset = (start.getDay() + 6) % 7
  start.setDate(start.getDate() - mondayOffset)
  return dayKey(start)
}

const monthKey = (value: Date) =>
  `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}`

export function bucketByPeriod(
  days: DailyCompliance[],
  granularity: PeriodGranularity
): DailyCompliance[] {
  if (granularity === 'day') return days
  const order: string[] = []
  const grouped = new Map<string, DailyCompliance[]>()
  for (const day of days) {
    const date = dateAtNoon(day.key)
    const key = granularity === 'week' ? weekKey(date) : monthKey(date)
    if (!grouped.has(key)) order.push(key)
    grouped.set(key, [...(grouped.get(key) ?? []), day])
  }
  return order.map((key) => aggregate(key, grouped.get(key) ?? []))
}
