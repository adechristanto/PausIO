import type { Locale } from './types'

// A rune-backed module singleton, not a plain `let`: any component template
// (via `t()`/`tCount()`) that reads the active locale during render becomes
// reactively subscribed to it, so switching languages updates every string
// in place. Before this, `i18n.ts` held a plain module variable Svelte could
// not track, so the whole app had to be force-unmounted and remounted via
// `{#key localizationRevision}` on every language change to pick up new text
// — which also dropped keyboard focus and scroll position.
let current = $state<Locale>('en')

export function getActiveLocale(): Locale {
  return current
}

export function setActiveLocale(locale: Locale): void {
  current = locale
}
