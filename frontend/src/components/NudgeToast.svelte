<svelte:options runes={true} />

<script lang="ts">
  import { t } from '../lib/i18n'

  interface Props {
    /** Which reminder came due: 'blink' | 'posture' | 'hydration'. */
    nudge: string
  }
  let { nudge }: Props = $props()

  // The same strings the screen-reader announcement in the main window uses, so
  // a nudge reads identically whether it arrives as a macOS banner, as this
  // toast, or through assistive technology.
  const message = $derived(
    nudge === 'posture'
      ? t('nudge_posture_announcement')
      : nudge === 'hydration'
        ? t('nudge_hydration_announcement')
        : t('nudge_blink_announcement')
  )

  const icon = $derived(
    nudge === 'posture'
      ? 'M12 5a2 2 0 1 0 0-4 2 2 0 0 0 0 4M12 7v7m0 0-3 8m3-8 3 8M7 10h10'
      : nudge === 'hydration'
        ? 'M12 3s5 5.5 5 9a5 5 0 0 1-10 0c0-3.5 5-9 5-9'
        : 'M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6m10 2.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5'
  )
</script>

<!--
  Advisory only: the window itself is built non-focusable and carries no
  controls, so this never takes keyboard focus from whatever someone is typing
  into. `role="status"` with `aria-live="polite"` announces it without
  interrupting, matching the gentleness the reminder is meant to have.
-->
<aside class="nudge-toast" role="status" aria-live="polite">
  <span class="nudge-icon" aria-hidden="true">
    <svg viewBox="0 0 24 24"><path d={icon} /></svg>
  </span>
  <p>{message}</p>
</aside>
