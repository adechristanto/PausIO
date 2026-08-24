import type { DisplayTarget, Settings, Strictness } from './types'

/**
 * `strictness` and `display_target` are stored as two independent fields, but the
 * engine does not honour them independently:
 *
 * - `due_grace_seconds` (`crates/pausio-core/src/engine.rs:29`) returns early with
 *   Gentle's 180 s grace whenever `display_target` is `notification_only`, throwing
 *   away the chosen strictness.
 * - `events.rs` raises no overlay when strictness is `gentle` **or** the target is
 *   `notification_only`, which makes the All/Active/Primary choice inert.
 *
 * So of the 16 storable pairs only five behave distinctly, and some actively lie:
 * "Strict + Notification only" promised a locked fullscreen shield and delivered a
 * plain notification. This module is the single mapping between the stored pair and
 * the four modes a person can actually get, so the UI and the plain-language summary
 * cannot drift apart.
 */
export type DeliveryMode = 'notify' | 'ask' | 'cover' | 'hold'

export type CoveredDisplays = Exclude<DisplayTarget, 'notification_only'>

export const DELIVERY_MODES: readonly DeliveryMode[] = ['notify', 'ask', 'cover', 'hold']

export const MODE_TO_STRICTNESS: Record<DeliveryMode, Strictness> = {
  notify: 'gentle',
  ask: 'balanced',
  cover: 'firm',
  hold: 'strict',
}

/** The mode the engine will actually deliver, not the pair that happens to be stored. */
export const deliveryModeOf = (settings: Settings): DeliveryMode => {
  if ((settings.display_target ?? 'all') === 'notification_only') return 'notify'
  const strictness = settings.strictness ?? 'balanced'
  if (strictness === 'gentle') return 'notify'
  if (strictness === 'firm') return 'cover'
  if (strictness === 'strict') return 'hold'
  return 'ask'
}

/**
 * Which displays a covering mode would use. Kept separate from the mode so that
 * switching to "Just notify me" and back restores the person's display choice
 * instead of silently resetting it to All.
 */
export const coveredDisplaysOf = (settings: Settings): CoveredDisplays =>
  settings.display_target === 'notification_only' || settings.display_target === undefined
    ? 'all'
    : settings.display_target

/** The settings patch for choosing a mode, keeping the pair internally consistent. */
export const deliveryPatch = (
  settings: Settings,
  mode: DeliveryMode
): Pick<Settings, 'strictness' | 'display_target'> => ({
  strictness: MODE_TO_STRICTNESS[mode],
  display_target: mode === 'notify' ? 'notification_only' : coveredDisplaysOf(settings),
})
