# Linux/Wayland parity: implementation spec

Status: not implemented. This is a spec for a contributor with a Linux build
environment; cross-compiling the GTK/D-Bus FFI involved (`gobject-sys`,
`gio-sys`, `pango-sys`, `cairo-sys-rs`) requires `pkg-config` against
target-native libraries that a macOS/Windows dev machine cannot provide, so
none of the code below has been compiled or run. Getting the FFI/D-Bus call
shapes wrong can crash the native process, so treat this as a design to
implement and verify on real hardware, not as reviewed code. Open questions
that need a real session bus or compositor to resolve are called out
explicitly in each section.

## 1. Idle detection via `ext-idle-notify-v1` (Wayland) / logind D-Bus (both)

**Current state** (`src-tauri/src/platform/linux.rs`, `platform_idle_seconds`): shells out to `loginctl show-session --property=IdleHint --property=IdleSinceHintMonotonic` once a second (via the cached `loginctl_cached` helper in the same file). Works, but is a subprocess spawn every tick and requires `XDG_SESSION_ID`.

**Target:** replace the subprocess with a D-Bus session bus connection via the [`zbus`](https://crates.io/crates/zbus) crate (pure Rust, no libdbus dependency, so it doesn't pull in the GTK-family crates that block cross-compilation here).

```toml
[target.'cfg(target_os = "linux")'.dependencies]
zbus = { version = "5", default-features = false, features = ["tokio"] }
```

### Design

- Connect once at startup to the session bus and get a proxy for `org.freedesktop.login1.Session` at the path `/org/freedesktop/login1/session/self` (or resolve via `org.freedesktop.login1.Manager.GetSessionByPID(getpid())` if `/session/self` isn't present; this varies by distro, so check both).
- Read `IdleHint` (bool) and `IdleSinceHint` (`u64`, microseconds since epoch, not monotonic) via the standard `org.freedesktop.DBus.Properties.Get` interface. This differs from the `IdleSinceHintMonotonic` property the current `loginctl` output parses; confirm which property the D-Bus interface actually exposes against `busctl --user introspect ...` or `qdbus` output before writing the parser.
- Subscribe to `PropertiesChanged` signals on that interface for `IdleHint` instead of polling. This is the actual improvement over the current subprocess-every-second approach: idle transitions become event-driven.
- Keep the existing parsing function shape (`linux_idle_seconds_from(properties: &str, uptime_seconds: f64) -> Option<u32>`), but adapt its inputs to whatever zbus's typed property getters return (likely `(bool, u64)` tuples rather than a raw properties-text blob). Keep it a pure function taking primitives so it stays unit-testable independent of the D-Bus transport.

### Wayland idle fallback: `ext-idle-notify-v1`

For compositors that don't run logind (rare, but some minimal setups), or as the Wayland-native signal:

- Add `wayland-client` (v0.31+) as a dependency, `#[cfg(target_os = "linux")]`.
- Bind `wl_seat` and `ext_idle_notifier_v1` globals via the registry.
- Create an `ext_idle_notification_v1` with a fixed timeout (e.g. 5 minutes, matching the existing `report_idle` threshold in `pausio-core`), and listen for `idled`/`resumed` events.
- This requires running a small event loop on a dedicated thread (Wayland client connections are not `Send` across an arbitrary async runtime without care). Spawn a `std::thread` that owns the Wayland connection and idle notifier, and forward `idled`/`resumed` transitions to the main app via an `mpsc` channel or a `tauri::async_runtime::spawn` bridge.
- Compositor caveat to verify on real hardware: GNOME Mutter's Wayland compositor did not implement `ext-idle-notify-v1` for a long time (historically routing through `org.gnome.Mutter.IdleMonitor`, a GNOME-specific D-Bus interface). KDE and wlroots-based compositors (Sway, Hyprland) do implement the standard protocol. A full implementation would need three code paths: logind D-Bus (broadest coverage, works on both X11 and Wayland since it's session-manager-level rather than compositor-level), `ext-idle-notify-v1` (wlroots/KDE Wayland), and `org.gnome.Mutter.IdleMonitor` (GNOME Wayland). Since logind D-Bus already covers both display server types, ship the D-Bus path only and treat direct Wayland protocol binding as a stretch goal rather than a requirement; it adds two more failure-prone code paths for marginal gain over what logind already provides.

## 2. Lock/unlock via logind D-Bus signals

**Current state:** `platform_session_locked()` (`src-tauri/src/platform/linux.rs`) polls `loginctl show-session --property=LockedHint` once a second via the same `loginctl_cached` helper, and `sync_linux_session_lock` (same file) diffs that against `SessionLockState` on each poll to raise `Locked`/`Unlocked` events. This works, but is poll-driven rather than event-driven, unlike the `NSWorkspace` observers on macOS and `WTSRegisterSessionNotification` on Windows (`src-tauri/src/session_monitor.rs`).

**Target:** subscribe to the same `org.freedesktop.login1.Session` proxy's `Lock` and `Unlock` signals (not properties; logind emits these as distinct D-Bus signals, separate from the `LockedHint` property) via zbus, replacing the poll in `sync_linux_session_lock` with a signal handler.

- Emit `session_monitor::SessionEvent::Locked` / `Unlocked` (already defined, used by macOS/Windows) from the signal handler, reusing the existing `handle_session_event` dispatch.
- This removes the 1 Hz `loginctl` poll for lock state and is more correct: signal-driven detection can't miss a lock/unlock that falls between two 1-second polls. Unlikely to matter in practice, but consistent with the event-driven pattern used elsewhere in this codebase.

## 3. Overlay hardening on Wayland (`harden_break_overlay` is currently a no-op on Linux)

This is the highest-risk, highest-uncertainty item; do not attempt it without access to a real Sway/GNOME/KDE session to test against.

- **X11:** achievable today without new dependencies. Tauri's `always_on_top` mostly works on X11 ([tauri#3117](https://github.com/tauri-apps/tauri/issues/3117) is specifically about Wayland; X11 is unaffected). If the overlay still isn't reliably on top on some X11 window managers, fall back to calling `_NET_WM_STATE_ABOVE` directly via `x11rb`, already a transitive dependency of `tauri-plugin-global-shortcut` on Linux (check `cargo tree` to confirm it's already in the dependency graph).
- **wlroots/KDE Wayland (Sway, Hyprland, Plasma):** requires [`gtk-layer-shell`](https://github.com/wmww/gtk-layer-shell) bound into the GTK window Tauri already creates under the hood (Tauri's Linux backend is GTK+WebKitGTK). This means reaching into the raw `gtk::ApplicationWindow` that Tauri's `WebviewWindow` wraps (via `window.gtk_window()` if exposed; check `tauri::WebviewWindow`'s Linux-specific methods) and calling `gtk_layer_shell::init_for_window`, `set_layer(Layer::Overlay)`, `set_exclusive_zone(-1)`, and anchoring to all four edges to cover the whole output. The `gtk-layer-shell` crate provides safe Rust bindings; no raw FFI needed.
- **GNOME Mutter Wayland:** does not implement `wlr-layer-shell` (a wlroots-ecosystem protocol that GNOME deliberately doesn't support). There is no known way to force a window above all others on GNOME Wayland beyond the same limitations every other app has. Fall back to `always_on_top` (best-effort, may not survive focus changes) and report the gap in the health report (`overlay_hardening_supported: false` when the compositor is detected as GNOME Wayland, detectable via the `XDG_CURRENT_DESKTOP` and `XDG_SESSION_TYPE` env vars, which need no permission to read).
- **Compositor detection helper** (safe, testable, no FFI): a pure function `fn desktop_environment() -> (compositor: &str, session_type: &str)` reading `XDG_CURRENT_DESKTOP` / `XDG_SESSION_TYPE`, so the shell can choose X11 vs layer-shell vs "unsupported, report honestly" without any native calls. This is pure string parsing and needs no Linux machine to write and unit-test; implement it first.

## 4. Suggested implementation order

1. Compositor/session-type detection helper (pure function, testable anywhere, no Linux machine required).
2. logind D-Bus idle + lock/unlock via zbus (covers X11 and Wayland uniformly, replaces both remaining `loginctl` polls).
3. X11 `_NET_WM_STATE_ABOVE` overlay hardening fallback, if `always_on_top` proves insufficient in testing.
4. `gtk-layer-shell` overlay for wlroots/KDE.
5. GNOME Wayland: document as unsupported, surface in the health report, do not attempt a workaround.
6. New CI job: a headless `sway` (wlroots reference compositor) session to exercise steps 2 through 4 in CI rather than relying on manual testing alone.

## 5. Current Linux behavior (baseline, unchanged by this spec)

- `platform_idle_seconds` / `platform_session_locked` / `harden_break_overlay` (`src-tauri/src/platform/linux.rs`) work today via the `loginctl` subprocess/poll approach described above; this spec proposes replacing them, not fixing a regression.
- `platform_context_signal()` on Linux (`src-tauri/src/platform/linux.rs`) is a documented `None`-returning stub. No portable, permission-free fullscreen/Do-Not-Disturb signal exists across desktop environments (X11 vs Wayland, GNOME vs KDE vs wlroots), so this is reported honestly in the desktop health report rather than faked. macOS and Windows (`src-tauri/src/platform/macos.rs`, `src-tauri/src/platform/windows.rs`) implement the equivalent signal natively.
