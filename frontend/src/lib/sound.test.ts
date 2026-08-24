import { afterEach, describe, expect, it, vi } from 'vitest'
import { playBreakSound } from './sound'

class FakeGain {
  gain = { setValueAtTime: vi.fn(), linearRampToValueAtTime: vi.fn() }
  connect = vi.fn()
}
class FakeOscillator {
  type = ''
  frequency = { value: 0 }
  connect = vi.fn()
  start = vi.fn()
  stop = vi.fn()
}
class FakeAudioContext {
  currentTime = 0
  destination = {}
  oscillators: FakeOscillator[] = []
  createGain() {
    return new FakeGain()
  }
  createOscillator() {
    const oscillator = new FakeOscillator()
    this.oscillators.push(oscillator)
    return oscillator
  }
  resume = vi.fn(async () => {})
}

describe('playBreakSound', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
  })

  it('never touches the audio context when the theme is silence', () => {
    const fake = new FakeAudioContext()
    vi.stubGlobal(
      'AudioContext',
      vi.fn(() => fake)
    )
    playBreakSound('silence', 100, 'start')
    expect(fake.oscillators).toHaveLength(0)
  })

  it('never touches the audio context when volume is zero', () => {
    const fake = new FakeAudioContext()
    vi.stubGlobal(
      'AudioContext',
      vi.fn(() => fake)
    )
    playBreakSound('chime', 0, 'start')
    expect(fake.oscillators).toHaveLength(0)
  })

  it('plays a two-note chime, a single-note tone, and a single-note click', () => {
    const fake = new FakeAudioContext()
    vi.stubGlobal(
      'AudioContext',
      vi.fn(() => fake)
    )
    playBreakSound('chime', 80, 'start')
    expect(fake.oscillators).toHaveLength(2)
    fake.oscillators.length = 0
    playBreakSound('tone', 80, 'end')
    expect(fake.oscillators).toHaveLength(1)
    fake.oscillators.length = 0
    playBreakSound('click', 80, 'end')
    expect(fake.oscillators).toHaveLength(1)
  })

  it('does not throw when Web Audio is unavailable (e.g. this jsdom test environment)', () => {
    expect(() => playBreakSound('chime', 80, 'start')).not.toThrow()
  })

  it('leaves break audio to the native shell inside the macOS Tauri webview', () => {
    const fake = new FakeAudioContext()
    vi.stubGlobal(
      'AudioContext',
      vi.fn(() => fake)
    )
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    })

    playBreakSound('tone', 70, 'start')

    expect(fake.oscillators).toHaveLength(0)
  })

  it('leaves break audio to the native shell inside the Windows Tauri webview', () => {
    const fake = new FakeAudioContext()
    vi.stubGlobal(
      'AudioContext',
      vi.fn(() => fake)
    )
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' })
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: {},
    })

    playBreakSound('tone', 70, 'end')

    expect(fake.oscillators).toHaveLength(0)
  })
})
