<svelte:options runes={true} />

<script lang="ts">
  import pausioMark from '../assets/pausio-mark.svg'
  import { onMount, tick } from 'svelte'
  import { t } from '../lib/i18n'
  import { pauseChoices } from '../lib/pauseChoices'
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
  let promptActionsEl = $state<HTMLElement>()
  let pauseMenuOpen = $state(false)

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
  // mid-sentence. Nothing is auto-focused, so a stray keypress (e.g.
  // Space/Enter) before a person has deliberately interacted cannot start
  // the break/pause. Tab is always allowed through so keyboard-only users can
  // reach the controls at all; once focus has moved onto one of this
  // window's own buttons, every key reaches it normally so Enter/Space and
  // arrow-key menu navigation (onMenuKeydown) work as expected.
  const blockKeyboard = (event: KeyboardEvent) => {
    if (pauseMenuOpen || event.key === 'Tab') return
    const active = document.activeElement
    if (active instanceof Node && promptActionsEl?.contains(active)) return
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
    <div class="prompt-actions" bind:this={promptActionsEl}>
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
