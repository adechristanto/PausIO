import { cleanup, fireEvent, render, screen } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import BreakPrompt from './BreakPrompt.svelte'
import type { Settings, Snapshot } from '../lib/types'

afterEach(cleanup)

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

const dueState: Snapshot = {
  phase: { break_due: { kind: 'short' } },
  remaining_seconds: 0,
  completed_short_breaks: 0,
  postpones_today: 0,
}

const renderPrompt = (overrides: Record<string, unknown> = {}) =>
  render(BreakPrompt, {
    state: dueState,
    settings,
    onStart: vi.fn(async () => {}),
    onPostpone: vi.fn(async () => {}),
    onPauseFor: vi.fn(async () => {}),
    ...overrides,
  })

describe('break decision prompt', () => {
  it('offers the exact short-break and two-minute choices', async () => {
    const onStart = vi.fn(async () => {})
    const onPostpone = vi.fn(async () => {})
    renderPrompt({ onStart, onPostpone })

    await fireEvent.click(screen.getByRole('button', { name: 'Start 20s break' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Postpone 2 min' }))
    expect(onStart).toHaveBeenCalledOnce()
    expect(onPostpone).toHaveBeenCalledOnce()
  })

  it('does not auto-focus any control when the prompt opens, so stray keypresses cannot trigger it', () => {
    const view = renderPrompt()

    expect(document.activeElement).not.toBe(view.container.querySelector('.button-primary'))
  })

  it('offers the long-break choice when a long break is due', () => {
    renderPrompt({
      state: { ...dueState, phase: { break_due: { kind: 'long' } } },
    })

    expect(screen.getByRole('button', { name: 'Start 5m break' })).toBeTruthy()
  })

  it('does not offer a postpone path for firm or strict delivery', () => {
    const view = renderPrompt({ settings: { ...settings, strictness: 'firm' } })
    expect(view.container.querySelector('.button-quiet')).not.toBeNull()
    expect(screen.queryByRole('button', { name: 'Postpone 2 min' })).toBeNull()
  })

  it('offers timed pauses from a dropdown beside the postpone button', async () => {
    const onPauseFor = vi.fn(async () => {})
    renderPrompt({ onPauseFor })

    const trigger = screen.getByRole('button', { name: 'Pause for…' })
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu')
    expect(trigger.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByRole('menu')).toBeNull()

    await fireEvent.click(trigger)
    expect(trigger.getAttribute('aria-expanded')).toBe('true')
    const menu = screen.getByRole('menu', { name: 'Pause for…' })
    expect(menu.contains(document.activeElement)).toBe(true)

    await fireEvent.click(screen.getByRole('menuitem', { name: 'Pause 1 hr' }))
    expect(onPauseFor).toHaveBeenCalledWith(60)
    expect(screen.queryByRole('menu')).toBeNull()
  })

  it.each([
    ['Pause 30 min', 30],
    ['Pause 1 hr', 60],
    ['Pause 2 hr', 120],
  ] as const)('maps %s to %s minutes', async (label, minutes) => {
    const onPauseFor = vi.fn(async () => {})
    renderPrompt({ onPauseFor })

    await fireEvent.click(screen.getByRole('button', { name: 'Pause for…' }))
    await fireEvent.click(screen.getByRole('menuitem', { name: label }))
    expect(onPauseFor).toHaveBeenCalledWith(minutes)
  })

  it('shows the timed-pause dropdown for firm delivery too', () => {
    renderPrompt({ settings: { ...settings, strictness: 'firm' } })

    expect(screen.getByRole('button', { name: 'Pause for…' })).toBeTruthy()
  })

  it('closes the menu on Escape and returns focus to the trigger', async () => {
    renderPrompt()

    const trigger = screen.getByRole('button', { name: 'Pause for…' })
    await fireEvent.click(trigger)
    expect(screen.getByRole('menu')).toBeTruthy()

    await fireEvent.keyDown(screen.getByRole('menu'), { key: 'Escape' })
    expect(screen.queryByRole('menu')).toBeNull()
    expect(document.activeElement).toBe(trigger)
  })

  it('closes the menu on an outside pointerdown', async () => {
    renderPrompt()

    await fireEvent.click(screen.getByRole('button', { name: 'Pause for…' }))
    expect(screen.getByRole('menu')).toBeTruthy()

    await fireEvent.pointerDown(document.body)
    expect(screen.queryByRole('menu')).toBeNull()
  })
})
