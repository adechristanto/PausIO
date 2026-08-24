<svelte:options runes={true} />

<script lang="ts">
  interface Props {
    /** Remaining time as 0..1, or null while indeterminate (no snapshot yet). */
    fraction: number | null
    /** Pre-formatted clock, e.g. "19:48" or "--:--". */
    clock: string
    subLabel: string
    /** Semantic display tone; colours live in the parent design system. */
    tone?: 'focus' | 'warning' | 'rest' | 'paused' | 'dormant' | 'loading'
    /** The overlay uses the same dial at a more compact scale. */
    size?: 'dashboard' | 'overlay'
  }
  let { fraction, clock, subLabel, tone = 'focus', size = 'dashboard' }: Props = $props()

  const R = 45 // arc radius in viewBox units
  const C = 2 * Math.PI * R // 282.7433388230814

  // A full circumference with a dash offset is reliable at 0 and 1: the arc
  // drains clockwise from the calibration point without the conic-gradient seam.
  let offset = $derived(fraction === null ? C : C * (1 - fraction))
  let endpoint = $derived(
    fraction === null
      ? null
      : {
          x: 50 + R * Math.sin(fraction * Math.PI * 2),
          y: 50 - R * Math.cos(fraction * Math.PI * 2),
        }
  )
</script>

<div class="horizon-timer timer-{tone} timer-{size}">
  <!-- Do not name this `ring`: Tailwind reserves `.ring` for a rectangular
       box-shadow utility, which would outline this SVG's bounding box. -->
  <svg class="timer-ring" viewBox="0 0 100 100" aria-hidden="true" focusable="false">
    <circle class="ring-halo" cx="50" cy="50" r="48" />
    <g transform="rotate(-90 50 50)">
      <circle class="ring-track" cx="50" cy="50" r={R} />
      {#if fraction !== null}
        <circle
          class="ring-fill"
          cx="50"
          cy="50"
          r={R}
          stroke-dasharray="{C} {C}"
          stroke-dashoffset={offset}
        />
      {/if}
    </g>
    <circle class="ring-calibration" cx="50" cy="5" r="1.25" />
    {#if endpoint}<circle class="ring-endpoint" cx={endpoint.x} cy={endpoint.y} r="2.25" />{/if}
  </svg>
  <div class="timer-inner">
    <strong
      >{#each clock.split(':') as segment, i}{#if i > 0}<span class="timer-colon">:</span>{/if}<span
          class="timer-digits">{segment}</span
        >{/each}</strong
    >
    <span>{subLabel}</span>
  </div>
</div>
