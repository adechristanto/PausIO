<svelte:options runes={true} />

<script lang="ts">
  import Advanced from './Advanced.svelte'
  import ShortcutField from './ShortcutField.svelte'
  import {
    coveredDisplaysOf,
    deliveryModeOf,
    deliveryPatch,
    DELIVERY_MODES,
    MODE_TO_STRICTNESS,
    type DeliveryMode,
  } from '../lib/delivery'
  import { clockToMinutes, formatTimeOfDay, minutesToClock } from '../lib/format'
  import { applyPreset, PRESET_IDS } from '../lib/presets'
  import { isMinuteInActiveWindow, isRoundTheClock, roundTheClockPatch } from '../lib/schedule'
  import { describeSettings } from '../lib/summary'
  import { t, tCount, watchStateLabel } from '../lib/i18n'
  import { tooltip } from '../lib/tooltip'
  import type {
    Accent,
    AutostartStatus,
    BreakRoutine,
    ContextReason,
    DesktopHealth,
    DisplayTarget,
    Locale,
    NudgeResult,
    Settings,
    SettingsProfiles,
    Snapshot,
    SoundTheme,
    SystemSound,
    Theme,
    WatchStatus,
  } from '../lib/types'

  type SettingsCategory =
    'breaks' | 'schedule' | 'appearance' | 'shortcuts' | 'privacy' | 'wearables'

  interface Props {
    settings: Settings
    settingsCategory: SettingsCategory
    autostartStatus: AutostartStatus | null
    isUpdatingAutostart: boolean
    watchStatus?: WatchStatus | null
    nudgeResult?: NudgeResult | null
    desktopHealth: DesktopHealth | null
    isApple: boolean
    isMobile: boolean
    settingsRegion: HTMLElement | undefined
    breakMessagesDraft: string
    fixedBreaksDraft: string
    resetLocalDataConfirmation: boolean
    diagnosticsOpen: boolean
    advancedOpen: boolean
    healthReport: string
    healthReportCopied: boolean
    editSettings: (next: Settings) => void
    setNumber: (key: 'short_break_seconds' | 'long_break_seconds', value: number) => void
    toggleDay: (index: number) => void
    commitBreakMessages: () => void
    commitFixedBreakTimes: () => void
    testReminder: () => Promise<void>
    previewSystemSound: (sound: SystemSound) => Promise<void>
    setAutostart: (enabled: boolean) => Promise<void>
    syncWatchSettings?: () => Promise<void>
    sendTestNudge?: () => Promise<void>
    exportHealthReport: () => Promise<void>
    resetLocalData: () => Promise<void>
    profiles?: SettingsProfiles
    saveProfile?: (name: 'work' | 'home') => Promise<void>
    applyProfile?: (name: 'work' | 'home') => Promise<void>
    activeProfile?: 'work' | 'home' | null
    state?: Snapshot | null
    setContext?: (context: ContextReason | null, durationMinutes?: number) => Promise<void>
    contextMinutes?: number
  }
  let {
    settings,
    settingsCategory,
    autostartStatus,
    isUpdatingAutostart,
    watchStatus = null,
    nudgeResult = null,
    desktopHealth,
    isApple,
    isMobile,
    settingsRegion = $bindable(),
    breakMessagesDraft = $bindable(),
    fixedBreaksDraft = $bindable(),
    resetLocalDataConfirmation,
    diagnosticsOpen = $bindable(),
    advancedOpen = $bindable(false),
    healthReport,
    healthReportCopied,
    editSettings,
    setNumber,
    toggleDay,
    commitBreakMessages,
    commitFixedBreakTimes,
    testReminder,
    previewSystemSound,
    setAutostart,
    syncWatchSettings,
    sendTestNudge,
    exportHealthReport,
    resetLocalData,
    profiles = {},
    saveProfile,
    applyProfile,
    activeProfile = null,
    state = null,
    setContext,
    contextMinutes = $bindable(60),
  }: Props = $props()

  const appLocale = () => (settings?.locale === 'de' ? 'de-DE' : 'en-US')
  const intervalMinutes = () => (settings ? Math.round(settings.work_seconds / 60) : 20)
  const dayLabels = () => {
    const locale = settings?.locale === 'de' ? 'de-DE' : 'en-US'
    return Array.from({ length: 7 }, (_, index) =>
      new Intl.DateTimeFormat(locale, { weekday: 'short' }).format(new Date(2023, 0, 1 + index))
    )
  }
  const activeDayCount = (mask: number) => {
    let count = 0
    for (let i = 0; i < 7; i += 1) if ((mask & (1 << i)) !== 0) count += 1
    return count
  }
  const isOnlyActiveDay = (mask: number, index: number) =>
    (mask & (1 << index)) !== 0 && activeDayCount(mask) === 1
  // Each state has a different remedy, so they are reported separately rather
  // than collapsed into "unknown": banners turned off is a System Settings
  // toggle, an unsigned build needs a Team ID, and a denial needs a change of
  // mind. Breaks work regardless — see notificationFallsBack below.
  const permissionLabel = (state: string) =>
    state === 'granted' || state === 'denied' || state === 'prompt'
      ? t(`permission_${state}` as 'permission_granted' | 'permission_denied' | 'permission_prompt')
      : state === 'not_determined'
        ? t('permission_prompt')
        : state === 'alerts_off'
          ? t('permission_alerts_off')
          : state === 'unavailable'
            ? t('permission_unavailable')
            : t('permission_unknown')
  const notificationFallsBack = $derived(
    !!desktopHealth && desktopHealth.notification_permission !== 'granted'
  )
  const precisionLabel = (state: WatchStatus['reminder_precision']) =>
    state === 'exact'
      ? t('wearables_exact')
      : state === 'inexact'
        ? t('wearables_inexact')
        : t('wearables_precision_unavailable')

  const deliveryMode = $derived(deliveryModeOf(settings))
  const coversScreen = $derived(deliveryMode !== 'notify')
  const coveredDisplays = $derived(coveredDisplaysOf(settings))
  const modeHintKey = $derived(
    `strictness_${MODE_TO_STRICTNESS[deliveryMode]}_hint` as
      | 'strictness_gentle_hint'
      | 'strictness_balanced_hint'
      | 'strictness_firm_hint'
      | 'strictness_strict_hint'
  )
  // Only Firm and Strict produce no interim surface at Due (events.rs:494). Balanced
  // still sends an actionable "start now / postpone" notification, so it is not
  // warningless even with the pre-break notice off -- the caution belongs to the two
  // modes that genuinely cover the screen with nothing preceding it.
  const noAdvanceNotice = $derived(
    settings.pre_break_seconds === 0 && (deliveryMode === 'cover' || deliveryMode === 'hold')
  )
  const setDeliveryMode = (mode: DeliveryMode) =>
    editSettings({ ...settings, ...deliveryPatch(settings, mode) })
  const summarySentences = $derived(describeSettings(settings))

  // Every preset only ever patches timing/delivery/nudge fields (see lib/presets.ts),
  // so it cannot silently change a language, a theme, or a keyboard shortcut. It goes
  // through the same debounced `editSettings` path as any other edit, so the existing
  // Saving.../Saved indicator is the confirmation — no separate "applied" toast needed.
  const applyPresetTo = (id: (typeof PRESET_IDS)[number]) => editSettings(applyPreset(settings, id))

  // active_at() (settings.rs:405) treats an equal start and end as always active. That
  // is a real, useful "no scheduled hours" mode, but a person would have to notice two
  // matching clock values to find it. Naming it turns a quirk into a feature.
  const roundTheClock = $derived(
    isRoundTheClock(settings.active_start_minutes, settings.active_end_minutes)
  )
  const toggleRoundTheClock = (checked: boolean) =>
    editSettings({ ...settings, ...roundTheClockPatch(checked) })

  // A fixed break only fires while the phase is Working/PreBreak (engine.rs:350), which
  // active hours gate independently of the day mask. One outside the window silently
  // never happens; saying so before it's saved beats a feature that quietly does nothing.
  const offendingFixedBreaks = $derived(
    (settings.fixed_break_minutes ?? [])
      .filter(
        (minute) =>
          !roundTheClock &&
          !isMinuteInActiveWindow(
            minute,
            settings.active_start_minutes,
            settings.active_end_minutes
          )
      )
      .map((minute) => minutesToClock(minute))
  )
</script>

<section class="settings" bind:this={settingsRegion} tabindex="-1" aria-labelledby="settings-title">
  {#if settingsCategory === 'breaks'}
    <section class="settings-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="7" /><path d="M12 8v4l3 2" /></svg
          ></span
        >
        <div>
          <h2>{t('section_breaks')}</h2>
          <p>{t('section_breaks_hint')}</p>
        </div>
      </div>

      <div class="preset-row">
        <span class="preset-row-label">{t('presets_heading')}</span>
        <div>
          {#each PRESET_IDS as id (id)}
            <button type="button" class="preset-button" onclick={() => applyPresetTo(id)}
              >{t(
                `preset_${id}` as
                  | 'preset_classic'
                  | 'preset_gentle'
                  | 'preset_focus_blocks'
                  | 'preset_eye_strain_recovery'
              )}</button
            >
          {/each}
        </div>
      </div>

      <div class="settings-summary" aria-live="polite">
        <strong>{t('summary_heading')}</strong>
        <p>{summarySentences.join(' ')}</p>
      </div>

      <label class="range-control">
        <span>{t('setting_work_interval')}</span>
        <output>{t('unit_minutes', { value: intervalMinutes() })}</output>
        <input
          aria-label={t('setting_work_interval')}
          type="range"
          min="300"
          max="7200"
          step="60"
          value={settings.work_seconds}
          oninput={(e) =>
            editSettings({ ...settings!, work_seconds: Number(e.currentTarget.value) })}
        />
      </label>

      <div class="setting-list">
        <div class="strictness-row">
          <label for="delivery-mode">{t('setting_delivery_mode')}</label>
          <select
            id="delivery-mode"
            class="strictness-select"
            aria-describedby="delivery-mode-hint"
            value={deliveryMode}
            onchange={(event) => setDeliveryMode(event.currentTarget.value as DeliveryMode)}
          >
            {#each DELIVERY_MODES as mode}
              <option value={mode}
                >{t(
                  `delivery_mode_${mode}` as
                    | 'delivery_mode_notify'
                    | 'delivery_mode_ask'
                    | 'delivery_mode_cover'
                    | 'delivery_mode_hold'
                )}</option
              >
            {/each}
          </select>
          <small id="delivery-mode-hint">{t(modeHintKey)}</small>
          {#if noAdvanceNotice}
            <small class="setting-warning" role="alert">{t('delivery_no_notice_warning')}</small>
          {/if}
        </div>
        {#if !isMobile && coversScreen}
          <label class="select-row">
            <span>{t('setting_display_target')}</span>
            <select
              value={coveredDisplays}
              onchange={(event) =>
                editSettings({
                  ...settings!,
                  display_target: event.currentTarget.value as DisplayTarget,
                })}
            >
              {#each ['all', 'active', 'primary'] as target}
                <option value={target}
                  >{t(
                    `display_${target}` as 'display_all' | 'display_active' | 'display_primary'
                  )}</option
                >
              {/each}
            </select>
          </label>
        {/if}
        {#if deliveryMode === 'ask'}
          <label class="select-row">
            <span>{t('setting_postpone_limit')}</span>
            <select
              value={settings.postpone_limit ?? ''}
              onchange={(e) =>
                editSettings({
                  ...settings!,
                  postpone_limit:
                    e.currentTarget.value === '' ? null : Number(e.currentTarget.value),
                })}
            >
              <option value="">{t('postpone_unlimited')}</option>
              {#each [1, 2, 3, 5, 10] as limit}<option value={limit}
                  >{tCount('unit_postpones', limit)}</option
                >{/each}
            </select>
          </label>
        {/if}
      </div>

      <div class="section-subheading">
        <h3>{t('section_sound')}</h3>
        {#if desktopHealth?.platform === 'windows'}
          <p class="setting-note">{t('sound_windows_note')}</p>
        {/if}
      </div>
      <div class="setting-list">
        {#if !isMobile}
          <label class="toggle-row">
            <span
              ><strong>{t('setting_notification_sound')}</strong><small
                >{t('setting_notification_sound_hint')}</small
              ></span
            >
            <input
              type="checkbox"
              role="switch"
              checked={settings.notification_sound ?? false}
              onchange={(event) =>
                editSettings({ ...settings!, notification_sound: event.currentTarget.checked })}
            />
          </label>
          {#if settings.notification_sound ?? false}
            <label class="select-row">
              <span>{t('setting_notification_sound_name')}</span>
              <select
                value={settings.notification_sound_name ?? 'default'}
                onchange={(event) =>
                  editSettings({
                    ...settings!,
                    notification_sound_name: event.currentTarget.value as SystemSound,
                  })}
              >
                {#each ['default', 'chime', 'ding', 'alert', 'complete'] as name}
                  <option value={name}
                    >{t(
                      `system_sound_${name}` as
                        | 'system_sound_default'
                        | 'system_sound_chime'
                        | 'system_sound_ding'
                        | 'system_sound_alert'
                        | 'system_sound_complete'
                    )}</option
                  >
                {/each}
              </select>
              <button
                type="button"
                class="text-button"
                onclick={() => previewSystemSound(settings!.notification_sound_name ?? 'default')}
                >{t('action_test_sound')}</button
              >
            </label>
          {/if}
        {/if}
        <label class="select-row">
          <span>{t('setting_sound_theme')}</span>
          <select
            value={settings.sound_theme ?? 'silence'}
            onchange={(event) =>
              editSettings({
                ...settings!,
                sound_theme: event.currentTarget.value as SoundTheme,
              })}
          >
            {#each ['silence', 'chime', 'tone', 'click'] as theme}
              <option value={theme}
                >{t(
                  `sound_theme_${theme}` as
                    | 'sound_theme_silence'
                    | 'sound_theme_chime'
                    | 'sound_theme_tone'
                    | 'sound_theme_click'
                )}</option
              >
            {/each}
          </select>
        </label>
        {#if (settings.sound_theme ?? 'silence') !== 'silence'}
          <label class="range-control">
            <span>{t('setting_sound_volume')}</span>
            <output>{t('unit_percent', { value: settings.sound_volume ?? 70 })}</output>
            <input
              aria-label={t('setting_sound_volume')}
              type="range"
              min="0"
              max="100"
              step="5"
              value={settings.sound_volume ?? 70}
              oninput={(e) =>
                editSettings({ ...settings!, sound_volume: Number(e.currentTarget.value) })}
            />
          </label>
        {/if}
      </div>

      <Advanced bind:open={advancedOpen}>
        <div class="stepper-row">
          <div>
            <span>{t('setting_eye_break')}</span><small>{t('setting_eye_break_hint')}</small>
          </div>
          <div class="stepper">
            <button
              aria-label={t('decrease', { item: t('setting_eye_break') })}
              disabled={settings.short_break_seconds <= 5}
              onclick={() =>
                setNumber('short_break_seconds', Math.max(5, settings!.short_break_seconds - 5))}
              >−</button
            >
            <strong>{settings.short_break_seconds}<small>{t('unit_seconds_short')}</small></strong>
            <button
              aria-label={t('increase', { item: t('setting_eye_break') })}
              disabled={settings.short_break_seconds >= 120}
              onclick={() =>
                setNumber('short_break_seconds', Math.min(120, settings!.short_break_seconds + 5))}
              >+</button
            >
          </div>
        </div>
        <div class="stepper-row long-break-row">
          <div>
            <span>{t('setting_longer_breaks')}</span><small>{t('setting_longer_breaks_hint')}</small
            >
          </div>
          <div class="long-break-controls">
            <select
              value={settings.long_break_every ?? ''}
              onchange={(e) =>
                editSettings({
                  ...settings!,
                  long_break_every:
                    e.currentTarget.value === '' ? null : Number(e.currentTarget.value),
                })}
            >
              <option value="">{t('long_break_off')}</option>
              {#each [2, 3, 4, 5, 6, 7, 8] as cadence}<option value={cadence}
                  >{t('long_break_cadence', { value: cadence })}</option
                >{/each}
            </select>
            {#if settings.long_break_every}
              <div class="stepper">
                <button
                  aria-label={t('decrease', { item: t('setting_longer_breaks') })}
                  disabled={settings.long_break_seconds <= 60}
                  onclick={() =>
                    setNumber(
                      'long_break_seconds',
                      Math.max(60, settings!.long_break_seconds - 60)
                    )}>−</button
                >
                <strong
                  >{Math.round(settings.long_break_seconds / 60)}<small
                    >{t('unit_minutes_short')}</small
                  ></strong
                >
                <button
                  aria-label={t('increase', { item: t('setting_longer_breaks') })}
                  disabled={settings.long_break_seconds >= 1800}
                  onclick={() =>
                    setNumber(
                      'long_break_seconds',
                      Math.min(1800, settings!.long_break_seconds + 60)
                    )}>+</button
                >
              </div>
            {/if}
          </div>
        </div>
        <label class="select-row">
          <span>{t('setting_warning')}</span>
          <select
            value={settings.pre_break_seconds}
            onchange={(e) =>
              editSettings({
                ...settings!,
                pre_break_seconds: Number(e.currentTarget.value),
              })}
          >
            {#each [0, 10, 30, 60] as warning}<option value={warning}
                >{warning === 0
                  ? t('warning_off')
                  : t('unit_seconds_value', { value: warning })}</option
              >{/each}
          </select>
        </label>
        <label class="select-row">
          <span>{t('setting_blink_nudge')}</span>
          <select
            value={settings.blink_nudge_minutes ?? ''}
            onchange={(event) =>
              editSettings({
                ...settings!,
                blink_nudge_minutes:
                  event.currentTarget.value === '' ? null : Number(event.currentTarget.value),
              })}
          >
            <option value="">{t('blink_off')}</option>
            {#each [5, 10, 15, 20, 30, 45, 60] as minutes}<option value={minutes}
                >{t('unit_minutes', { value: minutes })}</option
              >{/each}
          </select>
        </label>
        <label class="select-row">
          <span>{t('setting_posture_nudge')}</span>
          <select
            value={settings.posture_nudge_minutes ?? ''}
            onchange={(event) =>
              editSettings({
                ...settings!,
                posture_nudge_minutes:
                  event.currentTarget.value === '' ? null : Number(event.currentTarget.value),
              })}
          >
            <option value="">{t('posture_off')}</option>
            {#each [15, 20, 30, 45, 60, 90, 120] as minutes}<option value={minutes}
                >{t('unit_minutes', { value: minutes })}</option
              >{/each}
          </select>
        </label>
        <label class="select-row">
          <span>{t('setting_hydration_nudge')}</span>
          <select
            value={settings.hydration_nudge_minutes ?? ''}
            onchange={(event) =>
              editSettings({
                ...settings!,
                hydration_nudge_minutes:
                  event.currentTarget.value === '' ? null : Number(event.currentTarget.value),
              })}
          >
            <option value="">{t('hydration_off')}</option>
            {#each [15, 20, 30, 45, 60, 90, 120] as minutes}<option value={minutes}
                >{t('unit_minutes', { value: minutes })}</option
              >{/each}
          </select>
        </label>
      </Advanced>
    </section>
  {/if}

  {#if settingsCategory === 'schedule'}
    <section class="settings-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><rect x="5" y="4" width="14" height="16" rx="2" /><path d="M8 2v4m8-4v4M5 9h14" /></svg
          ></span
        >
        <div>
          <h2>{t('section_schedule')}</h2>
          <p>{t('section_schedule_hint')}</p>
        </div>
      </div>
      <fieldset class="day-picker">
        <legend>{t('setting_active_days')}</legend>
        <div>
          {#each dayLabels() as label, index}
            <button
              class:chosen={(settings.active_days_mask & (1 << index)) !== 0}
              aria-pressed={(settings.active_days_mask & (1 << index)) !== 0}
              aria-disabled={isOnlyActiveDay(settings.active_days_mask, index)}
              use:tooltip={{
                label: t('tooltip_last_day'),
                disabled: !isOnlyActiveDay(settings.active_days_mask, index),
              }}
              onclick={() => toggleDay(index)}>{label}</button
            >
          {/each}
        </div>
      </fieldset>
      <label class="toggle-row">
        <span><strong>{t('setting_round_the_clock')}</strong></span>
        <input
          type="checkbox"
          role="switch"
          checked={roundTheClock}
          onchange={(event) => toggleRoundTheClock(event.currentTarget.checked)}
        />
      </label>
      {#if !roundTheClock}
        <div class="time-grid">
          <label>
            <span>{t('setting_start_time')}</span>
            <input
              aria-label={t('setting_start_time')}
              type="time"
              lang={appLocale()}
              value={minutesToClock(settings.active_start_minutes)}
              oninput={(e) => {
                const value = clockToMinutes(e.currentTarget.value)
                if (Number.isFinite(value))
                  editSettings({ ...settings!, active_start_minutes: value })
              }}
            />
          </label>
          <label>
            <span>{t('setting_end_time')}</span>
            <input
              aria-label={t('setting_end_time')}
              type="time"
              lang={appLocale()}
              value={minutesToClock(settings.active_end_minutes)}
              oninput={(e) => {
                const value = clockToMinutes(e.currentTarget.value)
                if (Number.isFinite(value))
                  editSettings({ ...settings!, active_end_minutes: value })
              }}
            />
          </label>
        </div>
      {/if}

      <Advanced bind:open={advancedOpen}>
        {#if setContext}
          <label class="select-row">
            <span>{t('setting_context')}</span>
            <select
              aria-label={t('setting_context')}
              value={state?.context ?? ''}
              onchange={(event) =>
                void setContext(
                  (event.currentTarget.value || null) as ContextReason | null,
                  contextMinutes
                )}
            >
              <option value="">{t('context_none')}</option>
              {#each ['meeting', 'screen_share', 'fullscreen', 'do_not_disturb', 'active_input'] as context}
                <option value={context}
                  >{t(
                    `context_${context}` as
                      | 'context_meeting'
                      | 'context_screen_share'
                      | 'context_fullscreen'
                      | 'context_do_not_disturb'
                      | 'context_active_input'
                  )}</option
                >
              {/each}
            </select>
          </label>
          {#if state?.context}
            <label class="select-row">
              <span>{t('setting_context_duration')}</span>
              <select
                aria-label={t('setting_context_duration')}
                value={contextMinutes}
                onchange={(event) => {
                  contextMinutes = Number(event.currentTarget.value)
                  void setContext(state?.context ?? null, contextMinutes || undefined)
                }}
              >
                {#each [15, 30, 60, 120] as minutes}<option value={minutes}
                    >{t('unit_minutes', { value: minutes })}</option
                  >{/each}
                <option value="0">{t('context_until_cleared')}</option>
              </select>
            </label>
            {#if state.context_expires_at}<p class="setting-note">
                {t('context_expires', {
                  time: formatTimeOfDay(new Date(state.context_expires_at), settings?.locale),
                })}
              </p>{/if}
          {/if}
        {/if}
        {#if desktopHealth?.auto_context_supported}
          <label class="toggle-row">
            <span
              ><strong>{t('setting_auto_detect_fullscreen')}</strong><small
                >{t('setting_auto_detect_fullscreen_hint')}</small
              ></span
            >
            <input
              type="checkbox"
              role="switch"
              checked={settings.auto_detect_fullscreen ?? false}
              onchange={(event) =>
                editSettings({
                  ...settings!,
                  auto_detect_fullscreen: event.currentTarget.checked,
                })}
            />
          </label>
          <label class="toggle-row">
            <span
              ><strong>{t('setting_auto_detect_dnd')}</strong><small
                >{t('setting_auto_detect_dnd_hint')}</small
              ></span
            >
            <input
              type="checkbox"
              role="switch"
              checked={settings.auto_detect_do_not_disturb ?? false}
              onchange={(event) =>
                editSettings({
                  ...settings!,
                  auto_detect_do_not_disturb: event.currentTarget.checked,
                })}
            />
          </label>
        {:else if desktopHealth}
          <!-- auto_context_supported is cfg!(target_os = "windows") (commands.rs:461), so
               these toggles simply vanished on macOS and Linux. Saying so is better than
               leaving a person to wonder whether the feature exists and they cannot find it. -->
          <p class="setting-note">{t('auto_detect_unsupported')}</p>
        {/if}
        <label class="message-row">
          <span>{t('setting_fixed_breaks')}</span>
          <small>{t('setting_fixed_breaks_hint')}</small>
          <textarea
            aria-label={t('setting_fixed_breaks')}
            placeholder={'12:30, 15:00'}
            bind:value={fixedBreaksDraft}
            onblur={commitFixedBreakTimes}></textarea>
          {#each offendingFixedBreaks as time (time)}
            <small class="setting-warning" role="alert">
              {t('fixed_break_outside_hours_warning', { time })}
            </small>
          {/each}
        </label>
        <label class="select-row">
          <span>{t('setting_daily_focus_limit')}</span>
          <select
            value={settings.daily_focus_limit_minutes ?? ''}
            onchange={(event) =>
              editSettings({
                ...settings!,
                daily_focus_limit_minutes:
                  event.currentTarget.value === '' ? null : Number(event.currentTarget.value),
              })}
          >
            <option value="">{t('daily_focus_limit_off')}</option>
            {#each [60, 120, 180, 240, 360, 480] as minutes}<option value={minutes}
                >{t('unit_minutes', { value: minutes })}</option
              >{/each}
          </select>
        </label>
      </Advanced>
    </section>

    <section class="settings-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" /><circle
              cx="12"
              cy="7"
              r="4"
            /></svg
          ></span
        >
        <div>
          <h2>{t('section_profiles')}</h2>
          <p>{t('section_profiles_hint')}</p>
        </div>
      </div>
      <div class="profile-grid">
        {#each ['work', 'home'] as profile}
          {@const name = profile as 'work' | 'home'}
          <div>
            <strong>{t(`profile_${name}` as 'profile_work' | 'profile_home')}</strong>
            <small>{profiles[name] ? t('profile_saved') : t('profile_empty')}</small>
            <span>
              <button class="text-button" onclick={() => void saveProfile?.(name)}
                >{t('profile_save_as', {
                  name: t(`profile_${name}` as 'profile_work' | 'profile_home'),
                })}</button
              >
              <button
                class="text-button"
                disabled={!profiles[name]}
                onclick={() => void applyProfile?.(name)}>{t('profile_apply')}</button
              >
            </span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if settingsCategory === 'appearance'}
    <section class="settings-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><path d="M12 3a9 9 0 1 0 9 9c0-5-4-9-9-9Z" /><path
              d="M12 3c-2.5 2.5-3.7 5.5-3.7 9S9.5 18.5 12 21"
            /><path d="M3 12h18" /></svg
          ></span
        >
        <div>
          <h2>{t('section_appearance')}</h2>
          <p>{t('section_appearance_hint')}</p>
        </div>
      </div>
      <div class="setting-list">
        <label class="select-row">
          <span>{t('setting_language')}</span>
          <select
            value={settings.locale ?? 'en'}
            onchange={(event) =>
              editSettings({ ...settings!, locale: event.currentTarget.value as Locale })}
          >
            <option value="en">{t('language_en')}</option>
            <option value="de">{t('language_de')}</option>
          </select>
        </label>
        <label class="select-row">
          <span>{t('setting_theme')}</span>
          <select
            value={settings.theme ?? 'system'}
            onchange={(event) =>
              editSettings({ ...settings!, theme: event.currentTarget.value as Theme })}
          >
            <option value="system">{t('theme_system')}</option>
            <option value="light">{t('theme_light')}</option>
            <option value="dark">{t('theme_dark')}</option>
          </select>
        </label>
        <div class="select-row">
          <span>{t('setting_accent')}</span>
          <div class="accent-picker" role="radiogroup" aria-label={t('setting_accent')}>
            {#each ['horizon', 'sage', 'amber', 'lilac'] as accent}
              {@const label = t(
                `accent_${accent}` as
                  'accent_horizon' | 'accent_sage' | 'accent_amber' | 'accent_lilac'
              )}
              <button
                type="button"
                class="accent-swatch"
                data-accent={accent}
                role="radio"
                aria-checked={(settings.accent ?? 'horizon') === accent}
                aria-label={label}
                use:tooltip={{ label }}
                onclick={() => editSettings({ ...settings!, accent: accent as Accent })}
              ></button>
            {/each}
          </div>
        </div>
      </div>

      <Advanced bind:open={advancedOpen}>
        <label class="select-row">
          <span>{t('setting_routine')}</span>
          <select
            value={settings.break_routine ?? 'guided'}
            onchange={(event) =>
              editSettings({
                ...settings!,
                break_routine: event.currentTarget.value as BreakRoutine,
              })}
          >
            {#each ['guided', 'quiet', 'far_gaze', 'blink', 'posture'] as routine}
              <option value={routine}
                >{t(
                  `routine_${routine}` as
                    | 'routine_guided'
                    | 'routine_quiet'
                    | 'routine_far_gaze'
                    | 'routine_blink'
                    | 'routine_posture'
                )}</option
              >
            {/each}
          </select>
        </label>
        <label class="message-row">
          <span>{t('setting_messages')}</span>
          <small>{t('setting_messages_hint')}</small>
          <textarea
            aria-label={t('setting_messages')}
            maxlength="1451"
            bind:value={breakMessagesDraft}
            onblur={commitBreakMessages}></textarea>
        </label>
        <label class="toggle-row">
          <span
            ><strong>{t('setting_show_clock')}</strong><small>{t('setting_show_clock_hint')}</small
            ></span
          >
          <input
            type="checkbox"
            role="switch"
            checked={settings.show_clock_in_break ?? false}
            onchange={(event) =>
              editSettings({
                ...settings!,
                show_clock_in_break: event.currentTarget.checked,
              })}
          />
        </label>
      </Advanced>
    </section>
  {/if}

  {#if settingsCategory === 'shortcuts' && !isMobile}
    <section class="settings-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><rect x="4" y="5" width="16" height="14" rx="2" /><path
              d="M8 10h.01M12 10h.01M16 10h.01M8 14h8"
            /></svg
          ></span
        >
        <div>
          <h2>{t('section_shortcuts')}</h2>
          <p>{t('section_shortcuts_hint')}</p>
        </div>
      </div>
      <div class="setting-list">
        <ShortcutField
          label={t('setting_end_break_shortcut')}
          value={settings.end_break_shortcut ?? null}
          onChange={(next) => editSettings({ ...settings!, end_break_shortcut: next })}
        />
        <ShortcutField
          label={t('setting_pause_toggle_shortcut')}
          value={settings.pause_toggle_shortcut ?? null}
          onChange={(next) => editSettings({ ...settings!, pause_toggle_shortcut: next })}
        />
        <ShortcutField
          label={t('setting_take_break_shortcut')}
          value={settings.take_break_shortcut ?? null}
          onChange={(next) => editSettings({ ...settings!, take_break_shortcut: next })}
        />
      </div>
      {#if autostartStatus?.supported}
        <label class="toggle-row">
          <span
            ><strong>{t('setting_start_at_login')}</strong><small
              >{t('setting_start_at_login_hint')}</small
            ></span
          >
          <input
            type="checkbox"
            role="switch"
            aria-label={t('setting_start_at_login')}
            checked={autostartStatus.enabled}
            disabled={isUpdatingAutostart}
            onchange={(event) => void setAutostart(event.currentTarget.checked)}
          />
        </label>
      {/if}
    </section>
  {/if}

  {#if settingsCategory === 'privacy'}
    <section class="settings-section privacy-section">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><path d="m12 3 7 3v5c0 4.4-3 8.3-7 10-4-1.7-7-5.6-7-10V6l7-3Z" /></svg
          ></span
        >
        <div>
          <h2>{t('section_history_privacy')}</h2>
          <p>{t('section_history_privacy_hint')}</p>
        </div>
      </div>
      <label class="toggle-row">
        <span><strong>{t('setting_history_enabled')}</strong></span>
        <input
          type="checkbox"
          role="switch"
          checked={settings.history_enabled ?? false}
          onchange={(event) =>
            editSettings({ ...settings!, history_enabled: event.currentTarget.checked })}
        />
      </label>
      {#if settings.history_enabled}
        <label class="select-row">
          <span>{t('setting_history_retention')}</span>
          <select
            value={settings.history_retention_days ?? ''}
            onchange={(event) =>
              editSettings({
                ...settings!,
                history_retention_days:
                  event.currentTarget.value === '' ? null : Number(event.currentTarget.value),
              })}
          >
            {#each [30, 90, 365] as days}<option value={days}
                >{t('history_days', { value: days })}</option
              >{/each}
            <option value="">{t('history_unlimited')}</option>
          </select>
        </label>
      {/if}

      <Advanced bind:open={advancedOpen}>
        <div class="advanced-block">
          <p>{t('privacy_reset_hint')}</p>
          <button class="button button-danger" onclick={resetLocalData}
            >{resetLocalDataConfirmation ? t('privacy_reset_confirm') : t('privacy_reset')}</button
          >
        </div>
        {#if !isMobile}
          <section class="diagnostics">
            <button
              class="diagnostics-trigger"
              aria-expanded={diagnosticsOpen}
              onclick={() => (diagnosticsOpen = !diagnosticsOpen)}
            >
              <span
                ><i
                  class:available={desktopHealth?.notification_permission === 'granted'}
                  aria-hidden="true"
                ></i>{t('diagnostics_heading')}</span
              >
              <svg class:open={diagnosticsOpen} viewBox="0 0 24 24" aria-hidden="true"
                ><path d="m7 10 5 5 5-5" /></svg
              >
            </button>
            {#if diagnosticsOpen}
              <div class="diagnostics-content">
                <p>
                  {t('diagnostics_permission', {
                    state: desktopHealth
                      ? permissionLabel(desktopHealth.notification_permission)
                      : t('diagnostics_checking'),
                  })}
                </p>
                {#if notificationFallsBack}
                  <!-- Notifications are a courtesy, not the mechanism: say so, so an
                     unavailable permission does not read as a broken timer. -->
                  <p>{t('diagnostics_notification_fallback')}</p>
                {/if}
                <p>
                  {t('diagnostics_displays', {
                    count: desktopHealth ? desktopHealth.display_count : t('diagnostics_checking'),
                  })}
                </p>
                <div>
                  <button class="text-button" onclick={testReminder}
                    >{t('diagnostics_test_reminder')}</button
                  >
                  <button class="text-button" onclick={exportHealthReport}
                    >{t('diagnostics_export')}</button
                  >
                </div>
                {#if healthReport}<textarea
                    class="health-report"
                    readonly
                    aria-label={t('diagnostics_export')}
                    value={healthReport}></textarea>{#if healthReportCopied}<p>
                      {t('diagnostics_copied')}
                    </p>{/if}{/if}
              </div>
            {/if}
          </section>
        {/if}
      </Advanced>
    </section>
  {/if}

  {#if settingsCategory === 'wearables' && isMobile && syncWatchSettings}
    <section class="settings-section wearable-settings" aria-labelledby="wearables-heading">
      <div class="section-heading">
        <span class="section-icon" aria-hidden="true"
          ><svg viewBox="0 0 24 24"
            ><rect x="7" y="5" width="10" height="14" rx="3" /><path
              d="M9 2h6l1 3H8l1-3Zm0 20h6l1-3H8l1 3Z"
            /><path d="M10 11h4" /></svg
          ></span
        >
        <div>
          <h2 id="wearables-heading">{t('wearables_heading')}</h2>
          <p>{t('wearables_hint')}</p>
        </div>
      </div>
      <div class="setting-list wearable-status-list">
        <div class="wearable-connection" class:connected={watchStatus?.reachable}>
          <span class="wearable-connection-dot" aria-hidden="true"></span>
          <strong>{t('watch_status', { state: watchStateLabel(watchStatus) })}</strong>
        </div>
        <p>
          {t('wearables_permission', {
            state: permissionLabel(watchStatus?.notification_permission ?? 'unknown'),
          })}
        </p>
        <p>
          {t('wearables_precision', {
            state: precisionLabel(watchStatus?.reminder_precision),
          })}
        </p>
        <p>
          {t('wearables_horizon', {
            value: watchStatus?.schedule_horizon_at
              ? new Date(watchStatus.schedule_horizon_at).toLocaleString(appLocale())
              : t('watch_unknown_revision'),
          })}
        </p>
        {#if watchStatus?.last_error}<p>
            {t('wearables_degraded', { value: watchStatus.last_error })}
          </p>{/if}
        {#if nudgeResult}<p>
            {t('watch_nudge_result', {
              value: t(
                `nudge_result_${nudgeResult}` as
                  'nudge_result_delivered' | 'nudge_result_queued' | 'nudge_result_unavailable'
              ),
            })}
          </p>{/if}
        <div class="wearable-actions">
          <button class="button button-secondary" onclick={syncWatchSettings}
            >{t('watch_sync')}</button
          >
          {#if watchStatus?.reachable && watchStatus.capabilities?.test_haptic}
            <button class="button button-primary" onclick={sendTestNudge}>{t('watch_nudge')}</button
            >
          {/if}
        </div>
      </div>
    </section>
  {/if}
</section>
