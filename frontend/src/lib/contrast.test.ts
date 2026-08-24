import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * WCAG 2.2 AA contrast sweep over every theme x accent combination the app can actually
 * render (Phase 1 acceptance criterion #8 of the UI/UX audit). Values are extracted directly
 * from styles.css rather than duplicated as literals here, so this fails the moment a future
 * edit reintroduces a token that can't be read, not just when someone remembers to re-run a
 * calculator by hand.
 */

const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8')

function relativeLuminance(hex: string): number {
  const channel = (value: number) =>
    value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
  const [r, g, b] = [0, 2, 4].map((i) =>
    channel(parseInt(hex.replace('#', '').slice(i, i + 2), 16) / 255)
  )
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}
function contrast(a: string, b: string): number {
  const [l1, l2] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x)
  return (l1 + 0.05) / (l2 + 0.05)
}

/** Pulls `--token: #hex;` custom properties out of one `{ ... }` declaration block. */
function extractTokens(block: string): Record<string, string> {
  const tokens: Record<string, string> = {}
  for (const match of block.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{3,8})/g))
    tokens[match[1]] = match[2]
  return tokens
}
function findBlock(selectorPattern: string, within = css): string {
  const match = within.match(new RegExp(`${selectorPattern}\\s*\\{([^}]*)\\}`))
  if (!match) throw new Error(`selector not found: ${selectorPattern}`)
  return match[1]
}

const darkBase = extractTokens(findBlock(':root\\s*'))
const lightExplicit = extractTokens(findBlock(":root\\[data-theme='light'\\]\\s*"))
const darkExplicit = extractTokens(findBlock(":root\\[data-theme='dark'\\]\\s*"))
const lightSystem = extractTokens(
  findBlock('@media \\(prefers-color-scheme: light\\)\\s*\\{\\s*:root\\s*')
)

/** The `@media (...) { ... }` block whose body contains `bodyMarker`, found by brace-counting
 * rather than a guessed closing pattern (a nested media query can contain further `{}` pairs). */
function findMediaBlockBody(bodyMarker: string): string {
  // The second occurrence: the first is the unconditional (dark-mode-tuned) rule this media
  // query exists to override for light mode.
  const firstIndex = css.indexOf(bodyMarker)
  const markerIndex = firstIndex === -1 ? -1 : css.indexOf(bodyMarker, firstIndex + 1)
  if (markerIndex === -1)
    throw new Error(`second occurrence not found in styles.css: ${bodyMarker}`)
  const openIndex = css.lastIndexOf('@media', markerIndex)
  const bodyStart = css.indexOf('{', openIndex) + 1
  let depth = 1
  let i = bodyStart
  for (; i < css.length && depth > 0; i += 1) {
    if (css[i] === '{') depth += 1
    else if (css[i] === '}') depth -= 1
  }
  return css.slice(bodyStart, i - 1)
}

// The shared `@media (prefers-color-scheme: light) { :root[data-accent='sage'] {...} ... }`
// block (distinct from the unconditional per-accent rules used for dark mode).
const systemLightAccentBlock = findMediaBlockBody(":root[data-accent='sage']")

const accentBase: Record<string, Record<string, string>> = {}
const accentLightExplicit: Record<string, Record<string, string>> = {}
const accentLightSystem: Record<string, Record<string, string>> = {}
for (const accent of ['sage', 'amber', 'lilac']) {
  accentBase[accent] = extractTokens(findBlock(`:root\\[data-accent='${accent}'\\]\\s*`))
  accentLightExplicit[accent] = extractTokens(
    findBlock(`:root\\[data-theme='light'\\]\\[data-accent='${accent}'\\]\\s*`)
  )
  accentLightSystem[accent] = extractTokens(
    findBlock(`:root\\[data-accent='${accent}'\\]\\s*`, systemLightAccentBlock)
  )
}

// Both light-mode paths (`theme: 'system'` on a light OS, and `theme: 'light'` explicitly)
// are meant to share identical rule bodies by construction; assert that rather than
// re-deriving values, so the two can't silently drift apart.
describe('light-mode tokens: the system-preference path and the explicit-theme path agree', () => {
  it('base tokens (canvas, text, borders, …)', () => {
    expect(lightSystem).toEqual(lightExplicit)
  })
  it.each(['sage', 'amber', 'lilac'])('%s accent tokens', (accent) => {
    expect(accentLightSystem[accent]).toEqual(accentLightExplicit[accent])
  })
})

type Scheme = {
  name: string
  canvas: string
  surfaceRaised: string
  surfaceInput: string
  text: string
  textMuted: string
  lineStrong: string
  accent: string
  accentInk: string
  focus: string
}

const schemes: Scheme[] = [
  {
    name: 'dark / horizon',
    canvas: darkExplicit.canvas,
    surfaceRaised: darkExplicit['surface-raised'],
    surfaceInput: darkExplicit['surface-input'],
    text: darkExplicit.text,
    textMuted: darkExplicit['text-muted'],
    lineStrong: darkExplicit['line-strong'],
    accent: darkExplicit.accent,
    accentInk: darkExplicit['accent-ink'],
    focus: darkExplicit.focus,
  },
  {
    name: 'light / horizon',
    canvas: lightExplicit.canvas,
    surfaceRaised: lightExplicit['surface-raised'],
    surfaceInput: lightExplicit['surface-input'],
    text: lightExplicit.text,
    textMuted: lightExplicit['text-muted'],
    lineStrong: lightExplicit['line-strong'],
    accent: lightExplicit.accent,
    accentInk: lightExplicit['accent-ink'],
    focus: lightExplicit.focus,
  },
  ...(['sage', 'amber', 'lilac'] as const).map((accent) => ({
    name: `dark / ${accent}`,
    canvas: darkExplicit.canvas,
    surfaceRaised: darkExplicit['surface-raised'],
    surfaceInput: darkExplicit['surface-input'],
    text: darkExplicit.text,
    textMuted: darkExplicit['text-muted'],
    lineStrong: darkExplicit['line-strong'],
    accent: accentBase[accent].accent,
    accentInk: accentBase[accent]['accent-ink'],
    focus: accentBase[accent].focus,
  })),
  ...(['sage', 'amber', 'lilac'] as const).map((accent) => ({
    name: `light / ${accent}`,
    canvas: lightExplicit.canvas,
    surfaceRaised: lightExplicit['surface-raised'],
    surfaceInput: lightExplicit['surface-input'],
    text: lightExplicit.text,
    textMuted: lightExplicit['text-muted'],
    lineStrong: lightExplicit['line-strong'],
    accent: accentLightExplicit[accent].accent,
    accentInk: accentLightExplicit[accent]['accent-ink'],
    focus: accentLightExplicit[accent].focus,
  })),
]

describe('WCAG 2.2 AA contrast sweep across every theme x accent combination', () => {
  it('covers all 8 reachable combinations', () => {
    expect(schemes).toHaveLength(8)
    for (const { name, ...tokens } of schemes)
      for (const value of Object.values(tokens)) expect(value, name).toMatch(/^#[0-9a-fA-F]{3,8}$/)
  })

  it.each(schemes)('$name: body text is >=4.5:1 on canvas', (scheme) => {
    expect(contrast(scheme.text, scheme.canvas)).toBeGreaterThanOrEqual(4.5)
  })

  it.each(schemes)('$name: muted text is >=4.5:1 on canvas and on the raised surface', (scheme) => {
    expect(contrast(scheme.textMuted, scheme.canvas)).toBeGreaterThanOrEqual(4.5)
    expect(contrast(scheme.textMuted, scheme.surfaceRaised)).toBeGreaterThanOrEqual(4.5)
  })

  it.each(schemes)(
    '$name: control borders (--line-strong) are >=3:1 on canvas and on inputs',
    (scheme) => {
      expect(contrast(scheme.lineStrong, scheme.canvas)).toBeGreaterThanOrEqual(3)
      expect(contrast(scheme.lineStrong, scheme.surfaceInput)).toBeGreaterThanOrEqual(3)
    }
  )

  it.each(schemes)(
    '$name: the accent used as text (eyebrows, links) is >=4.5:1 on canvas',
    (scheme) => {
      expect(contrast(scheme.accent, scheme.canvas)).toBeGreaterThanOrEqual(4.5)
    }
  )

  it.each(schemes)('$name: the focus ring is >=3:1 on canvas', (scheme) => {
    expect(contrast(scheme.focus, scheme.canvas)).toBeGreaterThanOrEqual(3)
  })

  it.each(schemes)('$name: the primary button label is >=4.5:1 on its accent fill', (scheme) => {
    expect(contrast(scheme.accentInk, scheme.accent)).toBeGreaterThanOrEqual(4.5)
  })
})
