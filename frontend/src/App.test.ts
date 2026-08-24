import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App.svelte'
import type { AutostartStatus, Settings, Snapshot } from './lib/types'

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

const snapshot: Snapshot = {
  phase: 'working',
  remaining_seconds: 1188,
  completed_short_breaks: 2,
  postpones_today: 1,
}

const apiMock = vi.hoisted(() => ({
  getState: vi.fn<() => Promise<Snapshot>>(),
  getSettings: vi.fn<() => Promise<Settings>>(),
  getSettingsProfiles: vi.fn(),
  getOnboardingState: vi.fn<() => Promise<boolean>>(),
  completeOnboarding: vi.fn<() => Promise<void>>(),
  saveSettingsProfile: vi.fn(),
  applySettingsProfile: vi.fn(),
  setSettings: vi.fn<(next: Settings) => Promise<Settings>>(),
  setContext: vi.fn<() => Promise<Snapshot>>(),
  getHistory: vi.fn(),
  clearHistory: vi.fn(),
  exportHistory: vi.fn(),
  resetLocalData: vi.fn<() => Promise<Snapshot>>(),
  startSession: vi.fn<() => Promise<Snapshot>>(),
  startDueBreak: vi.fn<() => Promise<Snapshot>>(),
  pause: vi.fn<() => Promise<Snapshot>>(),
  pauseForMinutes: vi.fn<(minutes: number) => Promise<Snapshot>>(),
  resume: vi.fn<() => Promise<Snapshot>>(),
  takeBreakNow: vi.fn<() => Promise<Snapshot>>(),
  skipBreak: vi.fn<() => Promise<Snapshot>>(),
  postponeBreak: vi.fn<() => Promise<Snapshot>>(),
  syncWatchSettings: vi.fn<() => Promise<never>>(),
  sendTestNudge: vi.fn<() => Promise<'delivered'>>(),
  getWatchStatus: vi.fn(),
  getAutostartStatus: vi.fn<() => Promise<AutostartStatus>>(),
  setAutostartEnabled: vi.fn<(enabled: boolean) => Promise<AutostartStatus>>(),
  getDesktopHealth: vi.fn(),
  getHealthReport: vi.fn(),
  testReminder: vi.fn(),
  onTick: vi.fn(),
  onState: vi.fn(),
  onBreakEnded: vi.fn(),
  onBlinkNudge: vi.fn(),
  onPostureNudge: vi.fn(),
  onHydrationNudge: vi.fn(),
}))

vi.mock('./lib/pausio', () => ({ api: apiMock }))

// The clock digits and colon render as separate spans (for spacing), so their
// text is split across sibling nodes and getByText's own-text-node matching
// can't see it as one string — match on the full recursive textContent instead.
const findClock = (clock: string) =>
  screen.findByText((_, element) => element?.textContent === clock && element?.tagName === 'STRONG')

describe('Quiet Horizon app experience', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    apiMock.getState.mockResolvedValue({ ...snapshot })
    apiMock.getSettings.mockResolvedValue({ ...settings })
    apiMock.getSettingsProfiles.mockResolvedValue({})
    // Onboarding is a one-time first-run flow; every existing test exercises the
    // app as it behaves after that flow, so the default here is "already seen".
    apiMock.getOnboardingState.mockResolvedValue(true)
    apiMock.completeOnboarding.mockResolvedValue(undefined)
    apiMock.saveSettingsProfile.mockResolvedValue({ work: { ...settings } })
    apiMock.applySettingsProfile.mockResolvedValue({ ...settings })
    apiMock.setSettings.mockImplementation(async (next) => next)
    apiMock.setContext.mockResolvedValue({ ...snapshot })
    apiMock.getHistory.mockResolvedValue([])
    apiMock.clearHistory.mockResolvedValue(undefined)
    apiMock.exportHistory.mockResolvedValue('[]')
    apiMock.resetLocalData.mockResolvedValue({ ...snapshot })
    apiMock.startSession.mockResolvedValue({ ...snapshot })
    apiMock.startDueBreak.mockResolvedValue({
      ...snapshot,
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 20,
    })
    apiMock.pause.mockResolvedValue({ ...snapshot, phase: { paused: { reason: 'manual' } } })
    apiMock.pauseForMinutes.mockResolvedValue({
      ...snapshot,
      phase: { paused: { reason: 'manual' } },
      paused_until: '2026-07-27T15:30:00Z',
    })
    apiMock.resume.mockResolvedValue({ ...snapshot })
    apiMock.takeBreakNow.mockResolvedValue({
      ...snapshot,
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 20,
    })
    apiMock.skipBreak.mockResolvedValue({ ...snapshot })
    apiMock.postponeBreak.mockResolvedValue({ ...snapshot, remaining_seconds: 300 })
    apiMock.syncWatchSettings.mockResolvedValue(undefined as never)
    apiMock.sendTestNudge.mockResolvedValue('delivered')
    apiMock.getWatchStatus.mockResolvedValue({
      platform: 'macos',
      available: true,
      paired: true,
      app_installed: true,
      reachable: true,
      last_synced_revision: 1,
      last_error: null,
      capabilities: {
        timer_display: true,
        local_reminders: true,
        test_haptic: true,
        remote_actions: true,
      },
    })
    apiMock.getAutostartStatus.mockResolvedValue({ supported: true, enabled: false })
    apiMock.setAutostartEnabled.mockImplementation(async (enabled) => ({
      supported: true,
      enabled,
    }))
    apiMock.getDesktopHealth.mockResolvedValue({
      platform: 'macos',
      notification_permission: 'granted',
      display_count: 2,
      autostart_supported: true,
      autostart_enabled: false,
      history_enabled: false,
      history_retention_days: 30,
      display_target: 'all',
      auto_context_supported: false,
    })
    apiMock.getHealthReport.mockResolvedValue('{\n  "platform": "macos"\n}')
    apiMock.testReminder.mockResolvedValue(undefined)
    apiMock.onTick.mockResolvedValue(() => {})
    apiMock.onState.mockResolvedValue(() => {})
    apiMock.onBreakEnded.mockResolvedValue(() => {})
    apiMock.onBlinkNudge.mockResolvedValue(() => {})
    apiMock.onPostureNudge.mockResolvedValue(() => {})
    apiMock.onHydrationNudge.mockResolvedValue(() => {})
  })

  afterEach(cleanup)

  it('renders a modern floating close button on macOS and desktop', () => {
    const userAgent = vi
      .spyOn(navigator, 'userAgent', 'get')
      .mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')

    try {
      const { container } = render(App)
      expect(container.querySelector('.window-close-button')).toBeTruthy()
    } finally {
      userAgent.mockRestore()
    }
  })

  it('keeps iPhone content inside a safe frame without desktop close button', () => {
    const userAgent = vi
      .spyOn(navigator, 'userAgent', 'get')
      .mockReturnValue('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)')

    try {
      const { container } = render(App)
      const shell = container.querySelector('.app-shell')
      expect(shell?.classList.contains('ios-safe-frame')).toBe(true)
      expect(container.querySelector('.window-close-button')).toBeNull()
    } finally {
      userAgent.mockRestore()
    }
  })

  it('uses phone-sized settings navigation and keeps the work interval available on iPhone', async () => {
    const userAgent = vi
      .spyOn(navigator, 'userAgent', 'get')
      .mockReturnValue('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)')

    try {
      const { container } = render(App)
      await fireEvent.click(await screen.findByRole('button', { name: 'Settings' }))

      expect(container.querySelector('.sidebar')).toBeNull()
      expect(screen.getByRole('navigation', { name: 'Settings categories' })).toBeTruthy()
      expect(await screen.findByText('Time between breaks')).toBeTruthy()
    } finally {
      userAgent.mockRestore()
    }
  })

  it('opens analytics directly from the iPhone dashboard without restoring the desktop sidebar', async () => {
    const userAgent = vi
      .spyOn(navigator, 'userAgent', 'get')
      .mockReturnValue('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)')

    try {
      const { container } = render(App)
      await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))

      expect(await screen.findByRole('heading', { name: 'Analytics' })).toBeTruthy()
      expect(container.querySelector('.sidebar')).toBeNull()
    } finally {
      userAgent.mockRestore()
    }
  })

  it('presents a focused Today screen and preserves timer actions', async () => {
    render(App)

    expect(await screen.findByRole('heading', { name: 'Next eye break' })).toBeTruthy()
    expect(await findClock('19:48')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Eye break now' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Pause timer' })).toBeTruthy()
    const summary = screen.getByLabelText('Today’s summary')
    expect(summary.textContent?.replace(/\s+/g, ' ')).toContain('2 Short breaks')
    expect(screen.queryByText('Reminder style')).toBeNull()

    await fireEvent.click(screen.getByRole('button', { name: 'Pause timer' }))
    await waitFor(() => expect(apiMock.pause).toHaveBeenCalledTimes(1))
    expect(await screen.findByRole('button', { name: 'Resume' })).toBeTruthy()
  })

  it('anchors settings and profile switching in a quiet dock at the bottom-left', async () => {
    render(App)

    const settingsButton = await screen.findByRole('button', { name: 'Settings' })
    const dock = settingsButton.closest('.dashboard-dock')
    expect(dock).toBeTruthy()

    const profileTrigger = screen.getByRole('button', { name: 'Profiles' })
    expect(dock?.contains(profileTrigger)).toBe(true)

    await fireEvent.click(profileTrigger)

    // Nothing saved yet — both entries refuse to apply, matching the tray control.
    const work = screen.getByRole('menuitem', { name: 'Work' }) as HTMLButtonElement
    const home = screen.getByRole('menuitem', { name: 'Home' }) as HTMLButtonElement
    expect(work.disabled).toBe(true)
    expect(home.disabled).toBe(true)
  })

  it('applies a saved profile from the dock and shows it on the dropdown trigger', async () => {
    apiMock.getSettingsProfiles.mockResolvedValue({ work: { ...settings } })
    render(App)

    const profileTrigger = await screen.findByRole('button', { name: 'Profiles' })
    await fireEvent.click(profileTrigger)
    const work = (await screen.findByRole('menuitem', { name: 'Work' })) as HTMLButtonElement
    await waitFor(() => expect(work.disabled).toBe(false))

    await fireEvent.click(work)
    await waitFor(() => expect(apiMock.applySettingsProfile).toHaveBeenCalledWith('work'))
    await waitFor(() => expect(profileTrigger.textContent).toContain('Work'))
  })

  it('reads the phase status together with the ring, directly above it', async () => {
    render(App)

    const status = (await screen.findByText('Working')) as HTMLElement
    expect(status.classList.contains('phase-status')).toBe(true)
    const stage = status.closest('.timer-stage')
    expect(stage).toBeTruthy()
    expect(stage?.classList.contains('tone-working')).toBe(true)
    // Status precedes the dial so state and countdown read as one unit.
    const dial = stage?.querySelector('.horizon-timer')
    expect(dial).toBeTruthy()
    expect(
      status.compareDocumentPosition(dial as Node) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
  })

  // The `heading` role alone does not catch this: an `sr-only` <h1> still resolves by
  // role and name, which is exactly how the visible title and subtitle went missing when
  // the phase eyebrow was moved above the ring.
  it('keeps a visible dashboard title and subtitle alongside the ring', async () => {
    render(App)

    const heading = await screen.findByRole('heading', { name: 'Next eye break' })
    expect(heading.classList.contains('sr-only')).toBe(false)
    const intro = heading.closest('.today-intro')
    expect(intro).toBeTruthy()
    expect(intro?.querySelector('.today-subhead')?.textContent).toBe(
      'A little distance makes a difference.'
    )
  })

  it('leaves timed pauses to the tray menu and still reports when PausIO resumes', async () => {
    let pushState: ((next: Snapshot) => void) | undefined
    apiMock.onState.mockImplementation(async (listener: (next: Snapshot) => void) => {
      pushState = listener
      return () => {}
    })
    render(App)
    await screen.findByRole('button', { name: 'Eye break now' })

    // "Pause for 30/60/120 minutes" is a tray-menu affordance. The window must not
    // duplicate it, but it still has to explain a timed pause the tray started.
    expect(screen.queryByRole('button', { name: 'Pause for' })).toBeNull()
    expect(screen.queryByLabelText('Pause for')).toBeNull()
    expect(apiMock.pauseForMinutes).not.toHaveBeenCalled()

    pushState!({
      ...snapshot,
      phase: { paused: { reason: 'manual' } },
      paused_until: '2026-07-27T15:30:00Z',
    })
    expect(await screen.findByText(/Resumes at/)).toBeTruthy()
  })

  it('keeps the .horizon-timer strong contract the e2e suite depends on', async () => {
    render(App)
    await findClock('19:48')
    expect(document.querySelector('.horizon-timer strong')?.textContent).toBe('19:48')
  })

  it('splits settings into sub-category pages and keeps desktop diagnostics free of wearable state', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

    expect(await screen.findByRole('heading', { name: 'Settings' })).toBeTruthy()
    // Settings opens on the Breaks category by default; other categories are separate pages.
    expect(screen.getByRole('heading', { name: 'Breaks' })).toBeTruthy()
    expect(screen.queryByRole('heading', { name: 'Schedule' })).toBeNull()
    expect(screen.queryByText('Status: Connected')).toBeNull()

    await fireEvent.click(screen.getByRole('button', { name: 'Schedule' }))
    expect(await screen.findByRole('heading', { name: 'Schedule' })).toBeTruthy()
    expect(screen.queryByRole('heading', { name: 'Breaks' })).toBeNull()

    await fireEvent.click(screen.getByRole('button', { name: 'History and privacy' }))
    await fireEvent.click(screen.getByRole('button', { name: 'More settings' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Desktop health' }))
    expect(screen.queryByText('Status: Connected')).toBeNull()
    expect(apiMock.getWatchStatus).not.toHaveBeenCalled()
    expect(apiMock.syncWatchSettings).not.toHaveBeenCalled()
  })

  it('saves an explicit local work profile from settings before it can be applied', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Schedule' }))
    const saveWork = await screen.findByRole('button', { name: 'Save current as Work' })
    await fireEvent.click(saveWork)
    await waitFor(() => expect(apiMock.saveSettingsProfile).toHaveBeenCalledWith('work'))
  })

  it('requires a second explicit action before resetting local PausIO data', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'History and privacy' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    const reset = await screen.findByRole('button', { name: 'Reset local PausIO data' })
    expect(reset.className).toContain('button-danger')
    await fireEvent.click(reset)
    expect(apiMock.resetLocalData).not.toHaveBeenCalled()
    await fireEvent.click(
      screen.getByRole('button', { name: 'Confirm reset — this cannot be undone' })
    )
    await waitFor(() => expect(apiMock.resetLocalData).toHaveBeenCalledOnce())
  })

  it('gives the destructive clear-history action its own visual treatment, distinct from the export buttons', async () => {
    apiMock.getHistory.mockResolvedValue([
      { occurred_at: '2026-07-26T10:20:00Z', kind: 'completed', break_kind: 'short' },
    ])
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))
    const clear = await screen.findByRole('button', { name: 'Clear history' })
    const exportCsv = screen.getByRole('button', { name: 'Copy CSV' })
    expect(clear.className).toContain('button-danger')
    expect(exportCsv.className).not.toContain('button-danger')
  })

  it('shows a calm, muted zero in the daily summary rather than treating "0" like any other count', async () => {
    render(App)
    await screen.findByText('2') // completed_short_breaks resolved, not the loading render
    const summary = screen.getByLabelText('Today’s summary')
    const [shortBreaks, postpones] = summary.querySelectorAll('dd')
    // Fixture has completed_short_breaks: 2, postpones_today: 1 — neither is zero.
    expect(shortBreaks.className).not.toContain('zero')
    expect(postpones.className).not.toContain('zero')
  })

  it('marks an actual zero count with the muted "zero" treatment', async () => {
    apiMock.getState.mockResolvedValue({
      ...snapshot,
      completed_short_breaks: 0,
      postpones_today: 0,
    })
    render(App)
    await findClock('19:48') // waits for the resolved snapshot, not the loading render
    const summary = screen.getByLabelText('Today’s summary')
    const [shortBreaks, postpones] = summary.querySelectorAll('dd')
    expect(shortBreaks.className).toContain('zero')
    expect(postpones.className).toContain('zero')
  })

  it('never invokes wearable IPC from the desktop app, including after a settings save', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    const range = screen.getByRole('slider', { name: 'Time between breaks' })
    await fireEvent.input(range, { target: { value: '1800' } })
    await waitFor(() => expect(apiMock.setSettings).toHaveBeenCalled())
    await fireEvent.click(await screen.findByRole('button', { name: 'History and privacy' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Desktop health' }))

    expect(screen.queryByRole('button', { name: 'Sync now' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Send test buzz' })).toBeNull()
    expect(screen.queryByText(/^Status: /)).toBeNull()
    expect(apiMock.getWatchStatus).not.toHaveBeenCalled()
    expect(apiMock.syncWatchSettings).not.toHaveBeenCalled()
    expect(apiMock.sendTestNudge).not.toHaveBeenCalled()
  })

  it('shows only phone and watch-compatible settings on a mobile host', async () => {
    apiMock.getDesktopHealth.mockResolvedValue({
      platform: 'ios',
      notification_permission: 'unavailable',
      display_count: 0,
      autostart_supported: false,
      autostart_enabled: false,
      history_enabled: false,
      history_retention_days: 30,
      display_target: 'all',
      auto_context_supported: false,
    })
    apiMock.getWatchStatus.mockResolvedValue({
      platform: 'ios',
      available: true,
      paired: true,
      app_installed: true,
      reachable: true,
      last_synced_revision: 1,
      last_error: null,
      capabilities: {
        timer_display: true,
        local_reminders: true,
        test_haptic: true,
        remote_actions: true,
      },
    })
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await waitFor(() => expect(apiMock.getWatchStatus).toHaveBeenCalled())
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Wearables' }))

    expect(await screen.findByRole('button', { name: 'Sync now' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Send test buzz' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Shortcuts & startup' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Desktop health' })).toBeNull()
    expect(screen.queryByRole('switch', { name: /Start PausIO at login/ })).toBeNull()

    await fireEvent.click(screen.getByRole('button', { name: 'Breaks' }))
    expect(screen.queryByText('Cover which displays')).toBeNull()
    expect(screen.queryByText('Play a system notification sound')).toBeNull()

    await fireEvent.click(screen.getByRole('button', { name: 'Wearables' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Sync now' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Send test buzz' }))
    expect(apiMock.syncWatchSettings).toHaveBeenCalled()
    expect(apiMock.sendTestNudge).toHaveBeenCalled()
  })

  it('lets a desktop user opt into silent launch at login', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Shortcuts & startup' }))

    const toggle = await screen.findByRole('switch', { name: /Start PausIO at login/ })
    expect((toggle as HTMLInputElement).checked).toBe(false)
    await fireEvent.click(toggle)
    await waitFor(() => expect(apiMock.setAutostartEnabled).toHaveBeenCalledWith(true))
    expect((toggle as HTMLInputElement).checked).toBe(true)
  })

  it('hides login startup controls on unsupported platforms', async () => {
    apiMock.getAutostartStatus.mockResolvedValue({ supported: false, enabled: false })
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    expect(screen.queryByRole('switch', { name: /Start PausIO at login/ })).toBeNull()
  })

  it('restores the confirmed login startup state when the OS rejects a change', async () => {
    apiMock.setAutostartEnabled.mockRejectedValue({
      code: 'internal',
      message: 'Login item unavailable.',
    })
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Shortcuts & startup' }))
    const toggle = await screen.findByRole('switch', { name: /Start PausIO at login/ })
    await fireEvent.click(toggle)
    await waitFor(() =>
      expect(screen.getByRole('alert').textContent).toContain('Login item unavailable.')
    )
    expect((toggle as HTMLInputElement).checked).toBe(false)
  })

  it('disables the stepper button at each bound, instead of silently doing nothing', async () => {
    apiMock.getSettings.mockResolvedValue({
      ...settings,
      short_break_seconds: 5,
      long_break_seconds: 1800,
    })
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    // Short break length and the long-break duration moved behind "More settings":
    // they are the two controls in the whole app most people never touch twice.
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))

    expect(
      (await screen.findByRole('button', { name: 'Decrease Short break' })) as HTMLButtonElement
    ).toHaveProperty('disabled', true)
    expect(screen.getByRole('button', { name: 'Increase Short break' })).toHaveProperty(
      'disabled',
      false
    )
    expect(screen.getByRole('button', { name: 'Increase Longer breaks' })).toHaveProperty(
      'disabled',
      true
    )
    expect(screen.getByRole('button', { name: 'Decrease Longer breaks' })).toHaveProperty(
      'disabled',
      false
    )
  })

  it('announces the save status to screen readers instead of hiding it with aria-hidden', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    const status = document.querySelector('.settings-status') as HTMLElement
    expect(status.getAttribute('aria-hidden')).toBeNull()
    expect(status.getAttribute('aria-live')).toBe('polite')
    expect(status.getAttribute('role')).toBe('status')

    const range = screen.getByRole('slider', { name: 'Time between breaks' })
    await fireEvent.input(range, { target: { value: '1800' } })
    await waitFor(() => expect(status.textContent?.trim()).toBe('Saved'))
  })

  it('auto-saves a settings change after a short debounce, with no explicit save action', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })

    expect(screen.queryByRole('button', { name: 'Save changes' })).toBeNull()

    const range = screen.getByRole('slider', { name: 'Time between breaks' })
    await fireEvent.input(range, { target: { value: '1800' } })

    await waitFor(
      () =>
        expect(apiMock.setSettings).toHaveBeenCalledWith(
          expect.objectContaining({ work_seconds: 1800 })
        ),
      { timeout: 2000 }
    )
    expect(await screen.findByText('Saved')).toBeTruthy()
  })

  it('marks the schedule time inputs with the app language, not the OS default, so 24h/AM-PM formatting follows German', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Schedule' }))

    expect((await screen.findByLabelText('Start time')).getAttribute('lang')).toBe('en-US')
    await fireEvent.click(screen.getByRole('button', { name: 'Appearance' }))
    await fireEvent.change(screen.getByRole('combobox', { name: 'Language' }), {
      target: { value: 'de' },
    })
    await fireEvent.click(screen.getByRole('button', { name: 'Zeitplan' }))
    expect((await screen.findByLabelText('Startzeit')).getAttribute('lang')).toBe('de-DE')
  })

  it('persists language and appearance immediately across the application shell', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Appearance' }))

    await fireEvent.change(await screen.findByRole('combobox', { name: 'Language' }), {
      target: { value: 'de' },
    })
    expect(document.documentElement.lang).toBe('de')
    expect(await screen.findByRole('heading', { name: 'Einstellungen' })).toBeTruthy()

    await fireEvent.change(screen.getByRole('combobox', { name: 'Farbschema' }), {
      target: { value: 'dark' },
    })
    expect(document.documentElement.dataset.theme).toBe('dark')
    await fireEvent.click(screen.getByRole('radio', { name: 'Salbei' }))
    expect(document.documentElement.dataset.accent).toBe('sage')
    await waitFor(() =>
      expect(apiMock.setSettings).toHaveBeenCalledWith(
        expect.objectContaining({ locale: 'de', theme: 'dark', accent: 'sage' })
      )
    )
  })

  it('lets a user defer interruptions for a temporary screen share without saving personal activity data', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Schedule' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    const context = await screen.findByRole('combobox', { name: 'Temporarily quiet' })
    await fireEvent.change(context, { target: { value: 'screen_share' } })
    await waitFor(() => expect(apiMock.setContext).toHaveBeenCalledWith('screen_share', 60))
    expect(apiMock.setSettings).not.toHaveBeenCalled()
  })

  it('shows local PausIO opportunities in analytics and requires a second clear action', async () => {
    apiMock.getHistory.mockResolvedValue([
      { occurred_at: '2026-07-26T10:20:00Z', kind: 'completed', break_kind: 'short' },
    ])
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))
    expect(await screen.findByRole('heading', { name: 'Analytics' })).toBeTruthy()
    expect(screen.getAllByText('Completed').length).toBeGreaterThan(0)
    await fireEvent.click(screen.getByRole('button', { name: 'Clear history' }))
    expect(apiMock.clearHistory).not.toHaveBeenCalled()
    await fireEvent.click(screen.getByRole('button', { name: 'Confirm clear history' }))
    await waitFor(() => expect(apiMock.clearHistory).toHaveBeenCalledOnce())
  })

  it('never calls setSettings when settings are opened and closed with no edits', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })
    // The header's close button is the "leave Settings" affordance — Settings and
    // History are peer destinations reached via the sidebar, not a modal stack.
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }))
    await screen.findByRole('heading', { name: 'Next eye break' })

    await new Promise((resolve) => setTimeout(resolve, 500))
    expect(apiMock.setSettings).not.toHaveBeenCalled()
  })

  it('closes settings on Escape and returns focus to the gear icon', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })

    await fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Settings' })).toBeNull())
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Settings')
  })

  it('keeps Analytics and Settings visible in the sidebar and marks the active one with aria-current', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })
    const history = screen.getByRole('button', { name: 'Analytics' })
    const settingsNav = screen.getByRole('button', { name: 'Settings' })
    expect(history.getAttribute('aria-current')).toBeNull()
    expect(settingsNav.getAttribute('aria-current')).toBe('page')

    await fireEvent.click(history)
    await screen.findByRole('heading', { name: 'Analytics' })
    expect(screen.getByRole('button', { name: 'Analytics' }).getAttribute('aria-current')).toBe(
      'page'
    )
    // The sidebar itself never unmounts moving between these two peer destinations.
    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy()
  })

  it('refuses to leave zero active days and marks the last one as disabled', async () => {
    apiMock.getSettings.mockResolvedValue({ ...settings, active_days_mask: 0b0000010 }) // Monday only
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await screen.findByRole('heading', { name: 'Settings' })
    await fireEvent.click(screen.getByRole('button', { name: 'Schedule' }))

    const monday = screen.getByRole('button', { name: 'Mon' })
    expect(monday.getAttribute('aria-disabled')).toBe('true')
    await fireEvent.click(monday)
    await new Promise((resolve) => setTimeout(resolve, 500))
    expect(apiMock.setSettings).not.toHaveBeenCalled()
  })

  it('shows an explicit start action instead of Pause while dormant', async () => {
    apiMock.getState.mockResolvedValue({
      ...snapshot,
      phase: 'dormant',
      remaining_seconds: 1200,
    })
    render(App)

    const start = await screen.findByRole('button', { name: 'Start session' })
    expect(screen.queryByLabelText('Pause')).toBeNull()
    await fireEvent.click(start)
    await waitFor(() => expect(apiMock.startSession).toHaveBeenCalledOnce())
  })

  it('renders structured command errors as readable messages', async () => {
    apiMock.pause.mockRejectedValue({
      code: 'invalid_transition',
      message: 'This timer cannot be paused right now.',
    })
    render(App)

    await fireEvent.click(await screen.findByRole('button', { name: 'Pause timer' }))
    expect((await screen.findByRole('alert')).textContent).toContain(
      'This timer cannot be paused right now.'
    )
  })

  it('offers to start or postpone a break that is due, instead of showing no controls', async () => {
    apiMock.getState.mockResolvedValue({
      ...snapshot,
      phase: { break_due: { kind: 'short' } },
      remaining_seconds: 0,
    })
    render(App)

    const start = await screen.findByRole('button', { name: 'Eye break now' })
    expect(screen.getByRole('button', { name: 'Postpone 2 min' })).toBeTruthy()
    await fireEvent.click(start)
    await waitFor(() => expect(apiMock.startDueBreak).toHaveBeenCalledOnce())
  })

  it('hides postpone once strictness leaves the balanced tier, on the main window as well as the prompt', async () => {
    apiMock.getState.mockResolvedValue({
      ...snapshot,
      phase: { break_due: { kind: 'short' } },
      remaining_seconds: 0,
    })
    apiMock.getSettings.mockResolvedValue({ ...settings, strictness: 'firm' })
    render(App)

    await screen.findByRole('button', { name: 'Eye break now' })
    expect(screen.queryByRole('button', { name: 'Postpone 2 min' })).toBeNull()
  })

  it('offers to end a break early from the main window while a break is in progress', async () => {
    apiMock.getState.mockResolvedValue({
      ...snapshot,
      phase: { breaking: { kind: 'short' } },
      remaining_seconds: 15,
    })
    render(App)

    const end = await screen.findByRole('button', { name: 'End break early' })
    await fireEvent.click(end)
    await waitFor(() => expect(apiMock.skipBreak).toHaveBeenCalledOnce())
  })

  it('shows a calm inline empty state for analytics, not a floating error-styled toast', async () => {
    apiMock.getHistory.mockResolvedValue([])
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))

    const empty = await screen.findByText(/Local analytics are off/)
    expect(empty.closest('.analytics-empty-state')).toBeTruthy()
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('translates the break kind and explains respected context without leaking raw enum values', async () => {
    apiMock.getHistory.mockResolvedValue([
      { break_id: 'long-1', occurred_at: '2026-07-26T09:00:00Z', kind: 'due', break_kind: 'long' },
      {
        break_id: 'long-1',
        occurred_at: '2026-07-26T09:00:01Z',
        kind: 'deferred',
        context: 'screen_share',
      },
      {
        break_id: 'long-1',
        occurred_at: '2026-07-26T10:20:00Z',
        kind: 'completed',
        break_kind: 'long',
      },
    ])
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))
    await fireEvent.click(screen.getByRole('button', { name: 'All time' }))

    expect(await screen.findByText(/long/)).toBeTruthy()
    expect(screen.getByText('Quiet context respected')).toBeTruthy()
    expect(screen.queryByText(/screen_share/)).toBeNull()
  })

  it('shows a real, single-line placeholder for fixed break times — never an escaped literal', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Schedule' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    const textarea = (await screen.findByLabelText('Fixed break times')) as HTMLTextAreaElement
    expect(textarea.placeholder).toBe('12:30, 15:00')
  })

  it('lets you type a second fixed break time without characters vanishing mid-edit', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Schedule' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    const textarea = (await screen.findByLabelText('Fixed break times')) as HTMLTextAreaElement

    // A partial, not-yet-parseable second line must survive verbatim while focused.
    await fireEvent.input(textarea, { target: { value: '12:30\n1' } })
    expect(textarea.value).toBe('12:30\n1')
    expect(apiMock.setSettings).not.toHaveBeenCalled()

    await fireEvent.input(textarea, { target: { value: '12:30\n15:00' } })
    expect(textarea.value).toBe('12:30\n15:00')
    await fireEvent.blur(textarea)
    await waitFor(() =>
      expect(apiMock.setSettings).toHaveBeenCalledWith(
        expect.objectContaining({ fixed_break_minutes: [750, 900] })
      )
    )
  })

  it('lets you type a break message with a trailing space without it being stripped mid-edit', async () => {
    render(App)
    await screen.findByRole('heading', { name: 'Next eye break' })
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'Appearance' }))
    await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
    const textarea = (await screen.findByLabelText('Break messages')) as HTMLTextAreaElement

    await fireEvent.input(textarea, { target: { value: 'Rest your eyes ' } })
    expect(textarea.value).toBe('Rest your eyes ')
    await fireEvent.blur(textarea)
    await waitFor(() =>
      expect(apiMock.setSettings).toHaveBeenCalledWith(
        expect.objectContaining({ break_messages: ['Rest your eyes'] })
      )
    )
  })

  describe('clipboard export', () => {
    afterEach(() => {
      Reflect.deleteProperty(navigator, 'clipboard')
    })

    it('copies the health report only after the write actually succeeds', async () => {
      const writeText = vi.fn().mockResolvedValue(undefined)
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'History and privacy' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'Desktop health' }))
      await fireEvent.click(screen.getByRole('button', { name: 'Copy redacted health report' }))

      await waitFor(() => expect(writeText).toHaveBeenCalledWith('{\n  "platform": "macos"\n}'))
      expect(await screen.findByText('Redacted report copied locally.')).toBeTruthy()
      expect(screen.queryByRole('alert')).toBeNull()
    })

    it('shows a translated recovery message instead of a raw clipboard exception when copying fails', async () => {
      Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true })
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'History and privacy' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'More settings' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'Desktop health' }))
      await fireEvent.click(screen.getByRole('button', { name: 'Copy redacted health report' }))

      expect((await screen.findByRole('alert')).textContent).toContain(
        "Couldn't copy automatically"
      )
      expect(screen.queryByText('Redacted report copied locally.')).toBeNull()
      // The report itself is still shown for a manual copy, even though the automatic write failed.
      const report = (await screen.findByLabelText(
        'Copy redacted health report'
      )) as HTMLTextAreaElement
      expect(report.value).toBe('{\n  "platform": "macos"\n}')
    })

    it('never shows a false "copied" confirmation when the clipboard write silently no-ops', async () => {
      apiMock.getHistory.mockResolvedValue([
        { occurred_at: '2026-07-26T10:20:00Z', kind: 'completed', break_kind: 'short' },
      ])
      const writeText = vi.fn().mockRejectedValue(new DOMException('denied', 'NotAllowedError'))
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'Analytics' }))
      await fireEvent.click(await screen.findByRole('button', { name: 'Copy CSV' }))

      await waitFor(() => expect(writeText).toHaveBeenCalled())
      expect(screen.queryByText('Local export copied.')).toBeNull()
      expect((await screen.findByRole('alert')).textContent).toContain(
        "Couldn't copy automatically"
      )
    })
  })

  describe('break delivery', () => {
    const openDelivery = async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      // "Breaks" is the default category now -- Timing and Break delivery merged
      // into it, so the mode picker is on-screen without a second click.
      return screen.findByRole('combobox', { name: /How breaks appear/i })
    }

    it('offers one mode picker instead of two dropdowns that can contradict each other', async () => {
      await openDelivery()
      // "Notification only" was a display target that silently overrode strictness;
      // it is now a mode, so the pair can no longer disagree.
      expect(screen.queryByText('Notification only')).toBeNull()
      expect(screen.getByText('Ask first, then cover the screen')).toBeTruthy()
    })

    it('explains the selected mode, which the old bare select never did', async () => {
      const picker = await openDelivery()
      expect(screen.getByText(/A notification asks you to start now or postpone/)).toBeTruthy()

      await fireEvent.change(picker, { target: { value: 'hold' } })
      expect(await screen.findByText(/an emergency exit is always available/)).toBeTruthy()
    })

    it('hides the display choice for notify-only, where it would do nothing', async () => {
      const picker = await openDelivery()
      expect(screen.getByText('Cover which displays')).toBeTruthy()

      await fireEvent.change(picker, { target: { value: 'notify' } })
      await waitFor(() => expect(screen.queryByText('Cover which displays')).toBeNull())
    })

    it('stores a consistent strictness/display pair for every mode', async () => {
      const picker = await openDelivery()
      await fireEvent.change(picker, { target: { value: 'notify' } })
      await waitFor(() =>
        expect(apiMock.setSettings).toHaveBeenCalledWith(
          expect.objectContaining({ strictness: 'gentle', display_target: 'notification_only' })
        )
      )

      await fireEvent.change(picker, { target: { value: 'hold' } })
      await waitFor(() =>
        expect(apiMock.setSettings).toHaveBeenCalledWith(
          expect.objectContaining({ strictness: 'strict', display_target: 'all' })
        )
      )
    })

    it('warns when a covering mode is paired with no advance notice', async () => {
      apiMock.getSettings.mockResolvedValue({ ...settings, pre_break_seconds: 0 })
      const picker = await openDelivery()
      expect(screen.queryByText(/your screen is covered the moment/)).toBeNull()

      await fireEvent.change(picker, { target: { value: 'cover' } })
      expect(
        await screen.findByText(/your screen is covered the moment a break is due/)
      ).toBeTruthy()
    })

    it('only offers a postpone limit in the mode where postponing is reachable', async () => {
      const picker = await openDelivery()
      expect(screen.getByText('Postpones allowed per day')).toBeTruthy()
      // Was "1 postpones" -- the option list had no singular form.
      expect(screen.getByRole('option', { name: '1 postpone' })).toBeTruthy()
      expect(screen.getByRole('option', { name: '3 postpones' })).toBeTruthy()

      // Every pointer path to postpone() is gated on Balanced.
      await fireEvent.change(picker, { target: { value: 'cover' } })
      await waitFor(() => expect(screen.queryByText('Postpones allowed per day')).toBeNull())
    })
  })

  describe('settings summary', () => {
    it('states the combined outcome of the current settings', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      const summary = await screen.findByText(/Every 20 minutes: a break of 20 seconds/)
      expect(summary.textContent).toContain('Active Mon\u2013Fri, 09:00 until 18:00.')
      expect(summary.textContent).toContain('A heads-up arrives 30 seconds beforehand.')
      expect(summary.textContent).toContain('the break covers all displays')
    })

    it('recomputes when a setting changes, so the effect is visible immediately', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      await screen.findByText(/Every 20 minutes/)

      const interval = screen.getByRole('slider', { name: 'Time between breaks' })
      await fireEvent.input(interval, { target: { value: '2700' } })

      expect(await screen.findByText(/Every 45 minutes: a break of 20 seconds/)).toBeTruthy()
    })
  })

  describe('settings navigation keyboard support', () => {
    it('moves between categories with the arrow keys, Home and End', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      const breaks = await screen.findByRole('button', { name: 'Breaks' })
      breaks.focus()
      await fireEvent.keyDown(breaks, { key: 'ArrowDown' })
      expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Schedule' }))

      await fireEvent.keyDown(document.activeElement!, { key: 'End' })
      expect(document.activeElement).toBe(
        screen.getByRole('button', { name: 'History and privacy' })
      )

      // Wraps, so End then ArrowDown returns to the first category.
      await fireEvent.keyDown(document.activeElement!, { key: 'ArrowDown' })
      expect(document.activeElement).toBe(breaks)

      await fireEvent.keyDown(document.activeElement!, { key: 'ArrowUp' })
      expect(document.activeElement).toBe(
        screen.getByRole('button', { name: 'History and privacy' })
      )

      await fireEvent.keyDown(document.activeElement!, { key: 'Home' })
      expect(document.activeElement).toBe(breaks)
    })

    it('leaves unrelated keys to the browser', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      const breaks = await screen.findByRole('button', { name: 'Breaks' })
      breaks.focus()
      await fireEvent.keyDown(breaks, { key: 'Tab' })
      expect(document.activeElement).toBe(breaks)
    })
  })

  describe('first-run onboarding', () => {
    it('shows the wizard with a welcome screen before the dashboard on a fresh install, and never a Strict default', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      const { container } = render(App)

      expect(await screen.findByRole('heading', { name: 'Welcome to PausIO' })).toBeTruthy()
      const body = container.querySelector('.app-content-body') as HTMLDivElement | null
      expect(body?.inert).toBe(true)

      // Step 1 -> Step 2
      await fireEvent.click(screen.getByRole('button', { name: 'Get Started' }))
      expect(await screen.findByRole('heading', { name: 'When do you work?' })).toBeTruthy()

      // Step 2 -> Step 3
      await fireEvent.click(screen.getByRole('button', { name: 'Next' }))
      expect(
        await screen.findByRole('heading', { name: 'How should breaks interrupt you?' })
      ).toBeTruthy()

      // Safety invariant: Strict must never be pre-selected during onboarding.
      const askOption = screen.getByRole('radio', { name: /Ask first/i })
      const holdOption = screen.getByRole('radio', { name: /Cover the screen and hold it/i })
      expect(askOption.getAttribute('aria-checked')).toBe('true')
      expect(holdOption.getAttribute('aria-checked')).toBe('false')
    })

    it('never shows the wizard once onboarding has already been completed', async () => {
      apiMock.getOnboardingState.mockResolvedValue(true)
      render(App)

      expect(await screen.findByRole('heading', { name: 'Next eye break' })).toBeTruthy()
      expect(screen.queryByRole('heading', { name: 'Welcome to PausIO' })).toBeNull()
      expect(screen.queryByRole('heading', { name: 'When do you work?' })).toBeNull()
    })

    it('lets Skip close the wizard immediately from any step, and persists that it was seen', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      render(App)
      await screen.findByRole('heading', { name: 'Welcome to PausIO' })

      await fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
      await waitFor(() => expect(apiMock.completeOnboarding).toHaveBeenCalledOnce())
      expect(await screen.findByRole('heading', { name: 'Next eye break' })).toBeTruthy()
    })

    it('walks all four steps, firing a real break on the last one before finishing', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      render(App)
      await screen.findByRole('heading', { name: 'Welcome to PausIO' })
      const wizard = () => within(screen.getByRole('dialog'))
      expect(wizard().getByText('Step 1 of 4')).toBeTruthy()

      // Step 1: Welcome -> Step 2: Schedule
      await fireEvent.click(wizard().getByRole('button', { name: 'Get Started' }))
      expect(await wizard().findByRole('heading', { name: 'When do you work?' })).toBeTruthy()
      expect(wizard().getByText('Step 2 of 4')).toBeTruthy()

      // Step 2: Schedule -> Step 3: Delivery
      await fireEvent.click(wizard().getByRole('button', { name: 'Next' }))
      expect(
        await wizard().findByRole('heading', { name: 'How should breaks interrupt you?' })
      ).toBeTruthy()
      expect(wizard().getByText('Step 3 of 4')).toBeTruthy()

      // Step 3: Delivery -> Step 4: Test Break
      await fireEvent.click(wizard().getByRole('button', { name: 'Next' }))
      expect(await wizard().findByRole('heading', { name: 'See it for yourself' })).toBeTruthy()
      expect(wizard().getByText('Step 4 of 4')).toBeTruthy()

      // Fire test break
      await fireEvent.click(wizard().getByRole('button', { name: 'Eye break now' }))
      await waitFor(() => expect(apiMock.takeBreakNow).toHaveBeenCalledOnce())
      expect(await wizard().findByText(/look away for a moment/)).toBeTruthy()

      // Finish and enter dashboard
      await fireEvent.click(wizard().getByRole('button', { name: 'Start Using PausIO' }))
      await waitFor(() => expect(apiMock.completeOnboarding).toHaveBeenCalledOnce())
      expect(await screen.findByRole('heading', { name: 'Next eye break' })).toBeTruthy()
    })

    it('lets a schedule choice made during onboarding reach the saved settings', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      render(App)
      await screen.findByRole('heading', { name: 'Welcome to PausIO' })

      await fireEvent.click(screen.getByRole('button', { name: 'Get Started' }))
      await screen.findByRole('heading', { name: 'When do you work?' })

      await fireEvent.click(screen.getByRole('button', { name: 'Sun' }))
      await waitFor(() =>
        expect(apiMock.setSettings).toHaveBeenCalledWith(
          expect.objectContaining({ active_days_mask: settings.active_days_mask | 0b1 })
        )
      )
    })

    it('closes the wizard even when persisting completion fails, rather than trapping the user', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      apiMock.completeOnboarding.mockRejectedValue(new Error('offline'))
      render(App)
      await screen.findByRole('heading', { name: 'Welcome to PausIO' })

      await fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
      expect(await screen.findByRole('heading', { name: 'Next eye break' })).toBeTruthy()
    })

    it('lets the user switch language dynamically during onboarding', async () => {
      apiMock.getOnboardingState.mockResolvedValue(false)
      render(App)
      await screen.findByRole('heading', { name: 'Welcome to PausIO' })

      await fireEvent.click(screen.getByRole('button', { name: 'DE' }))
      expect(await screen.findByRole('heading', { name: 'Willkommen bei PausIO' })).toBeTruthy()
      expect(screen.getByRole('button', { name: 'Loslegen' })).toBeTruthy()

      await fireEvent.click(screen.getByRole('button', { name: 'EN' }))
      expect(await screen.findByRole('heading', { name: 'Welcome to PausIO' })).toBeTruthy()
      expect(screen.getByRole('button', { name: 'Get Started' })).toBeTruthy()
    })
  })

  describe('settings search', () => {
    it('finds and jumps to a control in a different category, opening "More settings" for it', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))
      await screen.findByRole('heading', { name: 'Breaks' })

      const search = screen.getByRole('searchbox', { name: 'Search settings' })
      await fireEvent.input(search, { target: { value: 'blink' } })

      const result = await screen.findByRole('option', { name: /Blink reminder/ })
      await fireEvent.click(result)

      // Lands on Breaks (where blink reminder lives) with "More settings" already
      // open, instead of leaving the person to guess which pane and disclosure.
      expect(await screen.findByRole('heading', { name: 'Breaks' })).toBeTruthy()
      expect(screen.getByRole('combobox', { name: 'Blink reminder' })).toBeTruthy()
      // The query itself clears so the result list does not linger over the pane.
      expect(document.querySelector('.settings-search-results')).toBeNull()
    })

    it('does not force "More settings" open for a result that is in the default view', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      const search = screen.getByRole('searchbox', { name: 'Search settings' })
      await fireEvent.input(search, { target: { value: 'Language' } })
      // The option's accessible name concatenates its visible label and category
      // hint text, so this matches the label as a prefix rather than exactly.
      await fireEvent.click(await screen.findByRole('option', { name: /^Language/ }))

      expect(await screen.findByRole('heading', { name: 'Appearance' })).toBeTruthy()
      expect(screen.getByRole('combobox', { name: 'Language' })).toBeTruthy()
      // "More settings" was not force-opened, since this result did not need it.
      expect(
        screen.queryByRole('button', { name: 'More settings' })?.getAttribute('aria-expanded')
      ).toBe('false')
    })

    it('shows an honest empty state for a query that matches nothing', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), {
        target: { value: 'xyzxyz' },
      })
      expect(await screen.findByText('No matching settings.')).toBeTruthy()
    })

    it('clears its own leftover open state when navigating normally afterwards', async () => {
      render(App)
      await screen.findByRole('heading', { name: 'Next eye break' })
      await fireEvent.click(screen.getByRole('button', { name: 'Settings' }))

      await fireEvent.input(screen.getByRole('searchbox', { name: 'Search settings' }), {
        target: { value: 'blink' },
      })
      await fireEvent.click(await screen.findByRole('option', { name: /Blink reminder/ }))
      await screen.findByRole('combobox', { name: 'Blink reminder' })

      // A pane visited by clicking its own nav button must not inherit another
      // pane's forced-open Advanced state.
      await fireEvent.click(screen.getByRole('button', { name: 'Schedule' }))
      expect(
        screen.getByRole('button', { name: 'More settings' }).getAttribute('aria-expanded')
      ).toBe('false')
    })
  })
})
