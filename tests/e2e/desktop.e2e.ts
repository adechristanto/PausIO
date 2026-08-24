import { $, browser, expect } from '@wdio/globals'

type EngineSnapshot = {
  phase: unknown
  remaining_seconds: number
  completed_short_breaks: number
}

async function invoke<T>(command: string, payload: Record<string, unknown> = {}): Promise<T> {
  return browser.execute(
    (name: string, args: Record<string, unknown>) => {
      const tauri = (
        window as Window & {
          __TAURI__: {
            core: {
              invoke: (command: string, payload: Record<string, unknown>) => Promise<unknown>
            }
          }
        }
      ).__TAURI__
      return tauri.core.invoke(name, args)
    },
    command,
    payload
  ) as Promise<T>
}

describe('PausIO desktop vertical slice', () => {
  it('shows the dashboard and persists a settings change in the isolated E2E store', async () => {
    await expect($('.app-shell')).toBeDisplayed()
    await expect($('h1')).toHaveText('Next eye break')
    await $('button[aria-label="Settings"]').click()
    // WebDriver's keyboard-oriented `setValue` appends to a range's current
    // value, which clamps it at the HTML max. Drive the same input event a
    // pointer interaction produces so this verifies the actual Svelte binding.
    await browser.execute((value: string) => {
      const interval = document.querySelector<HTMLInputElement>('input[type="range"]')
      if (!interval) throw new Error('work interval range is missing')
      interval.value = value
      interval.dispatchEvent(new Event('input', { bubbles: true }))
    }, '900')
    // Settings auto-saves on a short debounce; there is no explicit save action.
    await browser.waitUntil(
      async () =>
        browser.execute(
          () => document.querySelector('.settings-status')?.textContent?.includes('Saved') ?? false
        ),
      { timeoutMsg: 'expected the auto-save confirmation' }
    )
    await browser.refresh()
    await $('button[aria-label="Settings"]').click()
    await expect($('input[type="range"]')).toHaveValue('900')
  })

  it('starts a dormant session and pauses without entering a break', async () => {
    await $('button[aria-label="Done"]').click()
    const start = await $('button=Start session')
    if (await start.isExisting()) {
      await start.click()
    }
    await $('button[aria-label="Pause timer"]').click()
    await expect($('button[aria-label="Resume"]')).toBeDisplayed()
    await expect($('.phase-status')).toHaveText(expect.stringContaining('Paused'))
    await expect($('.break-overlay')).not.toExist()
    await $('button[aria-label="Resume"]').click()
    await expect($('button[aria-label="Pause timer"]')).toBeDisplayed()
  })

  it('starts and ends Break now without blocking the native event loop', async () => {
    const started = await invoke<EngineSnapshot>('take_break_now')
    expect(started.phase).toEqual({ breaking: { kind: 'short' } })

    const ended = await invoke<EngineSnapshot>('skip_break')
    expect(ended.phase).toEqual('working')
  })

  it('does not expose mobile-only wearable commands', async () => {
    const failure = await browser.execute(async () => {
      try {
        await (
          window as Window & { __TAURI__: { core: { invoke: (command: string) => Promise<void> } } }
        ).__TAURI__.core.invoke('get_watch_status')
        return ''
      } catch (error) {
        return String(error)
      }
    })

    expect(failure).toMatch(/get_watch_status|not found|unknown/i)
  })

  it('subtracts locked time once and starts fresh when the work interval is exhausted', async () => {
    const before = await invoke<EngineSnapshot>('get_state')
    const shortLock = await invoke<EngineSnapshot>('e2e_simulate_screen_lock', {
      lockedSeconds: 19,
    })
    const expectedRemaining = before.remaining_seconds - 19
    expect(shortLock.remaining_seconds).toBeGreaterThanOrEqual(expectedRemaining - 1)
    expect(shortLock.remaining_seconds).toBeLessThanOrEqual(expectedRemaining + 1)
    expect(shortLock.phase).toEqual(before.phase)
    expect(shortLock.completed_short_breaks).toBe(before.completed_short_breaks)

    const settings = await invoke<{ work_seconds: number }>('get_settings')
    const recovered = await invoke<EngineSnapshot>('e2e_simulate_screen_lock', {
      lockedSeconds: shortLock.remaining_seconds,
    })
    expect(recovered.remaining_seconds).toBe(settings.work_seconds)
    expect(recovered.phase).toEqual('working')
    expect(recovered.completed_short_breaks).toBe(before.completed_short_breaks)
    // The timer keeps ticking while assertions run, so an exact countdown
    // read races the 1s tick. Pause first, then compare the frozen display
    // against the paused snapshot.
    const paused = await invoke<EngineSnapshot>('pause')
    const pausedClock = `${String(Math.floor(paused.remaining_seconds / 60)).padStart(2, '0')}:${String(paused.remaining_seconds % 60).padStart(2, '0')}`
    await expect($('.horizon-timer strong')).toHaveText(pausedClock)
  })

  it('exposes opt-in login startup in desktop settings without changing the real login item', async () => {
    await $('button[aria-label="Settings"]').click()
    // Login startup lives in the System settings sub-category, not the default landing page.
    await $('button=System').click()
    await expect($('input[aria-label="Start PausIO at login"]')).toBeDisplayed()
    await $('button[aria-label="Done"]').click()
  })

  it('queues the configured system sound through the native macOS player', async () => {
    const isMacOS = await browser.execute(() => /Macintosh|Mac OS X/.test(navigator.userAgent))
    if (!isMacOS) return

    // A native lookup/playback failure is returned through IPC and fails the test.
    await invoke<void>('preview_system_sound', { sound: 'chime' })
  })

  // The embedded WebDriver protocol cannot reliably enumerate secondary macOS
  // WebKit windows, locally or on GitHub runners. Keep this scenario explicit
  // for manual desktop validation until the driver supports multi-window Tauri.
  it.skip('opens a forced break overlay on each monitor; the short break stays dismissible', async () => {
    const dashboard = await browser.getWindowHandle()
    await $('button=Take a break now').click()
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
      timeoutMsg: 'expected the break overlay window',
    })
    const overlay = (await browser.getWindowHandles()).find((handle) => handle !== dashboard)
    if (!overlay) throw new Error('break overlay did not receive a window handle')
    await browser.switchToWindow(overlay)
    await expect($('h1')).toHaveText('Look somewhere far away.')
    // The default short break stays dismissible; only the long break is a strict,
    // non-dismissible shield (see the manual multi-monitor validation notes in the plan).
    await $('button=I’m back').click()
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1, {
      timeoutMsg: 'expected completing the break to close every overlay',
    })
    await browser.switchToWindow(dashboard)
    await expect($('.horizon-timer strong')).toHaveText('15:00')
  })
})
