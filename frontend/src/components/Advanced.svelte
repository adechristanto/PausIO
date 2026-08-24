<svelte:options runes={true} />

<script lang="ts">
  import { t } from '../lib/i18n'

  /**
   * Every settings pane hides its rarely-touched controls behind one of these.
   * Nothing is removed from the app, only demoted, so the default view of each
   * pane stays under about five controls while everything stays reachable one
   * click away. Modelled on the existing `.diagnostics` disclosure rather than
   * a native `<details>`, so the open/close chevron and focus ring match it.
   */
  interface Props {
    open?: boolean
    children: import('svelte').Snippet
  }
  let { open = $bindable(false), children }: Props = $props()
</script>

<section class="advanced">
  <button class="advanced-trigger" aria-expanded={open} onclick={() => (open = !open)}>
    <span>{t('advanced_settings')}</span>
    <svg class:open viewBox="0 0 24 24" aria-hidden="true"><path d="m7 10 5 5 5-5" /></svg>
  </button>
  {#if open}
    <div class="advanced-content setting-list">
      {@render children()}
    </div>
  {/if}
</section>
