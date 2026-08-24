# Security Policy

## Supported versions

PausIO is at an early development stage. Only the current `main` branch receives fixes.

## Threat model

PausIO is a local-first desktop application. It:

- Makes no outbound network connections on its own behalf.
- Stores all settings and history locally using `tauri-plugin-store`.
- Stores that application data as local JSON rather than encrypting it itself; protection at rest relies on the operating system account and disk controls.
- Never reads application names, window titles, screen contents, audio, camera data, or keystrokes.
- Uses only aggregate, permission-free OS state (system idle time, session lock events, and on Windows, `SHQueryUserNotificationState`) for context detection.

The `e2e-webdriver` Cargo feature compiles in an unauthenticated localhost WebDriver endpoint used by the automated test suite. This feature must never be enabled in any build intended for installation. It is gated at the `Cargo.toml` level — not merely by a runtime environment variable — precisely so release artifacts structurally cannot contain it.

## Reporting a vulnerability

Please use [GitHub private vulnerability reporting](https://github.com/adechristanto/PausIO/security/advisories/new) to report security issues. This keeps the report confidential while it is being assessed.

Do not open a public GitHub issue for a security vulnerability.

**What to include:**

- A clear description of the issue and its potential impact.
- Steps to reproduce, including any relevant platform (macOS/Windows/Linux version, desktop environment if Linux).
- Whether it requires physical access to the device, a local account, or can be triggered remotely.

You can expect an acknowledgement within 5 business days. If the issue is confirmed, a fix will be prepared before any public disclosure.

Please allow the maintainers to coordinate a fix and disclosure timeline before publishing technical details.

## Scope

Reports are in scope for:

- Local data exposure or exfiltration.
- The `e2e-webdriver` endpoint being reachable in a non-test build.
- Privilege escalation via the Tauri IPC or capability system.
- Break overlay bypass that leaves the machine unusable without physical intervention.

Reports are out of scope for:

- Theoretical attacks that require the attacker to already have user-level access to the machine.
- Issues in Tauri, WebKit, or other upstream dependencies — report those upstream.
