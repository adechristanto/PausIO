# PausIO — Architecture

This document describes the repository as it exists. All paths, type names, and commands resolve in the tree.

---

## Repository layout

```
PausIO/
├── crates/
│   ├── pausio-core/          # Deterministic timer engine (GPL-3.0-only)
│   └── pausio-protocol/      # Shared JSON/serde contract (MIT OR Apache-2.0)
├── plugins/
│   └── tauri-plugin-eyecare/ # Rust + Swift (iOS) + Kotlin (Android) watch bridge
├── src-tauri/                # Tauri v2 desktop application shell
│   └── src/
│       ├── lib.rs            # Application logic (state, commands, platform, tray)
│       ├── i18n.rs           # English/German string catalogue
│       ├── session_monitor.rs# macOS/Windows session lock monitoring
│       └── tray_icon.rs      # SVG tray icon renderer
├── frontend/
│   └── src/
│       ├── App.svelte         # Root component (settings, history, timer display)
│       ├── components/        # BreakOverlay, BreakPrompt, TimerRing, …
│       └── lib/               # errors, format, i18n, pausio, sound, types, …
├── watch/
│   ├── apple-watch/          # SwiftPM package — watchOS companion
│   └── wear-os/              # Gradle project — Wear OS companion
├── scripts/                  # Build helpers (mobile generation, iOS recovery)
├── tests/                    # WebdriverIO E2E desktop suite
└── docs/                     # This directory
```

---

## Layer overview

### `crates/pausio-core` — timer engine

The engine is a pure-Rust state machine with no dependency on Tauri, webviews, or any UI. Its public API surface is about 16 items: `TimerEngine`, `Settings`, `Snapshot`, `SessionCheckpoint`, `EngineEvent`, `EngineError`, and the presentational enums (`Locale`, `Strictness`, `Theme`, `Accent`, `DisplayTarget`, `BreakRoutine`, `SoundTheme`). All live in `crates/pausio-core/src/lib.rs` (a single file; no subdirectory).

The engine is the only component covered by the 90% line-coverage gate (`cargo llvm-cov -p pausio-core --lib --fail-under-lines 90`). Its determinism is the foundation the rest of the stack relies on.

Key engine calls used by the desktop shell:

| Method                                                                          | Effect                                           |
| ------------------------------------------------------------------------------- | ------------------------------------------------ |
| `TimerEngine::new(settings, now)`                                               | Creates an engine in `Dormant` phase             |
| `TimerEngine::restore(settings, checkpoint, now)`                               | Recovers from a persisted checkpoint             |
| `engine.advance(elapsed_seconds, now)`                                          | Normal per-tick update                           |
| `engine.woke_after(elapsed_seconds)`                                            | Called when a tick gap ≥ 30 s is detected        |
| `engine.report_idle(idle_seconds)`                                              | Platform idle signal → `Paused { reason: Idle }` |
| `engine.activity_resumed()`                                                     | Idle cleared                                     |
| `engine.screen_locked()` / `screen_unlocked(locked_seconds, now)`               | Session lock events                              |
| `engine.start_session()` / `pause()` / `resume()`                               | Session lifecycle                                |
| `engine.take_break_now()` / `start_due_break()` / `skip_break()` / `postpone()` | Break controls                                   |
| `engine.replace_settings(settings, now)`                                        | Live settings update                             |
| `engine.set_context(reason)` / `set_context_for(reason, minutes)`               | Defer-break context                              |
| `engine.snapshot()`                                                             | Lock-free view of current phase and countdown    |
| `engine.settings()`                                                             | Reference to current settings                    |
| `engine.checkpoint()`                                                           | Serializable session state for persistence       |

### `crates/pausio-protocol` — shared contract

A deliberately small, dual-licensed (`MIT OR Apache-2.0`) crate containing only the types exchanged between the phone and watch: `TimerPhase`, `BreakKind`, `PauseReason`, `ContextReason`, `WatchSettingsEnvelopeV1`, `WatchStatus`, and `NudgeResult`. No framework or UI dependency. Its license boundary is intentional: watch companion apps can embed it without GPL inheritance.

The wire contract is versioned (`SCHEMA_VERSION = 1`). The contract fixture at `tests/fixtures/watch-settings-v1.json` is part of the test suite and must be updated whenever the envelope schema changes.

### `plugins/tauri-plugin-eyecare` — watch bridge

A standard Tauri v2 plugin that wraps:

- **iOS** (`ios/`): Swift code using WatchConnectivity to send `WatchSettingsEnvelopeV1` to the Apple Watch companion and receive back actions from the ring.
- **Android** (`android/`): Kotlin code using the Wearable Data Layer API for the equivalent Wear OS flow.
- **Rust** (`src/lib.rs`): The plugin's Tauri glue: `sync_settings`, `send_test_nudge`, `status`, and `take_pending_action`.

The plugin is linked only into iOS and Android hosts. Desktop builds do not contain the plugin, watch commands, watch-envelope persistence, or a desktop-to-phone relay.

### `src-tauri` — desktop shell

The Tauri desktop shell lives in `src-tauri/src/lib.rs`. It holds:

- **Shared state types**: `EngineState` (a `Mutex<TimerEngine>`), `SessionLockState`, `HistoryTracker`, `EngineView`, `PUBLISHER`.
- **Error types**: `ApiError`, `ApiResult`.
- **Store persistence**: `persist_settings`, `persist_session`, `append_history`, and helpers for two store files (`pausio-settings.json`, `pausio-history.json`). History lives in its own store so frequent session heartbeats (every 30 s) never rewrite the larger history array.
- **Mobile watch sync**: `next_watch_settings_envelope`, `deliver_watch_settings`, `sync_watch_state`; these paths compile only for iOS and Android hosts.
- **Event emission**: `emit`, `emit_tick`. Tick events are skipped for hidden windows to avoid waking every webview every second.
- **Platform adapters** (guarded by `#[cfg]`):
  - `platform_idle_seconds()` — macOS: `CGEventSourceSecondsSinceLastEventType`; Linux: `loginctl show-session` polled at 10 s intervals via `LOGINCTL_CACHE`; Windows: `GetLastInputInfo`.
  - `platform_context_signal()` — Windows only: `SHQueryUserNotificationState`.
  - `platform_session_locked()` — Linux only via the `loginctl` cache.
  - `sync_linux_session_lock()` — polls the cache each tick, fires `handle_session_event` on edge transitions.
- **Break windows**: `show_break_prompt`, `show_break_overlays`, `close_break_prompt`, `close_break_overlays`, `close_break_windows`. The overlay is hardened platform-specifically: `NSScreenSaverWindowLevel` on macOS, `HWND_TOPMOST + MarkFullscreenWindow` on Windows. `OVERLAY_GENERATION` is bumped on each teardown so the `spawn_overlay_watchdog` can detect stale breaks.
- **Main window / tray**: `show_main_window`, `hide_main_window`, `install_main_window_lifecycle`, `push_state_resync`.
- **Tray menu**: `build_tray`, `update_tray_state`, `retranslate_tray`, `TrayMenuItems`, `TRAY_MENU_ITEMS`, `TRAY_ICON`. Distinct from `tray_icon.rs`, which handles the SVG icon itself.
- **App menu** (macOS only): `build_app_menu`, `QUIT_MENU_ITEM`. A custom macOS menu keeps standard App/Edit/Window submenus while replacing the Quit item so `set_quit_enabled` can block `Cmd+Q` during a non-dismissible break.
- **Commands**: 30 `#[tauri::command]` handlers wired in `run()` via `generate_handler!`.
- **`run()`**: 430-line builder assembling plugins, managed state, setup closure, and tick loop.

The concurrency invariant is documented on `EngineView` in `state.rs` and enforced by the `EngineView` / `publish` / publisher-thread pattern: the engine mutex is never held across any `emit`, tray mutation, or window create/close call, because those dispatch to — and can block on — the main event loop, while several commands run on that same loop and call `lock_engine` themselves.

Publication is funnelled through a single dedicated thread (`state::install_publisher`) fed by an unbounded queue. Every mutation path — commands, tray and shortcut callbacks, session events, the 1 s tick loop — captures an `EngineView`, enqueues it while still holding the engine guard, and drops the guard. Only the publisher thread ever calls `emit`, so:

- no thread that holds the engine mutex ever waits on native UI dispatch, which is what makes the AB-BA deadlock impossible rather than merely unlikely;
- queue order equals mutation order, so a break's overlay can never be created after the teardown that superseded it;
- one slow window create/destroy queues work instead of fanning out threads that all block on the same event loop.

Countdown-only batches that a newer batch has already superseded are dropped rather than replayed, so a publisher that falls behind catches up instead of lagging further. The 30-second session-checkpoint heartbeat also lives on this thread, making it the only writer of that key.

Module files left untouched by the architecture split:

- `i18n.rs` — English/German compile-time string catalogue keyed on `Locale`.
- `session_monitor.rs` — macOS `NSWorkspace` notifications and Windows `WTSRegisterSessionNotification` for instantaneous lock/unlock events.
- `tray_icon.rs` — single static SVG rendered with `resvg`; stays a template-image silhouette on macOS rather than per-state bitmaps.

### `frontend` — Svelte 5 UI

Svelte 5 (runes) + TypeScript + Vite 7. No server-side rendering, no build-time data fetching, no router library. View routing is query-string based (`?view=break-overlay`, `?view=break-prompt`), handled by `App.svelte` directly.

Persistence is entirely Rust-side via `tauri-plugin-store`. The frontend never calls storage APIs directly; it receives state via `state:changed` / `settings:changed` events and issues Tauri commands.

Key frontend modules:

- `src/lib/types.ts` — TypeScript types mirroring the Rust `Snapshot`, `Settings`, `HistoryEvent`, etc.
- `src/lib/pausio.ts` — Tauri command wrappers and event subscriptions.
- `src/lib/i18n.ts` — Frontend i18n catalogue (matches `src-tauri/src/i18n.rs`).
- `src/lib/errors.ts`, `format.ts`, `history-analytics.ts`, `sound.ts`, `tooltip.ts` — utilities.
- `src/components/` — `BreakOverlay.svelte`, `BreakPrompt.svelte`, `TimerRing.svelte`, `ShortcutField.svelte`, `SettingsPanel.svelte`, `HistoryPanel.svelte`, `Onboarding.svelte`, `NudgeToast.svelte`, `Advanced.svelte`.

### Watch companions

Both companions receive `WatchSettingsEnvelopeV1` from the phone bridge and maintain an offline reminder schedule from it. They use absolute deadlines locally rather than one-second phone messages, retain only newer revisions, and acknowledge applied revisions. Neither makes network calls.

Apple Watch communication is strictly iPhone-to-watch through WatchConnectivity. Wear OS communication is Android-phone-to-watch through the Wearable Data Layer. Desktop builds do not communicate with wearables, directly or through a phone relay.

- **`watch/apple-watch/`** — SwiftPM package, Swift 6, targets watchOS. Tested with `swift test` from that directory.
- **`watch/wear-os/`** — Gradle project, Kotlin. Tested with `./gradlew :wear:testDebugUnitTest`.

### Scripts

| Script                             | Role                                                                                                                                                                                                                                          |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/tauri.mjs`                | Node.js launcher: changes to `src-tauri/`, then delegates to the locally-installed `@tauri-apps/cli`. Used by `pnpm tauri`.                                                                                                                   |
| `scripts/tauri.sh`                 | POSIX entry point for the same operation. Required by Xcode build phases and the Gradle `BuildTask.kt` patch (`scripts/patch-mobile-projects.sh:14,62`). Not a duplicate of `tauri.mjs` — removing it silently breaks iOS and Android builds. |
| `scripts/generate-mobile.sh`       | Thin wrapper around `pnpm tauri ios init` + `pnpm tauri android init`.                                                                                                                                                                        |
| `scripts/patch-mobile-projects.sh` | Idempotent patches for the generated iOS and Android projects: updates the Tauri entry point path and wires `tauri-plugin-eyecare`.                                                                                                           |
| `scripts/build-ios-simulator.sh`   | Runs `xcodegen` and `xcodebuild` for the iOS simulator target.                                                                                                                                                                                |

---

## Data stores

PausIO uses `tauri-plugin-store` for all local persistence. Two store files are created in the platform-standard app data directory:

| File                   | Contents                                                                                                                                                                                                                 |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `pausio-settings.json` | `settings` (serialized `Settings`), `session` (serialized `SessionCheckpoint`), `settings_profiles` (work/home snapshots); mobile hosts additionally keep `watch_revision` (monotonic u64) and the latest watch envelope |
| `pausio-history.json`  | `history` (array of `HistoryEvent`, capped at 5,000 entries)                                                                                                                                                             |

In E2E mode (`--e2e`), both stores are prefixed with `pausio-e2e-` and the session checkpoint is not restored, giving each test run a deterministic start.

---

## Build

```sh
# Node + Cargo workspace
pnpm install --frozen-lockfile

# Desktop development
pnpm tauri dev

# Frontend only (type check + unit tests + bundle)
pnpm check && pnpm test && pnpm build

# Rust workspace (format + lint + unit tests + coverage)
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo llvm-cov -p pausio-core --lib --fail-under-lines 90 --summary-only

# Watch companions
(cd watch/apple-watch && swift test)
./gradlew :wear:testDebugUnitTest   # from src-tauri/gen/android after pnpm gen:mobile

# E2E
pnpm test:e2e:desktop
```

Mobile host projects (`src-tauri/gen/`) are not committed. Generate them with `pnpm gen:mobile` on a machine with Xcode and Android SDK support.

---

## Security

The `e2e-webdriver` Cargo feature compiles in an unauthenticated localhost WebDriver endpoint. It must never be enabled for builds a person will install. The feature is gated at the `Cargo.toml` level (not only at runtime) so release artifacts structurally cannot contain it.

The engine never inspects application names, window titles, screen contents, audio, camera, or keystroke data. Platform idle and lock APIs are coarse device-state signals only. This is enforced by design and documented inline throughout `lib.rs` and `pausio-core/src/lib.rs`.

---

## Known limitations and technical debt

Tracked openly; each entry names what is blocking it.

| Item                                                                                      | Status                    | Blocker                                                                                                                                                                                                                      |
| ----------------------------------------------------------------------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Signed distribution (macOS Developer ID + notarization, Windows code signing)             | Blocked                   | Requires Apple Developer Program membership and a Windows code-signing certificate. See `docs/RELEASE_PIPELINE.md`.                                                                                                          |
| Linux/Wayland idle and lock parity                                                        | Not implemented           | X11-era `loginctl` polling covers only part of the Wayland landscape; per-compositor protocols (`ext-idle-notify`, D-Bus ScreenSaver) need design. See `docs/LINUX_WAYLAND_PLAN.md`.                                         |
| Watch battery targets unvalidated                                                         | Blocked                   | Requires sustained on-hardware measurement on Apple Watch and Wear OS devices.                                                                                                                                               |
| Unmaintained transitive dependencies (GTK3 via Tauri/Linux, `unic-*`, `proc-macro-error`) | Accepted risk, documented | Maintenance-status advisories only, no known vulnerabilities. Each RUSTSEC ID is listed with justification in `deny.toml`; cargo-deny re-checks weekly in CI. Revisit when Tauri migrates off GTK3 / urlpattern.             |
| macOS automatic context detection (fullscreen / Focus)                                    | Not implemented           | Needs either Accessibility permission or CoreGraphics window-list traversal; Focus state via `Assertions.json` false-positives. Reported honestly as unsupported in the desktop health report rather than silently no-oping. |
| Linux automatic context detection                                                         | Not implemented           | No portable, permission-free signal across X11/Wayland and GNOME/KDE/wlroots. Same honest-unsupported reporting as macOS.                                                                                                    |
