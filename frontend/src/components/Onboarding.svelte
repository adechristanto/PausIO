<svelte:options runes={true} />

<script lang="ts">
  import {
    coveredDisplaysOf,
    deliveryModeOf,
    deliveryPatch,
    DELIVERY_MODES,
    type DeliveryMode,
  } from '../lib/delivery'
  import { clockToMinutes, minutesToClock } from '../lib/format'
  import { isRoundTheClock, roundTheClockPatch } from '../lib/schedule'
  import { t } from '../lib/i18n'
  import { tooltip } from '../lib/tooltip'
  import type { DisplayTarget, Settings } from '../lib/types'
  import pausioMark from '../assets/pausio-mark.svg'

  /**
   * Modern 4-step onboarding flow:
   * 1. Welcome & Value Proposition (Why PausIO exists, core benefits, "Get Started")
   * 2. Schedule (When do you work? Active days and hours)
   * 3. Reminder Style (Visual delivery mode selection: Gentle, Balanced [Recommended], Firm)
   * 4. Interactive Test Break & Launch (Immediate verification and start)
   *
   * Safety invariant: Strict must never be pre-selected here.
   */
  interface Props {
    settings: Settings
    editSettings: (next: Settings) => void
    toggleDay: (index: number) => void
    takeBreakNow: () => Promise<void>
    onSkip: () => void
    onFinish: () => void
  }
  let { settings, editSettings, toggleDay, takeBreakNow, onSkip, onFinish }: Props = $props()

  const TOTAL_STEPS = 4
  let step = $state<1 | 2 | 3 | 4>(1)
  let breakStarted = $state(false)

  const appLocale = () => (settings.locale === 'de' ? 'de-DE' : 'en-US')
  const dayLabels = () => {
    const locale = appLocale()
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
  const roundTheClock = $derived(
    isRoundTheClock(settings.active_start_minutes, settings.active_end_minutes)
  )

  const deliveryMode = $derived(deliveryModeOf(settings))
  const coversScreen = $derived(deliveryMode !== 'notify')
  const coveredDisplays = $derived(coveredDisplaysOf(settings))
  const setDeliveryMode = (mode: DeliveryMode) =>
    editSettings({ ...settings, ...deliveryPatch(settings, mode) })

  const tryItNow = async () => {
    await takeBreakNow()
    breakStarted = true
  }

  const applyWorkdayPreset = () => {
    editSettings({
      ...settings,
      active_days_mask: 0b0111110, // Mon-Fri
      active_start_minutes: 540, // 09:00
      active_end_minutes: 1020, // 17:00
    })
  }
</script>

<div
  class="onboarding-overlay"
  role="dialog"
  aria-modal="true"
  aria-labelledby="onboarding-heading"
>
  <div class="onboarding-card">
    <div class="onboarding-top">
      <div
        class="onboarding-stepper"
        aria-label={t('onboarding_step_label', { step, total: TOTAL_STEPS })}
      >
        <span class="onboarding-step-label"
          >{t('onboarding_step_label', { step, total: TOTAL_STEPS })}</span
        >
        <div class="onboarding-dots" aria-hidden="true">
          {#each Array.from({ length: TOTAL_STEPS }, (_, i) => i + 1) as s}
            <span class="onboarding-dot" class:active={s === step} class:completed={s < step}
            ></span>
          {/each}
        </div>
      </div>
      <div class="onboarding-top-actions">
        <div class="lang-picker" role="group" aria-label={t('setting_language')}>
          <button
            type="button"
            class="lang-btn"
            class:active={settings.locale !== 'de'}
            aria-pressed={settings.locale !== 'de'}
            onclick={() => editSettings({ ...settings, locale: 'en' })}>EN</button
          >
          <button
            type="button"
            class="lang-btn"
            class:active={settings.locale === 'de'}
            aria-pressed={settings.locale === 'de'}
            onclick={() => editSettings({ ...settings, locale: 'de' })}>DE</button
          >
        </div>
        <button type="button" class="text-button" onclick={onSkip}>{t('onboarding_skip')}</button>
      </div>
    </div>

    {#if step === 1}
      <div class="onboarding-welcome">
        <div class="onboarding-brand-icon">
          <img src={pausioMark} alt="" width="44" height="44" />
        </div>
        <h1 id="onboarding-heading">{t('onboarding_welcome_heading')}</h1>
        <p class="onboarding-body onboarding-lead">{t('onboarding_welcome_subhead')}</p>

        <div class="onboarding-features">
          <div class="onboarding-feature-item">
            <div class="feature-icon" aria-hidden="true">
              <svg
                viewBox="0 0 24 24"
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M2 12s3.64-7 10-7 10 7 10 7-3.64 7-10 7S2 12 2 12z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
            </div>
            <div class="feature-text">
              <strong>{t('onboarding_feature_breaks_title')}</strong>
              <p>{t('onboarding_feature_breaks_body')}</p>
            </div>
          </div>

          <div class="onboarding-feature-item">
            <div class="feature-icon" aria-hidden="true">
              <svg
                viewBox="0 0 24 24"
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path
                  d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"
                />
              </svg>
            </div>
            <div class="feature-text">
              <strong>{t('onboarding_feature_posture_title')}</strong>
              <p>{t('onboarding_feature_posture_body')}</p>
            </div>
          </div>

          <div class="onboarding-feature-item">
            <div class="feature-icon" aria-hidden="true">
              <svg
                viewBox="0 0 24 24"
                width="20"
                height="20"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
              </svg>
            </div>
            <div class="feature-text">
              <strong>{t('onboarding_feature_privacy_title')}</strong>
              <p>{t('onboarding_feature_privacy_body')}</p>
            </div>
          </div>
        </div>
      </div>
    {:else if step === 2}
      <h1 id="onboarding-heading">{t('onboarding_schedule_heading')}</h1>
      <p class="onboarding-body">{t('onboarding_schedule_body')}</p>

      <div class="onboarding-preset-bar">
        <button type="button" class="preset-chip" onclick={applyWorkdayPreset}>
          <svg
            viewBox="0 0 24 24"
            width="14"
            height="14"
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            ><rect x="3" y="4" width="18" height="18" rx="2" ry="2" /><line
              x1="16"
              y1="2"
              x2="16"
              y2="6"
            /><line x1="8" y1="2" x2="8" y2="6" /><line x1="3" y1="10" x2="21" y2="10" /></svg
          >
          <span>{t('onboarding_preset_workdays')}</span>
        </button>
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
          onchange={(event) =>
            editSettings({ ...settings, ...roundTheClockPatch(event.currentTarget.checked) })}
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
                  editSettings({ ...settings, active_start_minutes: value })
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
                if (Number.isFinite(value)) editSettings({ ...settings, active_end_minutes: value })
              }}
            />
          </label>
        </div>
      {/if}
    {:else if step === 3}
      <h1 id="onboarding-heading">{t('onboarding_delivery_heading')}</h1>
      <p class="onboarding-body">{t('onboarding_delivery_body')}</p>

      <div class="onboarding-mode-cards" role="radiogroup" aria-label={t('setting_delivery_mode')}>
        {#each DELIVERY_MODES as mode}
          {@const isSelected = deliveryMode === mode}
          <button
            type="button"
            class="mode-card"
            class:chosen={isSelected}
            role="radio"
            aria-checked={isSelected}
            onclick={() => setDeliveryMode(mode)}
          >
            <div class="mode-card-header">
              <div class="mode-card-icon" aria-hidden="true">
                {#if mode === 'notify'}
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                    <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                  </svg>
                {:else if mode === 'ask'}
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <rect x="2" y="4" width="20" height="14" rx="2" />
                    <path d="M8 20h8M12 18v2" />
                    <circle cx="8" cy="11" r="1" />
                    <path d="M12 11h4" />
                  </svg>
                {:else if mode === 'cover'}
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <rect x="2" y="3" width="20" height="14" rx="2" />
                    <path d="M8 21h8M12 17v4" />
                  </svg>
                {:else}
                  <svg
                    viewBox="0 0 24 24"
                    width="18"
                    height="18"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
                  </svg>
                {/if}
              </div>
              <span class="mode-card-title">
                {t(
                  `delivery_mode_${mode}` as
                    | 'delivery_mode_notify'
                    | 'delivery_mode_ask'
                    | 'delivery_mode_cover'
                    | 'delivery_mode_hold'
                )}
              </span>
              {#if mode === 'ask'}
                <span class="badge-recommended">{t('onboarding_recommended_badge')}</span>
              {/if}
            </div>
            <p class="mode-card-desc">
              {t(
                `delivery_mode_${mode}_desc` as
                  | 'delivery_mode_notify_desc'
                  | 'delivery_mode_ask_desc'
                  | 'delivery_mode_cover_desc'
                  | 'delivery_mode_hold_desc'
              )}
            </p>
          </button>
        {/each}
      </div>

      {#if coversScreen}
        <label class="select-row">
          <span>{t('setting_display_target')}</span>
          <select
            value={coveredDisplays}
            onchange={(event) =>
              editSettings({
                ...settings,
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
    {:else}
      <h1 id="onboarding-heading">{t('onboarding_try_heading')}</h1>
      <p class="onboarding-body">{t('onboarding_try_body')}</p>

      <div class="onboarding-try-card">
        {#if breakStarted}
          <div class="onboarding-try-confirmation" role="status">
            <svg
              viewBox="0 0 24 24"
              width="18"
              height="18"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <polyline points="20 6 9 17 4 12" />
            </svg>
            <span>{t('onboarding_try_done')}</span>
          </div>
        {:else}
          <button type="button" class="button button-primary test-break-btn" onclick={tryItNow}>
            <svg
              viewBox="0 0 24 24"
              width="18"
              height="18"
              aria-hidden="true"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              ><path d="M2 12s3.64-7 10-7 10 7 10 7-3.64 7-10 7S2 12 2 12z" /><circle
                cx="12"
                cy="12"
                r="3"
              /></svg
            >
            <span>{t('action_take_break')}</span>
          </button>
        {/if}
      </div>
    {/if}

    <div class="onboarding-nav">
      {#if step > 1}
        <button type="button" class="button button-secondary" onclick={() => (step -= 1)}
          >{t('onboarding_back')}</button
        >
      {/if}
      {#if step === 1}
        <button type="button" class="button button-primary" onclick={() => (step += 1)}
          >{t('onboarding_get_started')}</button
        >
      {:else if step < TOTAL_STEPS}
        <button type="button" class="button button-primary" onclick={() => (step += 1)}
          >{t('onboarding_next')}</button
        >
      {:else}
        <button type="button" class="button button-primary" onclick={onFinish}
          >{t('onboarding_start_app')}</button
        >
      {/if}
    </div>
  </div>
</div>
