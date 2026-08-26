import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * #0b0d12 is the dark launch canvas color, duplicated across four independent files with no
 * shared source of truth (a Tauri window background can't reference a CSS custom
 * property). This test is the invariant that keeps them from drifting apart.
 */
describe('canvas color stays in sync across the shell, the document, and Rust', () => {
  it('matches in styles.css, index.html, tauri.conf.json, and the Rust window builders', () => {
    const css = readFileSync(resolve(process.cwd(), 'src/styles.css'), 'utf8')
    const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8')
    const conf = readFileSync(resolve(process.cwd(), '../src-tauri/tauri.conf.json'), 'utf8')
    const breakWindows = readFileSync(
      resolve(process.cwd(), '../src-tauri/src/break_windows.rs'),
      'utf8'
    )

    expect(css).toContain('#0b0d12')
    expect(html).toContain('#0b0d12')
    expect(conf).toContain('"transparent": true')
    expect(conf).toContain('"shadow": true')
    const appShell = css.match(/\.app-shell\s*\{([^}]*)\}/)?.[1]
    expect(appShell).toContain('border-radius: var(--radius-lg);')
    expect(appShell).toContain('box-shadow: var(--shadow);')
    expect(appShell).not.toContain('border: 1px solid var(--line-optical);')
    // The prompt's webview must stay transparent while its notification-style card
    // slides in; otherwise the native window paints an opaque rectangle first.
    expect(breakWindows).toContain('Color(11, 13, 18, 0)')
    expect(breakWindows).toContain('.transparent(true)')
  })
})
