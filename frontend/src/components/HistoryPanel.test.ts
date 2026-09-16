import { cleanup, render, screen } from '@testing-library/svelte'
import { afterEach, describe, expect, it } from 'vitest'
import HistoryPanel from './HistoryPanel.svelte'
import type { HistoryEvent, Settings } from '../lib/types'

const settings: Settings = {
  work_seconds: 1200,
  short_break_seconds: 20,
  long_break_seconds: 300,
  long_break_every: 4,
  pre_break_seconds: 30,
  active_days_mask: 0b0111110,
  active_start_minutes: 540,
  active_end_minutes: 1080,
  postpone_limit: null,
}

const at = (day: string, kind: HistoryEvent['kind'], breakId: string): HistoryEvent => ({
  occurred_at: `${day}T12:00:00Z`,
  kind,
  break_id: breakId,
})

// Enough resolved opportunities + eligible days for the Routine Score's own
// data-sufficiency gate to be satisfied, so the show_routine_score setting is
// the only thing left deciding whether the section renders.
const sufficientHistory: HistoryEvent[] = []
for (const day of ['2026-07-20', '2026-07-21', '2026-07-22']) {
  for (let i = 0; i < 2; i += 1) {
    const id = `${day}-${i}`
    sufficientHistory.push(at(day, 'due', id))
    sufficientHistory.push(at(day, 'completed', id))
  }
}

const noop = () => {}
const noopAsync = async () => {}

const baseProps = {
  settingsRegion: undefined,
  historyRangeDays: 30 as const,
  historyClearConfirmation: false,
  historyExport: '',
  historyExportCopied: false,
  deviceLabel: 'This Mac',
  onEnableHistory: noop,
  onReviewSettings: noop,
  clearHistory: noopAsync,
  exportHistory: async () => {},
}

afterEach(cleanup)

describe('HistoryPanel', () => {
  it('agrees between the hero percent and the "N of M resolved" sentence, using resolved (not due) as the denominator', () => {
    // HistoryPanel derives "now" internally from the real clock, so these events use
    // today's actual date -- a fixed past date would fall outside the 'all' window's
    // period comparison and read as 0 of 0.
    const today = new Date().toISOString().slice(0, 10)
    const history: HistoryEvent[] = [
      at(today, 'due', 'resolved-1'),
      at(today, 'completed', 'resolved-1'),
      at(today, 'due', 'pending-1'),
      at(today, 'due', 'pending-2'),
      at(today, 'due', 'pending-3'),
    ]
    render(HistoryPanel, { ...baseProps, history, settings, historyRangeDays: 'all' })

    // 1 of 1 resolved breaks completed -- hero percent must read 100%, not 25%
    // (which is what completed/due would say with 3 still-pending breaks).
    expect(screen.getByText('100%')).toBeTruthy()
    expect(screen.getByText('1 of 1 resolved breaks completed')).toBeTruthy()
  })

  it('hides the Routine Score section by default even with sufficient data', () => {
    render(HistoryPanel, {
      ...baseProps,
      history: sufficientHistory,
      settings: { ...settings, show_routine_score: false },
    })
    expect(screen.queryByText('Routine Score')).toBeNull()
  })

  it('shows the Routine Score section once explicitly opted in via settings', () => {
    render(HistoryPanel, {
      ...baseProps,
      history: sufficientHistory,
      settings: { ...settings, show_routine_score: true },
    })
    expect(screen.getByText('Routine Score')).toBeTruthy()
  })
})
