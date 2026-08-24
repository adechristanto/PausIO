<svelte:options runes={true} />

<script lang="ts">
  import { t } from '../lib/i18n'
  import { tooltip } from '../lib/tooltip'

  interface Props {
    label: string
    /** Tauri accelerator syntax (e.g. "CmdOrCtrl+Shift+X"), or null when disabled. */
    value: string | null
    onChange: (value: string | null) => void
  }
  let { label, value, onChange }: Props = $props()

  let recording = $state(false)
  let fieldEl: HTMLButtonElement | undefined = $state()

  const isApple = /Mac|iPhone|iPad/.test(navigator.userAgent)

  // Human-readable rendering of Tauri accelerator syntax, e.g. "CmdOrCtrl+Shift+X"
  // -> "⌘⇧X" on macOS or "Ctrl+Shift+X" elsewhere.
  function displayLabel(accelerator: string): string {
    const parts = accelerator.split('+')
    const key = parts.pop() ?? ''
    const symbolFor: Record<string, string> = isApple
      ? { CmdOrCtrl: '⌘', Cmd: '⌘', Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Super: '⌘' }
      : {
          CmdOrCtrl: 'Ctrl+',
          Cmd: 'Ctrl+',
          Ctrl: 'Ctrl+',
          Alt: 'Alt+',
          Shift: 'Shift+',
          Super: 'Win+',
        }
    const modifiers = parts.map((part) => symbolFor[part] ?? `${part}+`).join('')
    return `${modifiers}${key}`
  }

  // Rejects a lone modifier keypress (Control/Meta/Alt/Shift with no other
  // key) so recording never captures a shortcut with no addressable key.
  const isPlainModifier = (key: string) => ['Control', 'Meta', 'Alt', 'Shift'].includes(key)

  const namedKeys: Record<string, string> = {
    ' ': 'Space',
    Escape: 'Escape',
    Enter: 'Enter',
    Tab: 'Tab',
    Backspace: 'Backspace',
    Delete: 'Delete',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
  }

  function keyToAcceleratorToken(event: KeyboardEvent): string | null {
    if (namedKeys[event.key]) return namedKeys[event.key]
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.key)) return event.key
    if (event.key.length === 1) return event.key.toUpperCase()
    return null
  }

  function onKeydown(event: KeyboardEvent) {
    if (!recording) return
    event.preventDefault()
    event.stopPropagation()
    if (event.key === 'Escape') {
      recording = false
      return
    }
    if (isPlainModifier(event.key)) return
    const token = keyToAcceleratorToken(event)
    if (!token) return
    const modifiers: string[] = []
    if (event.metaKey || event.ctrlKey) modifiers.push('CmdOrCtrl')
    if (event.altKey) modifiers.push('Alt')
    if (event.shiftKey) modifiers.push('Shift')
    recording = false
    onChange([...modifiers, token].join('+'))
  }

  function startRecording() {
    recording = true
    fieldEl?.focus()
  }
</script>

<div class="shortcut-row">
  <span>{label}</span>
  <div class="shortcut-control">
    <button
      type="button"
      class="shortcut-field"
      class:recording
      bind:this={fieldEl}
      aria-label={label}
      use:tooltip={{ label: t('shortcut_record'), disabled: recording }}
      onkeydown={onKeydown}
      onclick={startRecording}
      onblur={() => (recording = false)}
    >
      {#if recording}
        <span class="shortcut-recording">{t('shortcut_recording')}</span>
      {:else if value}
        <span class="shortcut-value">{displayLabel(value)}</span>
      {:else}
        <span class="shortcut-placeholder">{t('shortcut_placeholder')}</span>
      {/if}
    </button>
    <button
      type="button"
      class="shortcut-clear"
      class:is-hidden={!value}
      tabindex={value ? 0 : -1}
      aria-hidden={!value}
      aria-label={t('shortcut_clear')}
      use:tooltip={{ label: t('shortcut_clear'), disabled: !value }}
      onclick={() => onChange(null)}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
    </button>
  </div>
</div>
