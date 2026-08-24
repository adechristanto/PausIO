<svelte:options runes={true} />

<script lang="ts">
  import { analyzeHistory, bucketByPeriod } from '../lib/history-analytics'
  import type {
    AnalyticsRange,
    BreakOpportunity,
    OpportunityStatus,
    PeriodGranularity,
  } from '../lib/history-analytics'
  import { t, tCount } from '../lib/i18n'
  import type { BreakKind, HistoryEvent, Settings } from '../lib/types'

  interface Props {
    history: HistoryEvent[]
    settings: Settings | null
    settingsRegion: HTMLElement | undefined
    historyRangeDays: AnalyticsRange
    historyClearConfirmation: boolean
    historyExport: string
    historyExportCopied: boolean
    deviceLabel: string
    onEnableHistory: () => void
    onReviewSettings: () => void
    clearHistory: () => Promise<void>
    exportHistory: (format: 'json' | 'csv') => Promise<void>
  }

  let {
    history,
    settings,
    settingsRegion = $bindable(),
    historyRangeDays = $bindable(),
    historyClearConfirmation,
    historyExport,
    historyExportCopied,
    deviceLabel,
    onEnableHistory,
    onReviewSettings,
    clearHistory,
    exportHistory,
  }: Props = $props()

  let outcomeFilter = $state<'all' | OpportunityStatus>('all')
  const appLocale = () => (settings?.locale === 'de' ? 'de-DE' : 'en-US')
  const now = $derived(new Date())
  const analytics = $derived(analyzeHistory(history, now, historyRangeDays))
  const granularity = $derived<PeriodGranularity>(
    historyRangeDays === 'all' || historyRangeDays > 30
      ? 'month'
      : historyRangeDays > 7
        ? 'week'
        : 'day'
  )
  const buckets = $derived(bucketByPeriod(analytics.days, granularity))
  const filteredOpportunities = $derived(
    analytics.opportunities.filter(
      (opportunity) => outcomeFilter === 'all' || opportunity.status === outcomeFilter
    )
  )
  const groupedOpportunities = $derived.by(() => {
    const groups = new Map<string, BreakOpportunity[]>()
    for (const opportunity of filteredOpportunities) {
      const key = localDayKey(opportunity.occurredAt)
      groups.set(key, [...(groups.get(key) ?? []), opportunity])
    }
    return [...groups.entries()]
  })

  const localDayKey = (value: string) => {
    const date = new Date(value)
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
  }
  const dateLabel = (key: string) =>
    new Intl.DateTimeFormat(appLocale(), {
      weekday: 'long',
      month: 'short',
      day: 'numeric',
    }).format(new Date(`${key}T12:00:00`))
  const timeLabel = (value: string) =>
    new Intl.DateTimeFormat(appLocale(), {
      hour: 'numeric',
      minute: '2-digit',
      hour12: appLocale() !== 'de-DE',
    }).format(new Date(value))
  const breakKindLabel = (kind: BreakKind | null) =>
    kind ? t(kind === 'long' ? 'break_kind_long' : 'break_kind_short') : ''
  const statusLabel = (status: OpportunityStatus) => {
    if (status === 'ended_early') return t('analytics_status_ended_early')
    return t(
      `analytics_status_${status}` as
        | 'analytics_status_completed'
        | 'analytics_status_missed'
        | 'analytics_status_postponed'
        | 'analytics_status_pending'
        | 'analytics_status_manual'
    )
  }
  const rangeLabel = (range: AnalyticsRange) => {
    if (range === 'all') return t('analytics_range_all')
    if (range === 1) return t('analytics_range_today')
    if (range === 7) return t('analytics_range_7')
    if (range === 30) return t('analytics_range_30')
    return t('analytics_range_90')
  }
  const bucketLabel = (key: string) => {
    if (granularity === 'month') {
      const [year, month] = key.split('-')
      return new Intl.DateTimeFormat(appLocale(), { month: 'short' }).format(
        new Date(Number(year), Number(month) - 1, 1)
      )
    }
    if (granularity === 'week')
      return new Intl.DateTimeFormat(appLocale(), { month: 'short', day: 'numeric' }).format(
        new Date(`${key}T12:00:00`)
      )
    return new Intl.DateTimeFormat(appLocale(), { weekday: 'narrow' }).format(
      new Date(`${key}T12:00:00`)
    )
  }
  const insightText = (insight = analytics.insight) => {
    if (insight.kind === 'improvement') return t('analytics_insight_improvement', insight.values)
    if (insight.kind === 'recovery') return t('analytics_insight_recovery')
    if (insight.kind === 'postponement') return t('analytics_insight_postponement', insight.values)
    if (insight.kind === 'steady') return t('analytics_insight_steady', insight.values)
    if (insight.kind === 'first_steps') return t('analytics_insight_first_steps')
    if (insight.kind === 'time_block') return t('analytics_insight_time_block', insight.values)
    if (insight.kind === 'weekday') return t('analytics_insight_weekday', insight.values)
    if (insight.kind === 'faster_response')
      return t('analytics_insight_faster_response', insight.values)
    if (insight.kind === 'schedule_change') return t('analytics_insight_schedule_change')
    if (insight.kind === 'milestone') return t('analytics_insight_milestone', insight.values)
    return t('analytics_insight_summary', insight.values)
  }
  const timeBlockLabel = (key: string) =>
    t(
      `analytics_time_${key}` as
        'analytics_time_morning' | 'analytics_time_afternoon' | 'analytics_time_evening'
    )
  const weekdayLabel = (key: string) =>
    new Intl.DateTimeFormat(appLocale(), { weekday: 'long' }).format(
      new Date(2026, 7, 2 + Number(key))
    )
  const patternTitle = (kind: 'time_block' | 'weekday' | 'postponement') => {
    if (kind === 'time_block') return t('analytics_pattern_time')
    if (kind === 'weekday') return t('analytics_pattern_weekday')
    return t('analytics_pattern_postponement')
  }
  const patternLabel = (kind: 'time_block' | 'weekday' | 'postponement', key: string) => {
    if (kind === 'time_block') return timeBlockLabel(key)
    if (kind === 'weekday') return weekdayLabel(key)
    return t('analytics_pattern_repeated')
  }
  const milestoneText = (
    kind: 'first_healthy_period' | 'healthy_days' | 'personal_best' | 'recovery',
    value: number
  ) => {
    if (kind === 'healthy_days') return t('analytics_milestone_healthy_days', { value })
    if (kind === 'personal_best') return t('analytics_milestone_personal_best', { value })
    if (kind === 'recovery') return t('analytics_milestone_recovery')
    return t('analytics_milestone_first_period', { value })
  }
  const daySummary = (opportunities: BreakOpportunity[]) => {
    const recommended = opportunities.filter((opportunity) => opportunity.recommended)
    const completed = recommended.filter((opportunity) => opportunity.status === 'completed').length
    const resolved = recommended.filter((opportunity) =>
      ['completed', 'ended_early', 'missed'].includes(opportunity.status)
    ).length
    return resolved ? t('analytics_day_summary', { completed, resolved }) : t('analytics_day_open')
  }
</script>

<section
  class="settings analytics"
  bind:this={settingsRegion}
  tabindex="-1"
  aria-labelledby="settings-title"
>
  <div class="analytics-toolbar">
    <p>{t('analytics_device_scope', { device: deviceLabel })}</p>
    <div class="analytics-ranges" role="group" aria-label={t('history_range')}>
      {#each [1, 7, 30, 90, 'all'] as range}
        <button
          class:active={historyRangeDays === range}
          aria-pressed={historyRangeDays === range}
          onclick={() => (historyRangeDays = range as AnalyticsRange)}
          >{rangeLabel(range as AnalyticsRange)}</button
        >
      {/each}
    </div>
  </div>

  {#if history.length === 0}
    <section class="analytics-empty-state">
      <span class="history-empty-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24"><path d="M4 18V9m6 9V5m6 13v-6m4 6H2" /></svg>
      </span>
      <h2>
        {settings?.history_enabled ? t('analytics_empty_title') : t('analytics_disabled_title')}
      </h2>
      <p>{settings?.history_enabled ? t('analytics_empty') : t('analytics_disabled')}</p>
      {#if settings && !settings.history_enabled}
        <button class="button button-primary" onclick={onEnableHistory}
          >{t('history_enable_action')}</button
        >
      {/if}
    </section>
  {:else}
    <section class="analytics-overview" aria-labelledby="analytics-overview-title">
      <div class="analytics-overview-main">
        <p class="analytics-eyebrow" id="analytics-overview-title">
          {t('analytics_follow_through')}
        </p>
        <div class="analytics-hero-value">
          <strong>{analytics.period.percent === null ? '—' : `${analytics.period.percent}%`}</strong
          >
          {#if analytics.change !== null}
            <span class:positive={analytics.change >= 0}
              >{analytics.change >= 0 ? '+' : ''}{analytics.change} {t('analytics_points')}</span
            >
          {/if}
        </div>
        <p>
          {t('analytics_completed_of_resolved', {
            completed: analytics.period.completed,
            resolved: analytics.period.resolved,
          })}
        </p>
        {#if analytics.change !== null}<small>{t('analytics_vs_previous')}</small>{/if}
      </div>

      <div class="analytics-today-card">
        <div class="analytics-today-copy">
          <p class="analytics-eyebrow">{t('history_today')}</p>
          <strong>{analytics.today.completed} <span>/ {analytics.today.due}</span></strong>
          <p>{t('analytics_due_so_far')}</p>
        </div>
        <div
          class="analytics-progress"
          role="progressbar"
          aria-label={t('analytics_today_progress')}
          aria-valuemin="0"
          aria-valuemax={Math.max(analytics.today.due, 1)}
          aria-valuenow={analytics.today.completed}
        >
          <span
            style={`width: ${analytics.today.due ? (analytics.today.completed / analytics.today.due) * 100 : 0}%`}
          ></span>
        </div>
        <div class="analytics-outcome-chips">
          {#if analytics.today.pending}<span
              >{t('analytics_pending_count', { count: analytics.today.pending })}</span
            >{/if}
          {#if analytics.today.postponed}<span
              >{t('analytics_postponed_count', { count: analytics.today.postponed })}</span
            >{/if}
          {#if analytics.today.skipped}<span
              >{t('analytics_early_count', { count: analytics.today.skipped })}</span
            >{/if}
        </div>
      </div>
    </section>

    <dl class="analytics-kpis">
      <div>
        <dt>{t('analytics_healthy_days')}</dt>
        <dd>{analytics.healthyDays}<span> / {analytics.eligibleDays}</span></dd>
      </div>
      <div>
        <dt>{t('analytics_current_run')}</dt>
        <dd>
          {analytics.streakDays}<span>{tCount('analytics_day_unit', analytics.streakDays)}</span>
        </dd>
      </div>
      <div>
        <dt>{t('analytics_postponed')}</dt>
        <dd>{analytics.period.postponed}<span>{t('analytics_opportunities')}</span></dd>
      </div>
    </dl>

    <section class="routine-score-card" aria-labelledby="routine-score-title">
      <div class="routine-score-summary">
        <p class="analytics-eyebrow" id="routine-score-title">{t('analytics_routine_score')}</p>
        {#if analytics.routineScore.sufficient}
          <strong>{analytics.routineScore.value}<span>/100</span></strong>
          <p>{t('analytics_routine_score_hint')}</p>
        {:else}
          <strong class="insufficient">—</strong>
          <p>
            {t('analytics_routine_score_insufficient', {
              resolved: analytics.routineScore.resolved,
            })}
          </p>
        {/if}
      </div>
      <dl class="routine-score-breakdown">
        <div>
          <dt>{t('analytics_score_follow')}</dt>
          <dd>{analytics.routineScore.followThrough}<span>/70</span></dd>
        </div>
        <div>
          <dt>{t('analytics_score_timing')}</dt>
          <dd>{analytics.routineScore.timing}<span>/20</span></dd>
        </div>
        <div>
          <dt>{t('analytics_score_consistency')}</dt>
          <dd>{analytics.routineScore.consistency}<span>/10</span></dd>
        </div>
      </dl>
    </section>

    <section
      class="analytics-insight {analytics.insight.tone}"
      aria-labelledby="analytics-insight-title"
    >
      <span aria-hidden="true">
        <svg viewBox="0 0 24 24"
          ><path d="M12 3a7 7 0 0 0-4 12.7V19h8v-3.3A7 7 0 0 0 12 3Z" /><path
            d="M9 22h6M9 16h6"
          /></svg
        >
      </span>
      <div>
        <p class="analytics-eyebrow" id="analytics-insight-title">
          {t('analytics_insight_heading')}
        </p>
        <strong>{insightText()}</strong>
        <p>{t('analytics_insight_evidence')}</p>
        {#if analytics.insight.kind === 'postponement' || analytics.insight.kind === 'time_block' || analytics.insight.kind === 'schedule_change'}
          <button class="analytics-insight-action" onclick={onReviewSettings}
            >{t('analytics_review_settings')}</button
          >
        {/if}
      </div>
    </section>

    {#if analytics.insights.length > 1}
      <details class="analytics-more-insights">
        <summary>{t('analytics_more_insights')}</summary>
        <ul>
          {#each analytics.insights.slice(1) as insight}
            <li class={insight.tone}>{insightText(insight)}</li>
          {/each}
        </ul>
      </details>
    {/if}

    <section class="analytics-section" aria-labelledby="analytics-trend-title">
      <div class="analytics-section-heading">
        <div>
          <h2 id="analytics-trend-title">{t('analytics_break_activity')}</h2>
          <p>{t('analytics_break_activity_hint')}</p>
        </div>
        <div class="analytics-legend" aria-hidden="true">
          <span class="complete">{t('analytics_status_completed')}</span>
          <span class="incomplete">{t('analytics_not_completed')}</span>
        </div>
      </div>
      <div class="analytics-chart" aria-hidden="true">
        {#each buckets as bucket}
          <div class="analytics-bar">
            <div title={`${bucket.completed}/${bucket.resolved}`}>
              {#if bucket.due}
                <span
                  class="chart-empty"
                  style={`height: ${Math.max(4, (bucket.pending / bucket.due) * 100)}%`}
                ></span>
                <span
                  class="chart-incomplete"
                  style={`height: ${((bucket.skipped + bucket.missed) / bucket.due) * 100}%`}
                ></span>
                <span
                  class="chart-complete"
                  style={`height: ${(bucket.completed / bucket.due) * 100}%`}
                ></span>
              {:else}<span class="chart-none"></span>{/if}
            </div>
            <small>{bucketLabel(bucket.key)}</small>
          </div>
        {/each}
      </div>
      <table class="sr-only">
        <caption>{t('analytics_break_activity')}</caption>
        <thead
          ><tr
            ><th>{t('history_chart_period')}</th><th>{t('analytics_status_completed')}</th><th
              >{t('analytics_not_completed')}</th
            ></tr
          ></thead
        >
        <tbody>
          {#each buckets as bucket}
            <tr
              ><th>{bucketLabel(bucket.key)}</th><td>{bucket.completed}</td><td
                >{bucket.skipped + bucket.missed + bucket.pending}</td
              ></tr
            >
          {/each}
        </tbody>
      </table>
    </section>

    <section class="analytics-section" aria-labelledby="analytics-patterns-title">
      <div class="analytics-section-heading">
        <div>
          <h2 id="analytics-patterns-title">{t('analytics_patterns')}</h2>
          <p>{t('analytics_patterns_hint')}</p>
        </div>
      </div>
      {#if analytics.patterns.length}
        <div class="analytics-patterns">
          {#each analytics.patterns as pattern}
            <article>
              <p>{patternTitle(pattern.kind)}</p>
              <div>
                <strong>{patternLabel(pattern.kind, pattern.key)}</strong><span
                  >{pattern.percent}%</span
                >
              </div>
              <div class="pattern-meter" aria-hidden="true">
                <span style={`width: ${pattern.percent}%`}></span>
              </div>
              <small>{t('analytics_pattern_sample', { count: pattern.sample })}</small>
            </article>
          {/each}
        </div>
      {:else}
        <p class="analytics-filter-empty">{t('analytics_patterns_insufficient')}</p>
      {/if}
    </section>

    {#if analytics.milestones.length}
      <section class="analytics-section" aria-labelledby="analytics-milestones-title">
        <div class="analytics-section-heading">
          <div>
            <h2 id="analytics-milestones-title">{t('analytics_progress')}</h2>
            <p>{t('analytics_progress_hint')}</p>
          </div>
        </div>
        <div class="analytics-milestones">
          {#each analytics.milestones as milestone}
            <article>
              <span aria-hidden="true">✓</span><strong
                >{milestoneText(milestone.kind, milestone.value)}</strong
              >
            </article>
          {/each}
        </div>
      </section>
    {/if}

    <section class="analytics-section activity-section" aria-labelledby="analytics-activity-title">
      <div class="analytics-section-heading activity-heading">
        <div>
          <h2 id="analytics-activity-title">{t('analytics_activity')}</h2>
          <p>{t('analytics_activity_hint')}</p>
        </div>
        <select bind:value={outcomeFilter} aria-label={t('analytics_filter_outcome')}>
          <option value="all">{t('analytics_filter_all')}</option>
          <option value="completed">{t('analytics_status_completed')}</option>
          <option value="ended_early">{t('analytics_status_ended_early')}</option>
          <option value="missed">{t('analytics_status_missed')}</option>
          <option value="postponed">{t('analytics_status_postponed')}</option>
          <option value="pending">{t('analytics_status_pending')}</option>
          <option value="manual">{t('analytics_status_manual')}</option>
        </select>
      </div>
      {#if groupedOpportunities.length === 0}
        <p class="analytics-filter-empty">{t('analytics_filter_empty')}</p>
      {:else}
        <div class="activity-days">
          {#each groupedOpportunities as [key, opportunities]}
            <section class="activity-day">
              <header>
                <h3>{dateLabel(key)}</h3>
                <span>{daySummary(opportunities)}</span>
              </header>
              <ul>
                {#each opportunities as opportunity}
                  <li class="status-{opportunity.status}">
                    <span class="activity-marker" aria-hidden="true"></span>
                    <div>
                      <strong>{statusLabel(opportunity.status)}</strong>
                      <span>
                        {timeLabel(opportunity.occurredAt)}
                        {#if opportunity.breakKind}
                          · {breakKindLabel(opportunity.breakKind)}{/if}
                        {#if opportunity.postpones}
                          · {t('analytics_postponed_times', { count: opportunity.postpones })}{/if}
                      </span>
                    </div>
                    {#if opportunity.contexts.length}<small
                        >{t('analytics_context_respected')}</small
                      >{/if}
                  </li>
                {/each}
              </ul>
            </section>
          {/each}
        </div>
      {/if}
    </section>

    <footer class="analytics-data-footer">
      <p>
        {analytics.retainedSince
          ? t('analytics_retained_since', { date: dateLabel(analytics.retainedSince) })
          : t('analytics_local_only')}
      </p>
      <div class="history-actions">
        <button class="button button-secondary" onclick={() => exportHistory('csv')}
          >{t('history_export_csv')}</button
        >
        <button class="button button-secondary" onclick={() => exportHistory('json')}
          >{t('history_export_json')}</button
        >
        <button class="button button-danger history-clear" onclick={clearHistory}
          >{historyClearConfirmation ? t('history_clear_confirm') : t('history_clear')}</button
        >
      </div>
      {#if historyExport}
        <textarea
          class="health-report"
          readonly
          aria-label={t('history_export_json')}
          value={historyExport}></textarea>
        {#if historyExportCopied}<p class="setting-note">{t('history_export_copied')}</p>{/if}
      {/if}
    </footer>
  {/if}
</section>
