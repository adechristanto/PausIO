# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Multi-platform GitHub Actions release-candidate pipeline (`.github/workflows/release.yml`) that verifies tagged source, builds unsigned engineering artifacts, and creates a private draft prerelease pending signing and device validation.
- Public `ROADMAP.md` outlining desktop parity, packaging/signing, wearable benchmarks, and localization.
- Open-source contract files: `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1), `SECURITY.md` (GitHub private vulnerability reporting), `CHANGELOG.md`, issue templates, PR template, `.editorconfig`, and `.git-blame-ignore-revs`.
- `LICENSE-MIT` and `LICENSE-APACHE` texts in `crates/pausio-protocol/`, completing its declared `MIT OR Apache-2.0` dual license.
- `docs/architecture.md`, written from the code, replacing the pre-implementation developer documentation.
- Prettier with `prettier-plugin-svelte`, enforced by `pnpm format:check` in CI.
- `deny.toml` and a `security.yml` workflow running cargo-deny (advisories, licenses, bans, sources) on push and weekly.
- Dependabot for Cargo, npm, and GitHub Actions.

### Fixed

- Restored the dashboard's visible title and subtitle. Moving the phase eyebrow above the
  timer ring had removed them and left only an `sr-only` heading, which the existing
  `getByRole('heading')` assertion could not detect.
- Windows: the app could freeze permanently around a break transition (most visibly the 20-second short break). Publication of engine transitions serialised on a mutex that the 1 s tick loop acquired while still holding the engine mutex, while another publisher held it and waited on the main event loop — and any main-thread command calling `lock_engine` then closed a three-way cycle. All publication now runs on a single dedicated thread fed by a queue; no thread that holds the engine mutex ever waits on native UI dispatch, and queue order equals mutation order.
- Windows: a break overlay re-asserted `MarkFullscreenWindow(hwnd, true)` from its own `Focused(false)` handler while it was being destroyed, leaving the taskbar retracted after the break had ended. `OVERLAY_GENERATION` is now bumped before teardown and checked by the handler, and the fullscreen mark is released against a handle captured before the destroy.

### Changed

- Rebranded the project from Pauvio to PausIO: renamed the Rust crates (`pausio-core`,
  `pausio-protocol`, `pausio`/`pausio_lib`), npm packages (`pausio`, `@pausio/frontend`),
  Tauri product name and bundle identifier (`com.pausio.app`), Wear OS and watchOS companion
  apps and their bundle identifiers, all internal type/module/package names, and every
  user-visible string and asset. Local desktop store filenames changed from
  `pauvio-settings.json`/`pauvio-history.json` to `pausio-settings.json`/`pausio-history.json`,
  so existing local installs will start from empty settings/history on first launch after
  upgrading. Paired Wear OS/watchOS companions must be rebuilt and reinstalled alongside the
  desktop/mobile app, since the watch wire-protocol path and capability strings changed too.
- Redrew the PausIO logo as a single clean eye on a rounded-square field, and derived its
  geometry from the tray glyph in `src-tauri/src/tray_icon.rs` rather than drawing it by
  hand, so the app icon and the menu-bar mark are now the same outline at two sizes. The
  previous mark layered an eye, a timer dial, a knob and a sparkle behind two nested frames,
  which read as noise at the 26 px the sidebar actually shows it at. All raster app icons
  (desktop, iOS, Android) regenerated from it.
- Dropped the brand mark from the break overlay. The break is the subject of that screen.
- Rewrote `README.md` for a public audience: purpose, status, platform support, prerequisites, build/verify commands, license split.
- Expanded `CONTRIBUTING.md` with first-run walkthrough, per-layer test commands, branch naming, Conventional Commits scopes, and PR expectations.
- CI restructured: single macOS monolith split into `frontend`, `rust`, `swift`, `mobile-gen`, `e2e`, `ios`, `android` jobs; added `concurrency` cancellation, `Swatinem/rust-cache`, and push-trigger limited to `main`.
- Split `src-tauri/src/lib.rs` (2,779 lines) into focused modules: `state`, `store`, `events`, `break_windows`, `main_window`, `tray_menu`, `commands`, and `platform/{macos,windows,linux}`. Pure file moves; no logic changes.
- Corrected workspace `repository` metadata to `adechristanto/PausIO`; added `homepage` and `rust-version`.
- Extended `.gitignore` for editor directories, log files, and agent local settings; fixed `.claude/launch.json` to use pnpm.

### Removed

- Internal pre-implementation planning documents (`market-analysis.md`, `desktop-audit-and-freemium-playbook.md`, `prd.md`, `WEARABLES_CLOSED_BETA.md`, `M1_DECISIONS.md`, `Open-Source Release Preparation.md`).
- Pre-implementation artifacts from `docs/`: six generated `.docx` exports, three degraded `.converted.md` round-trips, an agent workflow note, and raw research dumps (≈2.5 MB total).
- `docs/developer-documentation.md`, which described an architecture the code does not have.

## [0.1.0] - 2026-08-02

Initial working state. Not yet distributed as a signed release.

### Added

- Deterministic 20-20-20 timer engine (`crates/pausio-core`) with pause/resume, long-break cadence, fixed break times, daily focus limit, context deferral, and a versioned session checkpoint. 90% line-coverage floor enforced in CI.
- Shared wire contract (`crates/pausio-protocol`, MIT OR Apache-2.0) with versioned `WatchSettingsEnvelopeV1` and a committed JSON fixture.
- Tauri v2 desktop shell (macOS, Windows, Linux): system tray with quick-controls popover, four break delivery modes (Gentle/Balanced/Firm/Strict), per-display targeting, session lock and idle detection, optional automatic context deferral on Windows, global shortcuts, login autostart, English and German.
- Non-dismissible strict break overlay with platform hardening (NSScreenSaverWindowLevel on macOS, taskbar retraction on Windows) and a last-resort watchdog.
- Opt-in local history (off by default) with configurable retention and JSON/CSV export; health report and test reminder in settings.
- iOS + Apple Watch bridge and Android + Wear OS bridge via `tauri-plugin-eyecare`; watches keep a bounded offline reminder schedule after settings sync.
- Svelte 5 frontend with Vitest component tests and a WebdriverIO desktop E2E suite (gated behind the opt-in `e2e-webdriver` Cargo feature).
- CI covering desktop bundles (Linux deb/appimage, Windows nsis/msi), Swift tests, Android lint/build, iOS simulator build, and byte-for-byte determinism of the mobile-host generator.

[Unreleased]: https://github.com/adechristanto/PausIO/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/adechristanto/PausIO/releases/tag/v0.1.0
