<svelte:options runes={true} />

<script lang="ts">
  import pausioMark from '../assets/pausio-mark.svg'
  import { onMount, tick } from 'svelte'
  import { t } from '../lib/i18n'
  import type { BreakKind, Settings, Snapshot } from '../lib/types'

  interface Props {
    state: Snapshot | null
    settings: Settings | null
    error?: string
    onStart: () => Promise<void>
    onPostpone: () => Promise<void>
    onPauseFor: (minutes: number) => Promise<void>
  }
  let { state: current, settings, error = '', onStart, onPostpone, onPauseFor }: Props = $props()
  let startButton: HTMLButtonElement | undefined
  let pauseTrigger = $state<HTMLButtonElement>()
  let pauseMenuEl = $state<HTMLElement>()
  let pauseMenuOpen = $state(false)

  const pauseChoices = [
    { minutes: 30, labelKey: 'break_pause_30' },
    { minutes: 60, labelKey: 'break_pause_60' },
    { minutes: 120, labelKey: 'break_pause_120' },
  ] as const

  const dueKind = (value: Snapshot | null): BreakKind => {
    if (value && typeof value.phase === 'object' && 'break_due' in value.phase) {
      return value.phase.break_due.kind
    }
    return 'short'
  }
  const breakLabel = () => {
    if (dueKind(current) === 'long') {
      return t('break_start_long', {
        minutes: Math.round((settings?.long_break_seconds ?? 300) / 60),
      })
    }
    return t('break_start_short', { seconds: settings?.short_break_seconds ?? 20 })
  }
  const canPostpone = () => (settings?.strictness ?? 'balanced') === 'balanced'

  const menuItems = () =>
    Array.from(pauseMenuEl?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])

  const togglePauseMenu = async () => {
    pauseMenuOpen = !pauseMenuOpen
    if (pauseMenuOpen) {
      await tick()
      menuItems()[0]?.focus()
    }
  }
  const closePauseMenu = (refocus = false) => {
    if (!pauseMenuOpen) return
    pauseMenuOpen = false
    if (refocus) pauseTrigger?.focus()
  }
  const choosePause = (minutes: number) => {
    closePauseMenu()
    void onPauseFor(minutes)
  }
  const onMenuKeydown = (event: KeyboardEvent) => {
    if (!pauseMenuOpen) return
    const items = menuItems()
    const index = items.indexOf(document.activeElement as HTMLButtonElement)
    if (event.key === 'Escape') {
      event.preventDefault()
      closePauseMenu(true)
    } else if (event.key === 'ArrowDown') {
      event.preventDefault()
      items[(index + 1) % items.length]?.focus()
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      items[(index - 1 + items.length) % items.length]?.focus()
    }
  }

  // This window never takes keyboard focus on its own — it is persistent and
  // appears in the corner without activating, so it cannot steal the caret
  // mid-sentence. Keys can only reach it after a deliberate click, and even
  // then we block keyboard input while it is open: auto-focusing the "Start"
  // button would let a stray keypress (e.g. Space/Enter) start the
  // break/pause before the user intended. The "Pause for…" dropdown, once
  // opened via mouse, still needs its own Escape/ArrowUp/ArrowDown handling
  // (onMenuKeydown) to be usable, so we don't swallow keys while that menu
  // is open.
  const blockKeyboard = (event: KeyboardEvent) => {
    if (pauseMenuOpen) return
    event.preventDefault()
    event.stopPropagation()
  }

  onMount(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null
      if (target && !pauseMenuEl?.contains(target) && !pauseTrigger?.contains(target)) {
        closePauseMenu()
      }
    }
    window.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('keydown', blockKeyboard, true)
    return () => {
      window.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('keydown', blockKeyboard, true)
    }
  })
</script>

<main class="break-prompt" aria-live="assertive">
  <section class="prompt-card">
    <div class="prompt-mark" aria-hidden="true">
      <img src={pausioMark} alt="" width="38" height="38" />
    </div>
    <h1>{t('break_due_heading')}</h1>
    {#if error}<p class="window-error" role="alert">{error}</p>{/if}
    <div class="prompt-actions">
      <button class="button button-primary" bind:this={startButton} onclick={onStart}
        >{breakLabel()}</button
      >
      {#if canPostpone()}<button class="button button-quiet" onclick={onPostpone}
          >{t('break_postpone')}</button
        >{/if}
      <div class="prompt-menu-wrap">
        <button
          class="button button-quiet"
          bind:this={pauseTrigger}
          aria-haspopup="menu"
          aria-expanded={pauseMenuOpen}
          onclick={togglePauseMenu}
          onkeydown={onMenuKeydown}>{t('break_pause_for')}</button
        >
        {#if pauseMenuOpen}
          <div
            class="prompt-menu"
            role="menu"
            aria-label={t('break_pause_for')}
            tabindex="-1"
            bind:this={pauseMenuEl}
            onkeydown={onMenuKeydown}
          >
            {#each pauseChoices as choice (choice.minutes)}
              <button role="menuitem" onclick={() => choosePause(choice.minutes)}
                >{t(choice.labelKey)}</button
              >
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </section>
</main>
