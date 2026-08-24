import { cleanup, render } from '@testing-library/svelte'
import { afterEach, describe, expect, it } from 'vitest'
import TimerRing from './TimerRing.svelte'

afterEach(cleanup)

describe('signature timer dial', () => {
  it('uses an isolated SVG class instead of Tailwind’s rectangular ring utility', () => {
    const { container } = render(TimerRing, {
      fraction: 0.5,
      clock: '12:00',
      subLabel: 'until your next break',
    })

    const dial = container.querySelector('svg')
    expect(dial?.classList.contains('timer-ring')).toBe(true)
    expect(dial?.classList.contains('ring')).toBe(false)
  })

  it('drains from a full ring to an empty ring as remaining time decreases', () => {
    const dashOffset = (fraction: number) => {
      const { container } = render(TimerRing, {
        fraction,
        clock: '12:00',
        subLabel: 'until your next break',
      })
      return Number(container.querySelector('.ring-fill')?.getAttribute('stroke-dashoffset'))
    }

    const circumference = 2 * Math.PI * 45
    expect(dashOffset(1)).toBeCloseTo(0)
    expect(dashOffset(0.5)).toBeCloseTo(circumference / 2)
    expect(dashOffset(0)).toBeCloseTo(circumference)
  })
})
