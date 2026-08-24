import type { SoundTheme } from './types'

/** 'start'/'end' of a break — the only two moments PausIO ever plays a cue for. */
export type SoundMoment = 'start' | 'end'

let sharedContext: AudioContext | null = null

function audioContext(): AudioContext | null {
  if (typeof window === 'undefined' || typeof window.AudioContext === 'undefined') return null
  if (!sharedContext) sharedContext = new AudioContext()
  return sharedContext
}

function handledByNativeMacOS(): boolean {
  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in window &&
    typeof navigator !== 'undefined' &&
    /Macintosh|Mac OS X/.test(navigator.userAgent)
  )
}

function handledByNativeWindows(): boolean {
  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in window &&
    typeof navigator !== 'undefined' &&
    /Windows/.test(navigator.userAgent)
  )
}

function tone(
  ctx: AudioContext,
  frequency: number,
  startAt: number,
  duration: number,
  peakGain: number
) {
  const oscillator = ctx.createOscillator()
  const gain = ctx.createGain()
  oscillator.type = 'sine'
  oscillator.frequency.value = frequency
  gain.gain.setValueAtTime(0, startAt)
  gain.gain.linearRampToValueAtTime(peakGain, startAt + 0.02)
  gain.gain.linearRampToValueAtTime(0, startAt + duration)
  oscillator.connect(gain)
  gain.connect(ctx.destination)
  oscillator.start(startAt)
  oscillator.stop(startAt + duration + 0.02)
}

/**
 * Plays a short, synthesized audio cue for a break's start or end. Every
 * theme is generated on the fly — no bundled audio files, nothing to license
 * or fail to load. Silently does nothing if the theme is `silence`, if Web
 * Audio is unavailable, or if the platform blocks audio without a prior user
 * gesture: a missed chime is never worth surfacing an error for.
 */
export function playBreakSound(
  theme: SoundTheme,
  volumePercent: number,
  moment: SoundMoment
): void {
  // A hidden WKWebView can be suspended while PausIO lives in the menu bar.
  // The Rust shell owns these cues on macOS and Windows natively; retaining
  // Web Audio here would make a visible-window break play twice while a
  // tray-only break stays mute.
  if (theme === 'silence' || handledByNativeMacOS() || handledByNativeWindows()) return
  // Kept deliberately gentle: even at 100% configured volume this tops out
  // well below a jarring system alert.
  const peakGain = Math.max(0, Math.min(1, volumePercent / 100)) * 0.2
  if (peakGain <= 0) return
  const ctx = audioContext()
  if (!ctx) return
  const now = ctx.currentTime
  try {
    if (theme === 'chime') {
      const [first, second] = moment === 'start' ? [660, 880] : [880, 660]
      tone(ctx, first, now, 0.18, peakGain)
      tone(ctx, second, now + 0.16, 0.22, peakGain)
    } else if (theme === 'tone') {
      tone(ctx, moment === 'start' ? 523 : 392, now, 0.3, peakGain)
    } else if (theme === 'click') {
      tone(ctx, moment === 'start' ? 1400 : 900, now, 0.05, peakGain)
    }
    void ctx.resume()
  } catch {
    // Autoplay policy or an unsupported context state: a missed cue is not
    // worth surfacing to the person taking a break.
  }
}
