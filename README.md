<div align="center">

<img src=".github/assets/PausIO_Banner.png" alt="PausIO, a local-first eye-care timer" width="100%" />

# PausIO

**A local-first 20-20-20 eye-care timer for desktop, mobile, and smartwatch.**

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0--only-blue.svg)](LICENSE)
[![Protocol: MIT OR Apache-2.0](https://img.shields.io/badge/Protocol-MIT%20OR%20Apache--2.0-green.svg)](crates/pausio-protocol)
[![CI](https://github.com/adechristanto/PausIO/actions/workflows/ci.yml/badge.svg)](https://github.com/adechristanto/PausIO/actions/workflows/ci.yml)

<br />

</div>

---

PausIO follows the 20-20-20 rule: every 20 minutes, take a 20-second break and look at
something 20 feet (6 meters) away.

It runs entirely on your own devices. There is no account, no cloud sync, and no
telemetry. Break delivery is configurable, from a quiet notification up to a
non-dismissible full-screen prompt, and the timer defers automatically when it detects
you're away, your screen is locked, or (on Windows) the system reports fullscreen or
Focus Assist.

---

## Features

- **System tray operation** — runs in the background with quick controls from the tray icon; no dock or taskbar clutter.
- **Four break delivery modes** — notify only, ask first (start now / postpone / timed break), a dismissible full-screen overlay, or a non-dismissible strict overlay with an emergency override.
- **Context-aware deferrals** — defers automatically on recent input activity, screen lock, and (macOS/Windows) fullscreen or Focus Assist. Deferrals rely only on aggregate OS state; PausIO never reads keystrokes, window titles, or screen content. A manual context (meeting, screen share, etc.) can also be set from Settings.
- **Idle and lock detection** — pauses when you step away; the time away counts toward your rest cycle.
- **Independent per-device timers** — desktop, phone, and watch each run their own 20-20-20 schedule. The phone pre-schedules its notifications with the OS, so reminders still arrive while the app is closed.
- **Optional wearable companion** — an Apple Watch or Wear OS companion can be connected from the phone's Settings for a private haptic nudge; off by default, and requires the paired phone app.
- **No accounts, no cloud sync, no telemetry.**
- **English and German** localization.

---

## Screenshots

The mobile and wearable companions are engineering previews; the screenshots below are
from those builds. Desktop screenshots are not available yet.

<div align="center">

|                                              Onboarding                                              |                                             Dashboard                                              |                                                  Active timer                                                  |
| :--------------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------------: | :------------------------------------------------------------------------------------------------------------: |
| <img src="docs/screenshots/android-onboarding.png" alt="PausIO onboarding on Android" width="220" /> | <img src="docs/screenshots/android-dashboard.png" alt="PausIO dashboard on Android" width="220" /> | <img src="docs/screenshots/android-active-timer.png" alt="PausIO active break timer on Android" width="220" /> |

<img src="docs/screenshots/wear-os.png" alt="PausIO break reminder on a Wear OS watch face" width="200" />

_Wear OS companion, an optional haptic nudge on the wrist._

</div>

---

## Release status

PausIO is in pre-release development. There are no signed production binaries yet;
build from source using the instructions below to evaluate the project.

The release-candidate CI workflow can create draft GitHub releases with unsigned
engineering artifacts. Those are for maintainer validation only and are not production
distributions. See [`docs/RELEASE_PIPELINE.md`](docs/RELEASE_PIPELINE.md) for what is
still required before a signed public release.

---

## Platform support

The timer itself runs standalone on every platform below. "Standalone" describes the
timer, not installation: the watch companions require their paired phone app to be
installed, since a watchOS or Wear OS app cannot be installed independently of it.

| Platform        | Timer runs standalone  | Current maturity              | Platform integration                                             |
| :-------------- | :--------------------- | :---------------------------- | :--------------------------------------------------------------- |
| **macOS**       | Yes                    | Local engineering validation  | System tray, native sound cues, aggregate idle detection         |
| **Windows**     | Yes                    | Build validated; runtime open | System tray, fullscreen / Focus Assist detection                 |
| **Linux**       | Yes                    | Build validated; runtime open | See [`docs/LINUX_WAYLAND_PLAN.md`](docs/LINUX_WAYLAND_PLAN.md)   |
| **Android**     | Yes                    | Engineering preview           | OS-scheduled local alarms; optional Wear OS companion            |
| **iOS**         | Yes                    | Engineering preview           | OS-scheduled local notifications; optional Apple Watch companion |
| **Apple Watch** | Requires paired iPhone | Simulator-tested preview      | Own offline schedule once paired; syncs from iPhone              |
| **Wear OS**     | Requires paired phone  | Emulator-tested preview       | Own offline alarms once paired; syncs from Android               |

Desktop builds contain no wearable code; the watch bridge is compiled only into the
iOS and Android hosts.

---

## Building from source

```bash
git clone https://github.com/adechristanto/PausIO.git
cd PausIO
pnpm install --frozen-lockfile
pnpm tauri dev
```

For full prerequisites (Rust, Node, pnpm, and the optional Android/iOS toolchains),
per-layer test commands, and coding conventions, see
[**CONTRIBUTING.md**](CONTRIBUTING.md).

---

## Privacy and threat model

- **No telemetry or analytics.** No third-party trackers, crash reporters, or behavioral tracking.
- **Local data only.** Preferences, schedules, and history are stored in local JSON files via `tauri-plugin-store`. PausIO does not encrypt this data itself; at-rest protection depends on your OS account and disk-encryption settings.
- **No surveillance inputs.** PausIO never accesses your webcam, microphone, window titles, keystrokes, or screen contents. Context deferrals use only aggregate OS idle/lock state and, on Windows, `SHQueryUserNotificationState`.

See [`SECURITY.md`](SECURITY.md) for the full threat model and vulnerability disclosure process.

---

## Architecture and documentation

- [**Architecture overview**](docs/architecture.md) — crate breakdown, IPC events, and state machine design.
- [**Roadmap**](ROADMAP.md) — planned features and milestones.
- [**Release pipeline**](docs/RELEASE_PIPELINE.md) — what remains before a signed public release.
- [**Linux/Wayland plan**](docs/LINUX_WAYLAND_PLAN.md) — design for native D-Bus/compositor idle and lock detection (not yet implemented).
- [**Lifecycle test matrix**](docs/LIFECYCLE_TEST_MATRIX.md) — manual hardware QA checklist run before each release.

---

## Contributing

Contributions are welcome. Read [**CONTRIBUTING.md**](CONTRIBUTING.md) for the
development setup, branch conventions, and PR expectations, and follow the
[**Code of Conduct**](CODE_OF_CONDUCT.md).

---

## License

- Application shell and timer engine: **[GPL-3.0-only](LICENSE)**.
- Shared protocol crate (`crates/pausio-protocol`): **[MIT](crates/pausio-protocol/LICENSE-MIT) OR [Apache-2.0](crates/pausio-protocol/LICENSE-APACHE)**, so watch companions can link it without GPL inheritance.
