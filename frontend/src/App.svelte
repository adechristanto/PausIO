<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import BreakOverlay from './components/BreakOverlay.svelte'
  import BreakPrompt from './components/BreakPrompt.svelte'
  import NudgeToast from './components/NudgeToast.svelte'
  import HistoryPanel from './components/HistoryPanel.svelte'
  import Onboarding from './components/Onboarding.svelte'
  import { searchSettings } from './lib/settingsSearch'
  import SettingsPanel from './components/SettingsPanel.svelte'
  import TimerRing from './components/TimerRing.svelte'
  import pausioMark from './assets/pausio-mark.svg'
  import { errorMessage } from './lib/errors'
  import { clockToMinutes, formatClock, formatTimeOfDay, minutesToClock } from './lib/format'
  import { pauseLabel, setLocale, t } from './lib/i18n'
  import { api } from './lib/pausio'
  import type { AnalyticsRange } from './lib/history-analytics'
  import { playBreakSound } from './lib/sound'
  import { tooltip } from './lib/tooltip'
  import type {
    Accent,
    AutostartStatus,
    BreakKind,
    BreakRoutine,
    ContextReason,
    DesktopHealth,
    DisplayTarget,
    HistoryEvent,
    Locale,
    NudgeResult,
    Settings,
    SettingsProfiles,
    Snapshot,
    SoundTheme,
    Strictness,
    SystemSound,
    Theme,
    WatchStatus,
  } from './lib/types'

  type Screen = 'dashboard' | 'settings' | 'history'
  type Tone = 'working' | 'warning' | 'rest' | 'paused' | 'dormant' | 'loading'
  type SettingsCategory =
    'breaks' | 'schedule' | 'appearance' | 'shortcuts' | 'privacy' | 'wearables'
  type SettingsLabelKey =
    | 'section_breaks'
    | 'section_schedule'
    | 'section_appearance'
    | 'section_shortcuts'
    | 'section_history_privacy'
    | 'wearables_heading'
  // Nine categories collapsed to five (plus mobile-only Wearables): Timing and
  // Break delivery merged into Breaks, Sound moved in with it, auto-detect moved
  // into Schedule (it answers the same question -- "when should you not interrupt
  // me"), Autostart paired with Shortcuts, diagnostics moved into Privacy, the
  // Profiles pane was retired in favour of the dashboard dropdown (profiles are a
  // personal saved setup, not something that belongs in a settings tree someone
  // is trying to learn), and "Temporarily quiet" left Settings entirely for the
  // dashboard dock -- it changes runtime state, not a stored setting.
  const settingsCategories: { id: SettingsCategory; labelKey: SettingsLabelKey }[] = [
    { id: 'breaks', labelKey: 'section_breaks' },
    { id: 'schedule', labelKey: 'section_schedule' },
    { id: 'appearance', labelKey: 'section_appearance' },
    { id: 'shortcuts', labelKey: 'section_shortcuts' },
    { id: 'privacy', labelKey: 'section_history_privacy' },
  ]
  const mobileSettingsCategories: { id: SettingsCategory; labelKey: SettingsLabelKey }[] = [
    { id: 'breaks', labelKey: 'section_breaks' },
    { id: 'schedule', labelKey: 'section_schedule' },
    { id: 'appearance', labelKey: 'section_appearance' },
    { id: 'privacy', labelKey: 'section_history_privacy' },
    { id: 'wearables', labelKey: 'wearables_heading' },
  ]

  let state: Snapshot | null = null
  let settings: Settings | null = null
  let confirmed: Settings | null = null // last engine-confirmed settings, for revert-on-error
  let error = ''
  let saved = false
  let isSaving = false
  let mode: Screen = 'dashboard'
  let settingsCategory: SettingsCategory = 'breaks'
  let watchStatus: WatchStatus | null = null
  let autostartStatus: AutostartStatus | null = null
  let nudgeResult: NudgeResult | null = null
  let isUpdatingAutostart = false
  let diagnosticsOpen = false
  let announce = ''
  let history: HistoryEvent[] = []
  let historyClearConfirmation = false
  let historyRangeDays: AnalyticsRange = 7
  let profiles: SettingsProfiles = {}
  let activeProfile: 'work' | 'home' | null = null
  let profileMenuOpen = false
  let profileTrigger: HTMLButtonElement | undefined
  let profileMenuEl: HTMLElement | undefined
  let resetLocalDataConfirmation = false
  let desktopHealth: DesktopHealth | null = null
  let healthReport = ''
  let healthReportCopied = false
  let historyExport = ''
  let historyExportCopied = false
  let contextMinutes = 60
  let showOnboarding = false
  let settingsQuery = ''
  // Shared across every pane's "More settings" disclosure -- only one pane is
  // ever mounted at a time, so one flag safely stands for whichever is showing.
  let advancedOpen = false

  const isMobileWearableHost = () =>
    desktopHealth?.platform === 'ios' || desktopHealth?.platform === 'android'
  const isMobileSettingsHost = () =>
    isMobileWearableHost() || /iPhone|iPad|iPod|Android/.test(navigator.userAgent)
  const analyticsDeviceLabel = () => {
    if (desktopHealth?.platform === 'ios') return t('analytics_device_iphone')
    if (desktopHealth?.platform === 'android') return t('analytics_device_android')
    if (desktopHealth?.platform === 'macos') return t('analytics_device_mac')
    if (desktopHealth?.platform === 'windows') return t('analytics_device_windows')
    if (desktopHealth?.platform === 'linux') return t('analytics_device_linux')
    return t('analytics_device_this')
  }
  const visibleSettingsCategories = () =>
    isMobileSettingsHost() ? mobileSettingsCategories : settingsCategories
  const wearableProps = () =>
    isMobileWearableHost() ? { watchStatus, nudgeResult, syncWatchSettings, sendTestNudge } : {}

  // Draft text for the two free-form textareas. Kept separate from `settings` so that
  // typing (including Enter, trailing spaces, or a still-incomplete "12:") is never
  // re-parsed and clobbered mid-keystroke. Parsing (and the settings write) happens only
  // on blur, via commitBreakMessages / commitFixedBreakTimes.
  let breakMessagesDraft = ''
  let fixedBreaksDraft = ''
  const syncMessageDrafts = (next: Settings | null) => {
    breakMessagesDraft = (next?.break_messages ?? []).join('\n')
    fixedBreaksDraft = (next?.fixed_break_minutes ?? []).map(minutesToClock).join('\n')
  }

  let settingsButton: HTMLButtonElement | undefined
  let settingsRegion: HTMLElement | undefined
  let appScroll: HTMLElement | undefined

  function selectSettingsCategory(id: SettingsCategory) {
    settingsCategory = id
    // Each pane's own Advanced state (see selectSearchResult below), not a single
    // flag shared across every pane -- otherwise opening "More settings" once and
    // then clicking to a different category would open that pane's too.
    advancedOpen = false
    appScroll?.scrollTo?.({ top: 0, behavior: 'instant' })
  }
  function selectSearchResult(result: ReturnType<typeof searchSettings>[number]) {
    settingsCategory = result.entry.category
    advancedOpen = result.entry.advanced
    settingsQuery = ''
    appScroll?.scrollTo?.({ top: 0, behavior: 'instant' })
  }

  /**
   * Roving focus across the settings categories. A run of same-purpose navigation
   * buttons should be traversable with the arrow keys rather than by tabbing through
   * every one of them. Direction follows the axis each nav is drawn on: the desktop
   * sidebar stacks vertically, the mobile strip runs horizontally.
   */
  function onSettingsNavKeydown(event: KeyboardEvent) {
    const button = event.currentTarget as HTMLButtonElement
    const container = button.parentElement
    if (!container) return
    const buttons = Array.from(container.querySelectorAll('button'))
    const index = buttons.indexOf(button)
    if (index < 0) return
    const horizontal = container.classList.contains('mobile-settings-nav')
    const targets: Record<string, number> = {
      [horizontal ? 'ArrowRight' : 'ArrowDown']: (index + 1) % buttons.length,
      [horizontal ? 'ArrowLeft' : 'ArrowUp']: (index - 1 + buttons.length) % buttons.length,
      Home: 0,
      End: buttons.length - 1,
    }
    const next = targets[event.key]
    if (next === undefined) return
    event.preventDefault()
    buttons[next].focus()
  }

  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let savedTimer: ReturnType<typeof setTimeout> | undefined

  const isApple = /Mac|iPhone|iPad/.test(navigator.userAgent)
  const isIOS = /iPhone|iPad|iPod/.test(navigator.userAgent)
  const isMac = /Mac/.test(navigator.userAgent) && !isIOS
  const isAndroid = /Android/.test(navigator.userAgent)
  // All desktop hosts (macOS, Windows, Linux) draw a frameless custom title bar
  // with a drag region and a top-right X close button that hides to tray/dock.
  const isDesktopTitlebar = !isIOS && !isAndroid
  // Outside a real Tauri webview (component tests, a plain browser preview)
  // `window.__TAURI_INTERNALS__` is absent and `getCurrentWindow()` throws
  // synchronously — the titlebar UI still renders there, just without a
  // window to control.
  const appWindow = isDesktopTitlebar && '__TAURI_INTERNALS__' in window ? getCurrentWindow() : null
  let isWindowMaximized = false
  const refreshMaximizedState = async () => {
    if (!appWindow) return
    isWindowMaximized = await appWindow.isMaximized()
  }
  const minimizeWindow = () => appWindow?.minimize()
  const toggleMaximizeWindow = () => appWindow?.toggleMaximize()
  const closeWindow = () => appWindow?.close()
  const windowView = new URLSearchParams(window.location.search).get('view') ?? 'main'
  const overlayDisplay = Number(new URLSearchParams(window.location.search).get('display') ?? '0')
  const overlay = windowView === 'break-overlay'
  const prompt = windowView === 'break-prompt'
  // PausIO's stand-in for a notification banner, shown only when macOS will not
  // draw one. It carries no timer state, so it needs none of the engine wiring
  // the other views set up below.
  const nudgeToast = windowView === 'nudge-toast'
  const nudgeKind = new URLSearchParams(window.location.search).get('nudge') ?? 'blink'
  // Applied synchronously, before first render: the toast is short-lived and
  // self-contained, so it takes its language from the URL rather than waiting on
  // a settings fetch that would make its one line flip language a frame in.
  if (nudgeToast) {
    const nudgeLocale = new URLSearchParams(window.location.search).get('locale')
    setLocale(nudgeLocale === 'de' ? 'de' : 'en')
    document.documentElement.lang = nudgeLocale === 'de' ? 'de' : 'en'
  }
  document.documentElement.dataset.windowView = windowView

  const isPaused = (value: Snapshot | null) =>
    Boolean(value && typeof value.phase === 'object' && 'paused' in value.phase)
  const isBreaking = (value: Snapshot | null) =>
    Boolean(value && typeof value.phase === 'object' && 'breaking' in value.phase)
  const isBreakDue = (value: Snapshot | null) =>
    Boolean(value && typeof value.phase === 'object' && 'break_due' in value.phase)
  const isDormant = (value: Snapshot | null) => value?.phase === 'dormant'
  const canPause = (value: Snapshot | null) =>
    value?.phase === 'working' || value?.phase === 'pre_break'
  const phase = (value: Snapshot | null) => {
    if (!value) return t('phase_loading')
    const current = value.phase
    if (typeof current === 'string')
      return t(`phase_${current}` as 'phase_working' | 'phase_pre_break' | 'phase_dormant')
    if ('break_due' in current) return t('phase_break_due')
    if ('breaking' in current)
      return current.breaking.kind === 'short' ? t('phase_short_break') : t('phase_long_break')
    return pauseLabel(current.paused.reason)
  }
  const phaseTone = (value: Snapshot | null): Tone => {
    if (!value) return 'loading'
    if (typeof value.phase === 'string')
      return value.phase === 'pre_break'
        ? 'warning'
        : value.phase === 'dormant'
          ? 'dormant'
          : 'working'
    if ('break_due' in value.phase) return 'warning'
    return 'breaking' in value.phase ? 'rest' : 'paused'
  }
  const dialTone = (
    value: Snapshot | null
  ): 'focus' | 'warning' | 'rest' | 'paused' | 'dormant' | 'loading' => {
    const current = phaseTone(value)
    if (current === 'working') return 'focus'
    return current
  }
  const countdownDuration = (value: Snapshot | null) => {
    if (!value || !settings) return 0
    if (typeof value.phase === 'object' && 'breaking' in value.phase)
      return value.phase.breaking.kind === 'long'
        ? settings.long_break_seconds
        : settings.short_break_seconds
    return settings.work_seconds
  }
  const remainingFraction = (value: Snapshot | null): number | null => {
    if (!value || !settings) return null
    const duration = countdownDuration(value)
    if (duration <= 0) return null
    return Math.max(0, Math.min(1, value.remaining_seconds / duration))
  }
  const intervalMinutes = () => (settings ? Math.round(settings.work_seconds / 60) : 20)
  const timerCaption = () => {
    if (isDormant(state) && settings) {
      return t('dormant_caption', {
        start: minutesToClock(settings.active_start_minutes),
        end: minutesToClock(settings.active_end_minutes),
      })
    }
    return t('today_rhythm', { work: intervalMinutes(), rest: settings?.short_break_seconds ?? 20 })
  }
  const pausedUntilLabel = (value: Snapshot | null) => {
    if (!value?.paused_until) return null
    const time = new Date(value.paused_until)
    if (Number.isNaN(time.getTime())) return null
    return t('pause_resumes_at', { time: formatTimeOfDay(time, settings?.locale) })
  }

  async function run(action: () => Promise<Snapshot>) {
    try {
      error = ''
      state = await action()
    } catch (e) {
      error = errorMessage(e)
    }
  }

  function applyAppearance(next: Settings) {
    const theme: Theme = next.theme ?? 'system'
    const locale: Locale = next.locale ?? 'en'
    document.documentElement.dataset.theme = theme
    document.documentElement.dataset.accent = next.accent ?? 'horizon'
    document.documentElement.lang = locale
    setLocale(locale)
  }

  function editSettings(next: Settings) {
    applyAppearance(next)
    settings = next
    clearTimeout(saveTimer)
    saveTimer = setTimeout(() => void commitSettings(), 450)
  }
  async function flushSettings() {
    if (saveTimer === undefined) return
    clearTimeout(saveTimer)
    saveTimer = undefined
    await commitSettings()
  }
  async function commitSettings() {
    if (!settings || isSaving) return
    const attempt = settings
    saveTimer = undefined
    try {
      isSaving = true
      error = ''
      const next = await api.setSettings(attempt)
      confirmed = next
      if (settings === attempt) {
        applyAppearance(next)
        settings = next // never clobber a newer edit
      }
      if (isMobileWearableHost()) {
        void api
          .syncWatchSettings()
          .then(refreshWatchStatus)
          .catch(() => {})
      }
      saved = true
      clearTimeout(savedTimer)
      savedTimer = setTimeout(() => (saved = false), 1600)
    } catch (e) {
      error = errorMessage(e)
      if (confirmed) {
        applyAppearance(confirmed)
        settings = confirmed // revert to last good
        syncMessageDrafts(confirmed)
      }
    } finally {
      isSaving = false
    }
  }

  async function refreshWatchStatus() {
    if (!isMobileWearableHost()) return
    try {
      watchStatus = await api.getWatchStatus()
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function syncWatchSettings() {
    if (!isMobileWearableHost()) return
    try {
      await api.syncWatchSettings()
      await refreshWatchStatus()
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function sendTestNudge() {
    if (!isMobileWearableHost()) return
    try {
      nudgeResult = await api.sendTestNudge()
      await refreshWatchStatus()
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function setAutostart(enabled: boolean) {
    if (!autostartStatus || isUpdatingAutostart) return
    const previous = autostartStatus
    try {
      isUpdatingAutostart = true
      error = ''
      autostartStatus = { ...previous, enabled }
      autostartStatus = await api.setAutostartEnabled(enabled)
    } catch (e) {
      autostartStatus = previous
      error = errorMessage(e)
    } finally {
      isUpdatingAutostart = false
    }
  }
  function toggleDay(index: number) {
    if (!settings) return
    const next = settings.active_days_mask ^ (1 << index)
    if (next === 0) return // refuse to disable the last active day
    editSettings({ ...settings, active_days_mask: next })
  }
  // Failing to persist "onboarding seen" is a nuisance (it reappears once), not a
  // safety issue, so the wizard closes either way rather than trapping the person
  // behind a screen because of a store write it never told them about.
  async function finishOnboarding() {
    showOnboarding = false
    try {
      await api.completeOnboarding()
    } catch {
      // Best-effort; see comment above.
    }
  }
  function setNumber(key: 'short_break_seconds' | 'long_break_seconds', value: number) {
    if (settings) editSettings({ ...settings, [key]: value })
  }
  const sameSequence = (a: number[], b: number[]) =>
    a.length === b.length && a.every((value, index) => value === b[index])
  function commitBreakMessages() {
    if (!settings) return
    const messages = breakMessagesDraft
      .split('\n')
      .map((message) => message.trim())
      .filter(Boolean)
      .slice(0, 12)
    breakMessagesDraft = messages.join('\n')
    const current = settings.break_messages ?? []
    if (
      messages.length === current.length &&
      messages.every((message, index) => message === current[index])
    )
      return
    editSettings({ ...settings, break_messages: messages })
  }
  function commitFixedBreakTimes() {
    if (!settings) return
    const minutes = [
      ...new Set(
        fixedBreaksDraft
          .split(/[,\n]/)
          .map((time) => clockToMinutes(time.trim()))
          .filter(Number.isFinite)
      ),
    ]
      .sort((left, right) => left - right)
      .slice(0, 12)
    fixedBreaksDraft = minutes.map(minutesToClock).join('\n')
    if (sameSequence(minutes, settings.fixed_break_minutes ?? [])) return
    editSettings({ ...settings, fixed_break_minutes: minutes })
  }
  async function saveProfile(name: 'work' | 'home') {
    try {
      profiles = await api.saveSettingsProfile(name)
      saved = true
      clearTimeout(savedTimer)
      savedTimer = setTimeout(() => (saved = false), 1600)
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function applyProfile(name: 'work' | 'home') {
    try {
      error = ''
      const next = await api.applySettingsProfile(name)
      applyAppearance(next)
      settings = next
      confirmed = next
      syncMessageDrafts(next)
      state = await api.getState()
      activeProfile = name
      announce = t('profile_applied', {
        name: t(name === 'work' ? 'profile_work' : 'profile_home'),
      })
    } catch (e) {
      error = errorMessage(e)
    }
  }
  const profileMenuItems = () =>
    Array.from(profileMenuEl?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])
  const toggleProfileMenu = async () => {
    profileMenuOpen = !profileMenuOpen
    if (profileMenuOpen) {
      await tick()
      profileMenuItems()
        .find((item) => !item.disabled)
        ?.focus()
    }
  }
  const closeProfileMenu = (refocus = false) => {
    if (!profileMenuOpen) return
    profileMenuOpen = false
    if (refocus) profileTrigger?.focus()
  }
  const chooseProfile = (name: 'work' | 'home') => {
    closeProfileMenu()
    void applyProfile(name)
  }
  const onProfileMenuKeydown = (event: KeyboardEvent) => {
    if (!profileMenuOpen) return
    const items = profileMenuItems()
    const index = items.indexOf(document.activeElement as HTMLButtonElement)
    if (event.key === 'Escape') {
      event.preventDefault()
      closeProfileMenu(true)
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      items[(index + 1) % items.length]?.focus()
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      items[(index - 1 + items.length) % items.length]?.focus()
    }
  }
  async function setContext(context: ContextReason | null, durationMinutes?: number) {
    try {
      error = ''
      state = await api.setContext(context, context ? durationMinutes : undefined)
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function testReminder() {
    try {
      error = ''
      await api.testReminder()
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function previewSystemSound(sound: SystemSound) {
    try {
      error = ''
      await api.previewSystemSound(sound)
    } catch (e) {
      error = errorMessage(e)
    }
  }

  /**
   * Writes clipboard-item data whose payload is still in flight, without losing the click's
   * transient user activation. `navigator.clipboard.write` must be called synchronously (no
   * `await` beforehand) — WebKit revokes clipboard permission the instant the task yields, which
   * is exactly what awaiting the Tauri IPC round-trip first would do. The Async Clipboard API
   * allows a ClipboardItem's representation to be a still-pending Promise for this reason.
   */
  function copyWhenReady(pending: Promise<string>): Promise<void> | null {
    if (typeof ClipboardItem === 'undefined' || !navigator.clipboard?.write) return null
    return navigator.clipboard.write([
      new ClipboardItem({
        'text/plain': pending.then((value) => new Blob([value], { type: 'text/plain' })),
      }),
    ])
  }
  async function copyResolvedText(text: string): Promise<boolean> {
    if (!navigator.clipboard?.writeText) return false
    try {
      await navigator.clipboard.writeText(text)
      return true
    } catch {
      return false
    }
  }
  async function exportHealthReport() {
    error = ''
    healthReportCopied = false
    const pending = api.getHealthReport()
    const clipboardWrite = copyWhenReady(pending)
    try {
      healthReport = await pending
    } catch (e) {
      error = errorMessage(e)
      return
    }
    if (clipboardWrite) {
      try {
        await clipboardWrite
        healthReportCopied = true
        return
      } catch {
        // Fall through to the direct writeText fallback below.
      }
    }
    healthReportCopied = await copyResolvedText(healthReport)
    if (!healthReportCopied) error = t('clipboard_manual_copy')
  }

  async function clearHistory() {
    if (!historyClearConfirmation) {
      historyClearConfirmation = true
      return
    }
    try {
      error = ''
      await api.clearHistory()
      history = []
      historyClearConfirmation = false
    } catch (e) {
      error = errorMessage(e)
    }
  }
  async function exportHistory(format: 'json' | 'csv') {
    error = ''
    historyExportCopied = false
    const pending = api.exportHistory(format)
    const clipboardWrite = copyWhenReady(pending)
    try {
      historyExport = await pending
    } catch (e) {
      error = errorMessage(e)
      return
    }
    if (clipboardWrite) {
      try {
        await clipboardWrite
        historyExportCopied = true
        return
      } catch {
        // Fall through to the direct writeText fallback below.
      }
    }
    historyExportCopied = await copyResolvedText(historyExport)
    if (!historyExportCopied) error = t('clipboard_manual_copy')
  }
  async function resetLocalData() {
    if (!resetLocalDataConfirmation) {
      resetLocalDataConfirmation = true
      return
    }
    try {
      error = ''
      const snapshot = await api.resetLocalData()
      state = snapshot
      const defaults = await api.getSettings()
      confirmed = defaults
      applyAppearance(defaults)
      settings = defaults
      syncMessageDrafts(defaults)
      history = []
      resetLocalDataConfirmation = false
      historyClearConfirmation = false
    } catch (e) {
      error = errorMessage(e)
    }
  }

  /** Settings and Analytics are peers, not a modal stack. */
  async function switchTo(next: Screen) {
    if (mode === next) return
    if (mode === 'settings') {
      commitBreakMessages()
      commitFixedBreakTimes()
      await flushSettings()
      // A stale "confirm" state must never survive leaving and reopening
      // Settings: the next click on the reset button would otherwise erase
      // all local data without the person having seen the confirmation.
      resetLocalDataConfirmation = false
    }
    if (next === 'history') {
      try {
        error = ''
        history = await api.getHistory()
        historyClearConfirmation = false
      } catch (e) {
        error = errorMessage(e)
        return
      }
    }
    mode = next
    announce =
      next === 'settings'
        ? t('settings_heading')
        : next === 'history'
          ? t('history_heading')
          : t('today_heading')
    await tick()
    appScroll?.scrollTo?.({ top: 0, behavior: 'instant' })
    if (next === 'settings') settingsRegion?.focus?.({ preventScroll: true })
    else if (next === 'history') settingsRegion?.focus?.({ preventScroll: true })
    else settingsButton?.focus?.({ preventScroll: true })
  }
  async function reviewAnalyticsSettings() {
    settingsCategory = 'breaks'
    await switchTo('settings')
  }
  function onKeydown(event: KeyboardEvent) {
    if (windowView !== 'main') return
    if ((event.metaKey || event.ctrlKey) && event.key === ',') {
      event.preventDefault()
      void switchTo('settings')
      return
    }
    if (event.key === 'Escape' && mode !== 'dashboard') {
      if ((event.target as HTMLElement | null)?.tagName === 'SELECT') return
      event.preventDefault()
      void switchTo('dashboard')
    }
  }

  onMount(() => {
    let offTick: (() => void) | undefined
    let offState: (() => void) | undefined
    let offBreakEnded: (() => void) | undefined
    let offBlinkNudge: (() => void) | undefined
    let offPostureNudge: (() => void) | undefined
    let offHydrationNudge: (() => void) | undefined
    void (async () => {
      try {
        // The nudge toast carries no timer state and no controls, so it needs
        // none of the state, settings, or event wiring the other views set up.
        if (nudgeToast) return
        if (windowView === 'main') {
          const initial = await Promise.all([
            api.getState(),
            api.getSettings(),
            api.getAutostartStatus(),
            api.getSettingsProfiles(),
            api.getDesktopHealth(),
            api.getOnboardingState(),
          ])
          let initialSettings = initial[1]
          const isFreshInstall = !initial[5]
          if (
            isFreshInstall &&
            typeof navigator !== 'undefined' &&
            navigator.language?.toLowerCase().startsWith('de')
          ) {
            initialSettings = { ...initialSettings, locale: 'de' }
          }
          applyAppearance(initialSettings)
          state = initial[0]
          settings = initialSettings
          confirmed = initialSettings
          syncMessageDrafts(initialSettings)
          autostartStatus = initial[2]
          profiles = initial[3]
          desktopHealth = initial[4]
          showOnboarding = isFreshInstall
          if (isMobileWearableHost()) await refreshWatchStatus()
        } else {
          const initial = await Promise.all([api.getState(), api.getSettings()])
          applyAppearance(initial[1])
          state = initial[0]
          settings = initial[1]
          confirmed = initial[1]
        }
        offTick = await api.onTick(
          (remaining) => state && (state = { ...state, remaining_seconds: remaining })
        )
        offState = await api.onState((next) => (state = next))
        // These are process-wide notification/announcement side effects, not
        // per-window UI. The main window's script keeps running even when
        // hidden to the tray, so it is the one place this must live — every
        // other window (overlay, prompt) would otherwise also
        // fire the same sound or announcement in parallel.
        if (windowView === 'main') {
          // The break cue is only ever heard once the pause has actually run
          // its course — never at break:started, and never at break:skipped,
          // which is an early exit rather than a completed pause.
          offBreakEnded = await api.onBreakEnded(
            () =>
              settings &&
              playBreakSound(settings.sound_theme ?? 'silence', settings.sound_volume ?? 70, 'end')
          )
          offBlinkNudge = await api.onBlinkNudge(() => (announce = t('nudge_blink_announcement')))
          offPostureNudge = await api.onPostureNudge(
            () => (announce = t('nudge_posture_announcement'))
          )
          offHydrationNudge = await api.onHydrationNudge(
            () => (announce = t('nudge_hydration_announcement'))
          )
          if (isMobileWearableHost()) {
            await api.syncWatchSettings()
            await refreshWatchStatus()
          }
        }
      } catch (e) {
        error = errorMessage(e)
      }
    })()
    return () => {
      offTick?.()
      offState?.()
      offBreakEnded?.()
      offBlinkNudge?.()
      offPostureNudge?.()
      offHydrationNudge?.()
    }
  })
  onDestroy(() => {
    clearTimeout(saveTimer)
    clearTimeout(savedTimer)
  })
  onMount(() => {
    if (!appWindow) return
    let offResized: (() => void) | undefined
    void (async () => {
      await refreshMaximizedState()
      offResized = await appWindow.onResized(() => void refreshMaximizedState())
    })()
    return () => offResized?.()
  })
  onMount(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null
      if (target && !profileMenuEl?.contains(target) && !profileTrigger?.contains(target)) {
        closeProfileMenu()
      }
    }
    window.addEventListener('pointerdown', onPointerDown)
    return () => window.removeEventListener('pointerdown', onPointerDown)
  })
</script>

<svelte:head><title>{t('app_title')}</title></svelte:head>
<svelte:window onkeydown={onKeydown} />

{#if nudgeToast}
  <NudgeToast nudge={nudgeKind} />
{:else if overlay}
  <BreakOverlay
    {state}
    {settings}
    {error}
    primary={overlayDisplay === 0}
    onDone={() => run(api.skipBreak)}
  />
{:else if prompt}
  <BreakPrompt
    {state}
    {settings}
    {error}
    onStart={() => run(api.startDueBreak)}
    onPostpone={() => run(api.postponeBreak)}
    onPauseFor={(minutes) => run(() => api.pauseForMinutes(minutes))}
  />
{:else}
  <main
    class="app-shell"
    class:mobile-shell={isMobileSettingsHost()}
    class:ios-safe-frame={isIOS}
    class:with-sidebar={mode !== 'dashboard'}
  >
    {#if isDesktopTitlebar && mode === 'dashboard'}
      <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>
      <button
        type="button"
        class="window-close-button"
        aria-label={t('window_close')}
        onclick={closeWindow}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    {/if}
    {#if mode !== 'dashboard' && !isMobileSettingsHost()}
      <nav class="sidebar" aria-label="PausIO">
        <div class="sidebar-brand" data-tauri-drag-region>
          <img
            class="brand-mark"
            src={pausioMark}
            alt=""
            aria-hidden="true"
            width="26"
            height="26"
          />
          <span class="brand-name" translate="no">PausIO</span>
        </div>
        <div class="sidebar-nav">
          <button
            class="sidebar-item"
            class:active={mode === 'history'}
            aria-current={mode === 'history' ? 'page' : undefined}
            aria-label={t('action_history')}
            onclick={() => switchTo('history')}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"
              ><path d="M4 20V11M10 20V4M16 20v-7M3 20h17" /></svg
            >
            <span>{t('action_history')}</span>
          </button>
          <button
            class="sidebar-item"
            class:active={mode === 'settings'}
            aria-current={mode === 'settings' ? 'page' : undefined}
            aria-label={t('settings_heading')}
            bind:this={settingsButton}
            aria-keyshortcuts={isApple ? 'Meta+,' : 'Control+,'}
            use:tooltip={{ label: t('settings_heading'), hint: isApple ? '⌘,' : 'Ctrl+,' }}
            onclick={() => switchTo('settings')}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"
              ><path
                d="M10.4 3.4h3.2l.6 2.2c.5.2.9.4 1.3.7l2.1-.7 1.6 2.8-1.6 1.5c.1.5.1 1 .1 1.5s0 1-.1 1.5l1.6 1.5-1.6 2.8-2.1-.7c-.4.3-.9.5-1.3.7l-.6 2.2h-3.2l-.6-2.2c-.5-.2-.9-.4-1.3-.7l-2.1.7-1.6-2.8 1.6-1.5a7.5 7.5 0 0 1 0-3L4.8 8.4l1.6-2.8 2.1.7c.4-.3.9-.5 1.3-.7l.6-2.2Z"
              /><circle cx="12" cy="12" r="2.7" /></svg
            >
            <span>{t('settings_heading')}</span>
          </button>
        </div>
        {#if mode === 'settings'}
          <div class="settings-search">
            <input
              type="search"
              aria-label={t('settings_search_label')}
              placeholder={t('settings_search_label')}
              bind:value={settingsQuery}
            />
            {#if settingsQuery.trim()}
              {@const results = searchSettings(settingsQuery)}
              <div
                class="settings-search-results"
                role="listbox"
                aria-label={t('settings_search_label')}
              >
                {#if results.length === 0}
                  <p class="settings-search-empty">{t('settings_search_no_results')}</p>
                {:else}
                  {#each results as result (result.entry.labelKey)}
                    {@const categoryLabel = t(
                      settingsCategories.find((c) => c.id === result.entry.category)?.labelKey ??
                        'section_breaks'
                    )}
                    <button
                      type="button"
                      role="option"
                      aria-selected="false"
                      onclick={() => selectSearchResult(result)}
                    >
                      <span>{result.label}</span>
                      <small
                        >{t(
                          result.entry.advanced
                            ? 'settings_search_in'
                            : 'settings_search_in_default',
                          {
                            category: categoryLabel,
                          }
                        )}</small
                      >
                    </button>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>
          <div class="sidebar-subnav" role="group" aria-label={t('settings_nav_label')}>
            {#each visibleSettingsCategories() as category (category.id)}
              <button
                class="sidebar-subitem"
                class:active={settingsCategory === category.id}
                aria-current={settingsCategory === category.id ? 'page' : undefined}
                onkeydown={onSettingsNavKeydown}
                onclick={() => selectSettingsCategory(category.id)}>{t(category.labelKey)}</button
              >
            {/each}
          </div>
        {/if}
      </nav>
    {/if}

    <div
      class="app-content"
      class:no-sidebar={mode === 'dashboard'}
      class:with-mobile-settings-nav={isMobileSettingsHost() && mode === 'settings'}
    >
      {#if showOnboarding && settings}
        <Onboarding
          {settings}
          {editSettings}
          {toggleDay}
          takeBreakNow={() => run(api.takeBreakNow)}
          onSkip={finishOnboarding}
          onFinish={finishOnboarding}
        />
      {/if}
      <!-- Covering the rest visually is not enough: without `inert`, the dashboard's
           headings and buttons stayed in the accessibility tree and tab order behind
           the overlay, reachable by a screen reader or keyboard user despite being
           hidden from sighted ones. -->
      <div class="app-content-body" inert={showOnboarding}>
        {#if mode !== 'dashboard'}
          <header class="app-header" data-tauri-drag-region>
            <h1 class="view-title" id="settings-title" data-tauri-drag-region>
              {mode === 'settings' ? t('settings_heading') : t('history_heading')}
            </h1>
            <div class="header-actions">
              {#if mode === 'settings'}
                <span class="settings-status" role="status" aria-live="polite">
                  {#if isSaving}{t('saving')}{:else if saved}{t('settings_saved')}{/if}
                </span>
              {/if}
              <button
                class="header-action"
                aria-label={t('settings_close')}
                onclick={() => switchTo('dashboard')}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M18 6L6 18M6 6l12 12" /></svg>
              </button>
            </div>
          </header>
        {/if}

        {#if isMobileSettingsHost() && mode === 'settings'}
          <nav class="mobile-settings-nav" aria-label={t('settings_nav_label')}>
            {#each visibleSettingsCategories() as category (category.id)}
              <button
                class:active={settingsCategory === category.id}
                aria-current={settingsCategory === category.id ? 'page' : undefined}
                onkeydown={onSettingsNavKeydown}
                onclick={() => selectSettingsCategory(category.id)}>{t(category.labelKey)}</button
              >
            {/each}
          </nav>
        {/if}

        <p class="sr-only" role="status" aria-live="polite">{announce}</p>

        <div class="app-scroll" bind:this={appScroll}>
          {#if error}
            <p class="notice notice-error" role="alert">
              <span aria-hidden="true">!</span>{error}<button
                aria-label={t('dismiss')}
                onclick={() => (error = '')}>×</button
              >
            </p>
          {/if}

          {#if mode === 'dashboard'}
            <section class="today" aria-busy={!state}>
              <div class="today-lead">
                <div class="today-intro">
                  <h1>{t('today_heading')}</h1>
                  <p class="today-subhead">{t('today_subheading')}</p>
                </div>
              </div>

              <div class="timer-stage tone-{phaseTone(state)}">
                <p class="phase-status">{phase(state)}</p>
                <TimerRing
                  fraction={remainingFraction(state)}
                  clock={state ? formatClock(state.remaining_seconds) : '--:--'}
                  subLabel={isBreaking(state) ? t('break_time_remaining') : t('until_next_pause')}
                  tone={dialTone(state)}
                />
                <p class="timer-caption">{timerCaption()}</p>
              </div>

              <div class="today-actions">
                <div class="timer-actions">
                  {#if isDormant(state)}
                    <button class="button button-primary" onclick={() => run(api.startSession)}
                      ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 7 8 5-8 5V7Z" /></svg
                      >{t('action_start_session')}</button
                    >
                  {:else if isPaused(state)}
                    <button
                      class="button button-primary"
                      aria-label={t('action_resume')}
                      onclick={() => run(api.resume)}
                      ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 7 8 5-8 5V7Z" /></svg
                      >{t('action_resume')}</button
                    >
                  {:else if canPause(state)}
                    <button class="button button-primary" onclick={() => run(api.takeBreakNow)}
                      ><svg viewBox="0 0 24 24" aria-hidden="true"
                        ><path d="M2 12s3.64-7 10-7 10 7 10 7-3.64 7-10 7S2 12 2 12z" /><circle
                          cx="12"
                          cy="12"
                          r="3"
                        /></svg
                      >{t('action_take_break')}</button
                    >
                    <button
                      class="button button-secondary"
                      aria-label={t('action_pause')}
                      onclick={() => run(api.pause)}
                      ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 7v10M15 7v10" /></svg
                      >{t('action_pause')}</button
                    >
                  {:else if isBreakDue(state)}
                    <button class="button button-primary" onclick={() => run(api.startDueBreak)}
                      ><svg viewBox="0 0 24 24" aria-hidden="true"
                        ><path d="M2 12s3.64-7 10-7 10 7 10 7-3.64 7-10 7S2 12 2 12z" /><circle
                          cx="12"
                          cy="12"
                          r="3"
                        /></svg
                      >{t('action_take_break')}</button
                    >
                    {#if (settings?.strictness ?? 'balanced') === 'balanced'}
                      <button class="button button-secondary" onclick={() => run(api.postponeBreak)}
                        >{t('break_postpone')}</button
                      >
                    {/if}
                  {:else if isBreaking(state)}
                    <button class="button button-secondary" onclick={() => run(api.skipBreak)}
                      >{t('break_end_early')}</button
                    >
                  {/if}
                </div>
                {#if isPaused(state) && pausedUntilLabel(state)}<p class="pause-resumes">
                    {pausedUntilLabel(state)}
                  </p>{/if}

                <dl class="daily-summary" aria-label={t('today_summary')}>
                  <div>
                    <dd class:zero={!state?.completed_short_breaks}>
                      {state?.completed_short_breaks ?? 0}
                    </dd>
                    <dt title={t('stat_short_breaks_hint')}>{t('stat_short_breaks')}</dt>
                  </div>
                  <div>
                    <dd class:zero={!state?.postpones_today}>{state?.postpones_today ?? 0}</dd>
                    <dt>{t('stat_postpones')}</dt>
                  </div>
                </dl>
              </div>
            </section>
          {:else if mode === 'settings' && settings}
            <SettingsPanel
              {settings}
              {settingsCategory}
              {autostartStatus}
              {isUpdatingAutostart}
              {desktopHealth}
              {isApple}
              isMobile={isMobileSettingsHost()}
              bind:settingsRegion
              bind:breakMessagesDraft
              bind:fixedBreaksDraft
              {resetLocalDataConfirmation}
              bind:diagnosticsOpen
              bind:advancedOpen
              {healthReport}
              {healthReportCopied}
              {editSettings}
              {setNumber}
              {toggleDay}
              {commitBreakMessages}
              {commitFixedBreakTimes}
              {testReminder}
              {previewSystemSound}
              {setAutostart}
              {...wearableProps()}
              {exportHealthReport}
              {resetLocalData}
              {profiles}
              {saveProfile}
              {applyProfile}
              {activeProfile}
              {state}
              {setContext}
              bind:contextMinutes
            />
          {:else if mode === 'history'}
            <HistoryPanel
              {history}
              {settings}
              bind:settingsRegion
              bind:historyRangeDays
              {historyClearConfirmation}
              {historyExport}
              {historyExportCopied}
              deviceLabel={analyticsDeviceLabel()}
              onReviewSettings={reviewAnalyticsSettings}
              onEnableHistory={() => void switchTo('settings')}
              {clearHistory}
              {exportHistory}
            />
          {/if}
        </div>

        {#if mode === 'dashboard'}
          <div class="dashboard-dock">
            <button
              class="dock-button"
              bind:this={settingsButton}
              aria-label={t('settings_heading')}
              aria-keyshortcuts={isApple ? 'Meta+,' : 'Control+,'}
              use:tooltip={{ label: t('settings_heading'), hint: isApple ? '⌘,' : 'Ctrl+,' }}
              onclick={() => switchTo('settings')}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true"
                ><path
                  d="M10.4 3.4h3.2l.6 2.2c.5.2.9.4 1.3.7l2.1-.7 1.6 2.8-1.6 1.5c.1.5.1 1 .1 1.5s0 1-.1 1.5l1.6 1.5-1.6 2.8-2.1-.7c-.4.3-.9.5-1.3.7l-.6 2.2h-3.2l-.6-2.2c-.5-.2-.9-.4-1.3-.7l-2.1.7-1.6-2.8 1.6-1.5a7.5 7.5 0 0 1 0-3L4.8 8.4l1.6-2.8 2.1.7c.4-.3.9-.5 1.3-.7l.6-2.2Z"
                /><circle cx="12" cy="12" r="2.7" /></svg
              >
            </button>
            <button
              class="dock-button"
              aria-label={t('action_history')}
              title={t('action_history')}
              onclick={() => switchTo('history')}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M3 12a9 9 0 1 0 3-6.7" />
                <path d="M3 4v5h5" />
                <path d="M12 7v5l3 2" />
              </svg>
            </button>
            <div class="profile-dropdown">
              <button
                class="profile-trigger"
                bind:this={profileTrigger}
                aria-haspopup="menu"
                aria-expanded={profileMenuOpen}
                aria-label={t('section_profiles')}
                onclick={() => void toggleProfileMenu()}
                onkeydown={onProfileMenuKeydown}
              >
                <span
                  >{activeProfile
                    ? t(`profile_${activeProfile}` as 'profile_work' | 'profile_home')
                    : t('section_profiles')}</span
                >
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
              </button>
              {#if profileMenuOpen}
                <div
                  class="profile-menu"
                  role="menu"
                  aria-label={t('section_profiles')}
                  tabindex="-1"
                  bind:this={profileMenuEl}
                  onkeydown={onProfileMenuKeydown}
                >
                  {#each ['work', 'home'] as profile (profile)}
                    {@const name = profile as 'work' | 'home'}
                    <button
                      role="menuitem"
                      class:active={activeProfile === name}
                      disabled={!profiles[name]}
                      onclick={() => chooseProfile(name)}
                      >{t(`profile_${name}` as 'profile_work' | 'profile_home')}</button
                    >
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    </div>
  </main>
{/if}
