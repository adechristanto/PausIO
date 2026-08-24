# PausIO Roadmap

This document outlines the planned milestones and development directions for PausIO. Priorities may adjust based on community feedback and contributions.

---

## 1. Desktop Experience & Platform Parity

- [x] **Core Engine Isolation**: Pure Rust state machine with 90%+ line coverage and deterministic countdown behavior.
- [x] **Multi-Surface Break Delivery**: Tray controls, gentle/assertive screen-dimming overlays, and break warning prompts.
- [x] **Smart Context Deferrals**: Aggregate OS idle/lock detection and fullscreen/Focus Assist deferrals on Windows and macOS.
- [ ] **Linux Wayland Native Parity**:
  - Replace `loginctl` subprocess polling with native D-Bus session events via `zbus`.
  - Implement idle detection via `ext-idle-notify-v1` and logind signals.
  - Overlay surface hardening on wlroots/KDE compositors via `gtk-layer-shell`.
  - Detailed technical spec: [`docs/LINUX_WAYLAND_PLAN.md`](docs/LINUX_WAYLAND_PLAN.md).
- [ ] **Native Break Sound Cues on Windows & Linux**:
  - Parity with macOS system sound synthesis using low-latency platform audio APIs.

---

## 2. Packaging, Signing & Distribution

- [x] **Unsigned GitHub Releases**: Automated CI/CD release matrix for Windows (`.exe`, `.msi`), macOS (`.dmg`, `.app`), Android (`.apk`), and iOS (`.ipa`).
- [ ] **Code Signing & Notarization**:
  - Apple Developer ID signing and notarization for macOS.
  - Microsoft Authenticode / Azure Trusted Signing for Windows.
  - See [`docs/RELEASE_PIPELINE.md`](docs/RELEASE_PIPELINE.md) for signing and updater architecture.
- [ ] **Package Manager Distribution**:
  - **macOS**: Official Homebrew Cask (`brew install --cask pausio`).
  - **Windows**: Windows Package Manager (`winget install PausIO.PausIO`).
  - **Linux**: Flathub (Flatpak) and Arch User Repository (AUR).
- [ ] **In-App Auto-Updater**:
  - Opt-in updater integration via `tauri-plugin-updater` with signed update manifests.

---

## 3. Mobile & Wearable Companions

- [x] **Deterministic Mobile Generation**: Single codebase generating iOS and Android companion shells.
- [x] **Wearable Bridge Contracts**: Apple Watch (SwiftPM/WatchConnectivity) and Wear OS (Compose/Data Layer) schedule synchronization.
- [ ] **Hardware Battery Benchmarking**:
  - Real-world power consumption validation against target thresholds (≤3%/day on watchOS, ≤4%/day on Wear OS).
- [ ] **Store Releases**:
  - Apple App Store & TestFlight beta distribution.
  - Google Play Store & Wear OS track distribution.
- [ ] **Interactive Complications & Tiles**:
  - Quick glance at next break time directly on watch faces.

---

## 4. Community & Localization

- [x] **English and German** full interface and notification support.
- [ ] **Weblate / Crowdin Integration**: Open platform for community translations.
- [ ] **Accessibility (a11y) Audits**: High-contrast modes, screen-reader navigation optimizations, and full keyboard-only workflows.

---

## Contributing

Have an idea or want to help implement a roadmap item? Check out [`CONTRIBUTING.md`](CONTRIBUTING.md) or join the discussion in [GitHub Issues](https://github.com/adechristanto/PausIO/issues).
