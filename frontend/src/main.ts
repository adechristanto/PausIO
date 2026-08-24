import './styles.css'
import App from './App.svelte'
import { mount } from 'svelte'
import { invoke } from '@tauri-apps/api/core'

// The WDIO bridge is bundled only into the explicit E2E build. It is absent
// from normal desktop/mobile artifacts and has no production runtime role.
if (import.meta.env.VITE_E2E === '1') {
  // The WebdriverIO service introspects this historic global. Keep the bridge
  // exclusively in the dedicated test artifact; production windows use the
  // module API and do not expose a global Tauri surface.
  ;(
    window as Window & {
      __TAURI__?: { core: { invoke: typeof invoke } }
    }
  ).__TAURI__ = { core: { invoke } }
  void import('@wdio/tauri-plugin')
}

mount(App, { target: document.getElementById('app')! })
