# Contributing to PausIO

Thank you for your interest in contributing. This file explains how to get a working development environment, run the tests, and submit a change.

## Core principles

PausIO is local-first and privacy-preserving. Contributions must not add:

- Accounts, sign-in flows, or cloud sync.
- Analytics, telemetry, or any form of usage tracking.
- Network relay or server-to-client push that is not initiated by the local device.
- Any API that reads application names, window titles, screen content, audio, camera data, or keystrokes.

## Prerequisites

| Tool              | Version           | Notes                                                                    |
| ----------------- | ----------------- | ------------------------------------------------------------------------ |
| Rust              | 1.93.0            | Pinned via `rust-toolchain.toml`; `rustup` will install it automatically |
| Node.js           | 22.22.2           | Use `.nvmrc` or any version manager                                      |
| pnpm              | 10.33.0           | `npm install -g pnpm@10.33.0` or via `corepack`                          |
| Java              | 17                | Required only for Android/Wear OS work                                   |
| Xcode             | 16+               | Required only for iOS/Apple Watch work (Swift 6 requires Xcode 16)       |
| Android SDK + NDK | NDK 28.2.13676358 | Required only for Android/Wear OS work                                   |
| xcodegen          | latest            | Required only for iOS simulator builds (`brew install xcodegen`)         |

A desktop-only build requires only Rust, Node, and pnpm.

## First run

```sh
git clone https://github.com/adechristanto/PausIO.git
cd PausIO
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm tauri dev` starts the Vite dev server at `localhost:1420` and launches the Tauri app pointing at it. Hot-reload works for frontend changes; Rust changes require a restart.

## Running the checks

Run all checks before opening a pull request:

```sh
# Frontend: formatting, type check, unit tests, production build
pnpm format:check
pnpm check && pnpm test && pnpm build

# Rust workspace: format, clippy, unit tests, coverage
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo llvm-cov -p pausio-core --lib --fail-under-lines 90 --summary-only
cargo deny check advisories licenses bans sources

# Shipped JavaScript dependency advisories
pnpm audit --prod --audit-level high

# Apple Watch (requires Xcode)
(cd watch/apple-watch && swift test)

# Desktop E2E
pnpm test:e2e:desktop
```

CI runs the full matrix including Android and iOS simulator builds. Those require the Android SDK and Xcode and are not expected to run locally on every machine.

## Layer-specific notes

### Timer engine (`crates/pausio-core`)

The engine has no dependency on Tauri or any UI. Keep it that way. A 90% line-coverage floor is enforced by CI. If you add a branch, add a test for it.

### Shared contract (`crates/pausio-protocol`)

This crate is dual-licensed (`MIT OR Apache-2.0`) so watch companion apps can embed it without GPL inheritance. All changes must maintain that license. If the `WatchSettingsEnvelopeV1` JSON schema changes, update `tests/fixtures/watch-settings-v1.json` too.

### Desktop shell (`src-tauri/src/`)

The concurrency invariant is documented on `EngineView` in `src-tauri/src/state.rs`. The short version: never hold the `EngineState` mutex across `emit`, tray mutation, or window create/close. Use `EngineView` + `drain_and_emit`.

### Mobile hosts

The generated iOS and Android host projects in `src-tauri/gen/` are not committed. Do not edit them by hand. Regenerate with:

```sh
pnpm gen:mobile
```

The patch script (`scripts/patch-mobile-projects.sh`) applies idempotent changes to the generated projects. The generation should be deterministic; CI verifies this by running it twice and diffing checksums.

## Branch naming

```
feat/<description>
fix/<description>
docs/<description>
refactor/<description>
ci/<description>
chore/<description>
```

Use kebab-case. Keep descriptions short (`feat/overlay-watchdog`, not `feat/add-the-overlay-watchdog-timeout-mechanism`).

## Commit convention

[Conventional Commits](https://www.conventionalcommits.org/), with scopes matching workspace members:

| Scope      | Where                                   |
| ---------- | --------------------------------------- |
| `core`     | `crates/pausio-core`                    |
| `protocol` | `crates/pausio-protocol`                |
| `desktop`  | `src-tauri/`                            |
| `frontend` | `frontend/`                             |
| `watch`    | `watch/`                                |
| `plugin`   | `plugins/`                              |
| `ci`       | `.github/workflows/`                    |
| `docs`     | `docs/`, `README.md`, `CONTRIBUTING.md` |

Examples:

```
feat(desktop): add overlay watchdog for non-dismissible breaks
fix(core): credit natural break when lock is shorter than idle threshold
docs(protocol): document WatchSettingsEnvelopeV1 revision semantics
```

## Pull request expectations

- One logical change per PR. Large mechanical changes (reformats, renames) in isolation.
- All CI jobs must pass before merge.
- Describe what the change does and why, not just what files changed.
- If the change touches the engine's deadlock invariant, the overlay lifecycle, or the watch sync path, call it out explicitly.
- Never attach artifacts from an `--all-features` or `e2e-webdriver` build to a release.
- Report suspected vulnerabilities privately as described in `SECURITY.md`.

## Non-goals & Scope boundaries

The following are deliberately out of scope:

- Cloud backup, user accounts, or remote sync of settings/history.
- Network-based reminders or server-to-client push notifications.
- Screen-time tracking or application monitoring (PausIO counts only its own active work intervals).
- Third-party analytics or telemetry SDKs.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Enforcement contact: [@adechristanto](https://github.com/adechristanto).
