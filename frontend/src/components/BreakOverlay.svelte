<svelte:options runes={true} />

<script lang="ts">
  import { onMount } from 'svelte'
  import TimerRing from './TimerRing.svelte'
  import { formatClock, formatTimeOfDay } from '../lib/format'
  import { t } from '../lib/i18n'
  import type { Settings, Snapshot } from '../lib/types'

  // A full-screen slam to near-black is the one thing worse for a photosensitive user than a
  // fade-in — so this is a plain CSS opacity transition (shortened, not skipped, under Reduce
  // Motion by .break-overlay's own rule in styles.css) rather than removed outright, per the
  // PRD's "Reduce Motion replaces slides/blurs with cross-fades", not "removes them".
  let entered = $state(false)
  let overlayElement: HTMLElement | undefined
  onMount(() => {
    entered = true
    if (primary) overlayElement?.focus()
  })

  interface Props {
    state: Snapshot | null
    settings?: Settings | null
    error?: string
    /** The focused overlay owns the single announcement and the error slot. */
    primary?: boolean
    /** Ends the break early from the focused overlay. */
    onDone?: () => Promise<void>
  }
  // Renamed on destructure: a local binding literally named `state` collides with the
  // `$state` rune below — Svelte treats `$state` as store-auto-subscription of that
  // binding instead of the rune (svelte.dev/e/store_rune_conflict).
  let { state: snapshot, settings = null, error = '', primary = false, onDone }: Props = $props()

  const kindOf = (value: Snapshot | null) =>
    value && typeof value.phase === 'object' && 'breaking' in value.phase
      ? value.phase.breaking.kind
      : 'short'

  // Only the focused overlay carries controls. Strict remains deliberately
  // difficult to dismiss, but it always exposes an explicit emergency exit:
  // a timer must never make a person's computer unusable.
  const strictness = $derived(settings?.strictness ?? 'balanced')
  const isEmergency = $derived(strictness === 'strict')
  const isDismissible = $derived(primary && !isEmergency)

  const totalOf = (value: Snapshot | null) => {
    const long = kindOf(value) === 'long'
    if (!settings) return long ? 300 : 20
    return Math.max(1, long ? settings.long_break_seconds : settings.short_break_seconds)
  }

  const fraction = $derived.by(() => {
    const total = totalOf(snapshot)
    const remaining = snapshot?.remaining_seconds ?? total
    return Math.max(0, Math.min(1, remaining / total))
  })
  // Only the "guided" routine is a multi-step "Gentle reset" sequence; the other
  // routines (quiet timer, a single held gaze, one blink cue, one posture cue)
  // are a single instruction for the whole break, so `totalSteps` is 1 and the
  // "step X of N" eyebrow is skipped for them entirely rather than always
  // claiming "step 1 of 4" regardless of which routine is actually running.
  const exercise = $derived.by(() => {
    const routine = settings?.break_routine ?? 'guided'
    if (routine === 'quiet') return { step: 1, totalSteps: 1, message: t('break_guidance') }
    if (routine === 'far_gaze') return { step: 1, totalSteps: 1, message: t('exercise_far') }
    if (routine === 'blink') return { step: 1, totalSteps: 1, message: t('exercise_blink') }
    if (routine === 'posture') return { step: 1, totalSteps: 1, message: t('exercise_rest') }
    const total = totalOf(snapshot)
    const elapsed = Math.max(0, total - (snapshot?.remaining_seconds ?? total))
    const keys = [
      'exercise_far',
      'exercise_blink',
      'exercise_figure_eight',
      'exercise_rest',
    ] as const
    // Spend each step for an equal share of however long this break actually is, rather than
    // a fixed 5s — otherwise a long break (minutes) reaches the last step in 20s and then
    // sits frozen on it for the rest of the break, which is what the fixed cadence used to do.
    const stepSeconds = total / keys.length
    const index = Math.min(keys.length - 1, Math.floor(elapsed / stepSeconds))
    return { step: index + 1, totalSteps: keys.length, message: t(keys[index]) }
  })
  const customMessage = $derived.by(() => {
    const messages =
      settings?.break_messages?.map((message) => message.trim()).filter(Boolean) ?? []
    return messages.length === 0
      ? null
      : messages[(snapshot?.completed_short_breaks ?? 0) % messages.length]
  })
  // Rides the existing per-second tick rather than starting its own interval:
  // `snapshot.remaining_seconds` changes once a second while a break runs,
  // which is enough to keep this reading current without new timer state.
  const currentClock = $derived.by(() => {
    void snapshot?.remaining_seconds
    return formatTimeOfDay(new Date(), settings?.locale)
  })

  let announcement = $state('')
  let announcedAt = -1

  // FR-903: announce at start, at the halfway mark, and at the end. Never per tick.
  $effect(() => {
    if (!primary || !snapshot) return
    const left = snapshot.remaining_seconds
    const span = totalOf(snapshot)
    const next =
      left >= span
        ? t('break_announce_start', { seconds: span })
        : left === Math.round(span / 2)
          ? t('break_announce_half', { seconds: left })
          : left === 0
            ? t('break_announce_end')
            : null
    if (next && left !== announcedAt) {
      announcedAt = left
      announcement = next
    }
  })
</script>

<main class="break-overlay" class:entered tabindex="-1" bind:this={overlayElement}>
  <div class="break-halo" aria-hidden="true"></div>
  <section class="break-card">
    <p class="eyebrow">
      {kindOf(snapshot) === 'long' ? t('phase_long_break') : t('phase_short_break')}
    </p>
    <h1>{kindOf(snapshot) === 'long' ? t('break_heading_long') : t('break_heading')}</h1>
    <div class="break-ring-wrap">
      <TimerRing
        fraction={snapshot ? fraction : null}
        clock={formatClock(snapshot?.remaining_seconds ?? totalOf(snapshot))}
        subLabel=""
        tone="rest"
        size="overlay"
      />
    </div>
    <p class="sr-only">
      {t('break_remaining', { seconds: snapshot?.remaining_seconds ?? totalOf(snapshot) })}
    </p>
    {#if settings?.show_clock_in_break}<p class="break-clock">{currentClock}</p>{/if}
    {#if exercise.totalSteps > 1}<p class="exercise-step">
        {t('exercise_step', { step: exercise.step, total: exercise.totalSteps })}
      </p>{/if}
    {#if customMessage}<p class="break-message">{customMessage}</p>{/if}
    <p class="break-guidance">{exercise.message}</p>
    {#if primary && error}<p class="window-error" role="alert">{error}</p>{/if}
    {#if isDismissible}
      <div class="break-actions">
        <button class="button button-primary" onclick={onDone}
          >{kindOf(snapshot) === 'short' ? t('break_done') : t('break_end_early')}</button
        >
      </div>
    {:else if primary}
      <div class="break-actions">
        <button class="button button-quiet" onclick={onDone}>{t('break_emergency_end')}</button>
      </div>
    {/if}
  </section>

  {#if primary}<p class="sr-only" role="status" aria-live="polite">{announcement}</p>{/if}
</main>
