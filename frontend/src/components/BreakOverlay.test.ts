import { cleanup, fireEvent, render, screen } from '@testing-library/svelte'
import { afterEach, describe, expect, it, vi } from 'vitest'
import BreakOverlay from './BreakOverlay.svelte'
import type { Settings, Snapshot } from '../lib/types'

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

afterEach(cleanup)

describe('break overlay', () => {
  it('offers "I’m back" on the primary short break, and it ends the break', async () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    const onDone = vi.fn(async () => {})
    render(BreakOverlay, { state, settings, primary: true, onDone })

    const button = screen.getByRole('button', { name: 'I’m back' })
    await fireEvent.click(button)
    expect(onDone).toHaveBeenCalledOnce()
  })

  it('focuses the primary overlay for assistive technology', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    const view = render(BreakOverlay, { state, settings, primary: true })

    expect(document.activeElement).toBe(view.container.querySelector('.break-overlay'))
  })

  it('keeps an explicit emergency exit on a strict long break', async () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'long' } },
      remaining_seconds: 250,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    const onDone = vi.fn(async () => {})
    render(BreakOverlay, {
      state,
      settings: { ...settings, strictness: 'strict' },
      primary: true,
      onDone,
    })
    const emergency = screen.getByRole('button', { name: 'End break early (emergency)' })
    await fireEvent.click(emergency)
    expect(onDone).toHaveBeenCalledOnce()
  })

  it('does not offer a dismiss control on a non-primary (secondary monitor) short-break overlay', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state, settings, primary: false })
    expect(screen.queryAllByRole('button')).toHaveLength(0)
  })

  it('shows the short-break heading and clock for a short break', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state, settings, primary: true })
    expect(screen.getByRole('heading', { name: 'Look somewhere far away.' })).toBeTruthy()
    expect(document.querySelector('.horizon-timer strong')?.textContent).toBe('00:14')
    expect(screen.getByText('Blink slowly five times.')).toBeTruthy()
  })

  it('shows the long-break heading for a long break', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'long' } },
      remaining_seconds: 250,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state, settings, primary: true })
    expect(screen.getByRole('heading', { name: 'Step away for a moment.' })).toBeTruthy()
  })

  it('advances the guided reset throughout a whole long break, instead of freezing on step 4 after 20s', () => {
    // 300s long break: at 20s elapsed the fixed-5s-per-step cadence this replaces would
    // already be on the last step, frozen there for the remaining 4m40s. Proportional
    // pacing (300s / 4 steps = 75s/step) should still be on step 1.
    const early: Snapshot = {
      phase: { breaking: { kind: 'long' } },
      remaining_seconds: 280,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state: early, settings, primary: true })
    expect(screen.getByText('Gentle reset · step 1 of 4')).toBeTruthy()
    cleanup()

    // Halfway through (150s elapsed of 300s) should be on step 3 (150 / 75 = 2 -> index 2).
    const mid: Snapshot = {
      phase: { breaking: { kind: 'long' } },
      remaining_seconds: 150,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state: mid, settings, primary: true })
    expect(screen.getByText('Gentle reset · step 3 of 4')).toBeTruthy()
    cleanup()

    // Near the very end should be on the final step, not stuck there since 20s in.
    const late: Snapshot = {
      phase: { breaking: { kind: 'long' } },
      remaining_seconds: 10,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state: late, settings, primary: true })
    expect(screen.getByText('Gentle reset · step 4 of 4')).toBeTruthy()
  })

  it('fades in on mount rather than slamming to near-black instantly', async () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state, settings, primary: true })
    const main = document.querySelector('.break-overlay') as HTMLElement
    // onMount runs synchronously after the initial render in this test setup, so by the time
    // the component is queryable it has already flipped to entered — the class itself, not the
    // timing, is what the CSS `.break-overlay { opacity: 0 } .entered { opacity: 1 }` rule and
    // its Reduce-Motion duration override key off of.
    expect(main.className).toContain('entered')
  })

  it('renders a locally configured calm message during a break', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 20,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, {
      state,
      settings: { ...settings, break_messages: ['Relax your shoulders.'] },
      primary: true,
    })
    expect(screen.getByText('Relax your shoulders.')).toBeTruthy()
  })

  // The break itself is the subject of this screen; a brand mark on it is just one more
  // thing for a resting eye to land on.
  it('carries no brand mark, so the shield stays down to phase, countdown and guidance', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 20,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    const view = render(BreakOverlay, { state, settings, primary: true })
    expect(view.container.querySelector('img')).toBeNull()
  })

  it('does not carry an announcement or error on a non-primary (secondary monitor) overlay', () => {
    const state: Snapshot = {
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 14,
      completed_short_breaks: 0,
      postpones_today: 0,
    }
    render(BreakOverlay, { state, settings, error: 'boom', primary: false })
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByRole('status')).toBeNull()
  })
})
