import { beforeEach, describe, expect, it, vi } from 'vitest'
import fixture from '../../../tests/fixtures/watch-settings-v1.json'
import type { WatchSettingsEnvelopeV1 } from './types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))

describe('PausIO IPC facade', () => {
  beforeEach(() => invoke.mockReset())
  it('sends settings as the explicit command payload', async () => {
    const { api } = await import('./pausio')
    const settings = { work_seconds: 1200 } as never
    await api.setSettings(settings)
    expect(invoke).toHaveBeenCalledWith('set_settings', { settings })
  })
  it('uses the stable break command names', async () => {
    const { api } = await import('./pausio')
    await api.startSession()
    await api.startDueBreak()
    await api.takeBreakNow()
    await api.postponeBreak()
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      'start_session',
      'start_due_break',
      'take_break_now',
      'postpone_break',
    ])
  })
  it('exposes the stable watch diagnostic command names', async () => {
    const { api } = await import('./pausio')
    await api.syncWatchSettings()
    await api.sendTestNudge()
    await api.getWatchStatus()
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      'sync_watch_settings',
      'send_test_nudge',
      'get_watch_status',
    ])
  })
  it('uses explicit login-start and E2E screen-lock command payloads', async () => {
    const { api } = await import('./pausio')
    await api.getAutostartStatus()
    await api.setAutostartEnabled(true)
    await api.e2eSimulateScreenLock(20)
    expect(invoke.mock.calls).toEqual([
      ['get_autostart_status'],
      ['set_autostart_enabled', { enabled: true }],
      ['e2e_simulate_screen_lock', { lockedSeconds: 20 }],
    ])
  })
  it('uses named local settings profiles', async () => {
    const { api } = await import('./pausio')
    await api.getSettingsProfiles()
    await api.saveSettingsProfile('work')
    await api.applySettingsProfile('home')
    expect(invoke.mock.calls).toEqual([
      ['get_settings_profiles'],
      ['save_settings_profile', { name: 'work' }],
      ['apply_settings_profile', { name: 'home' }],
    ])
  })
  it('exposes timed tray-pause and private-history commands', async () => {
    const { api } = await import('./pausio')
    await api.pauseForMinutes(30)
    await api.getHistory()
    await api.clearHistory()
    await api.resetLocalData()
    expect(invoke.mock.calls).toEqual([
      ['pause_for_minutes', { minutes: 30 }],
      ['get_history'],
      ['clear_history'],
      ['reset_local_data'],
    ])
  })
  it('sends only a transient context reason to the adaptive engine', async () => {
    const { api } = await import('./pausio')
    await api.setContext('screen_share')
    await api.setContext(null)
    expect(invoke.mock.calls).toEqual([
      ['set_context', { context: 'screen_share' }],
      ['set_context', { context: null }],
    ])
  })
  it('reads the shared watch settings fixture and ignores future fields', () => {
    const { future_field: _futureField, ...known } = fixture
    const contract = known as WatchSettingsEnvelopeV1
    expect(contract).toMatchObject({ schema_version: 1, revision: 7, timezone: 'Europe/Berlin' })
    expect(JSON.parse(JSON.stringify(contract))).not.toHaveProperty('future_field')
  })

  it('models additive deadline fields without requiring legacy callers to send them', () => {
    const contract: WatchSettingsEnvelopeV1 = {
      schema_version: 1,
      revision: 8,
      timezone: 'Europe/Berlin',
      work_interval_seconds: 1200,
      short_break_seconds: 20,
      long_break_seconds: 300,
      pre_break_seconds: 30,
      active_days_mask: 127,
      active_start_minutes: 0,
      active_end_minutes: 0,
      paused: false,
      updated_at: '2026-08-03T12:00:00Z',
      phase: 'breaking',
      phase_deadline_at: '2026-08-03T12:00:20Z',
      break_active: true,
      break_kind: 'short',
    }
    expect(contract.phase_deadline_at).toBe('2026-08-03T12:00:20Z')
  })
})
