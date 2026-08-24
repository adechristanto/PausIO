# Linux/Wayland parity — implementation spec

**Status:** Not implemented. Written as an executable spec for the next session with a real Linux build environment (this session ran on macOS with no GTK/webkit2gtk cross-compilation sysroot available — `cargo check --target x86_64-unknown-linux-gnu` fails on `gobject-sys`/`gio-sys`/`pango-sys`/`cairo-sys-rs` build scripts needing `pkg-config` against target libraries that don't exist on this host). Shipping the FFI/D-Bus code below without any compile feedback loop was judged too risky — a wrong type parameter or session-bus call shape can crash the native process, which is worse than not having the feature. Everything here is designed to the point where implementation is mechanical, not exploratory.

## 1. Idle detection via `ext-idle-notify-v1` (Wayland) / logind D-Bus (both)

**Current state** (`src-tauri/src/lib.rs`, `platform_idle_seconds` under `#[cfg(target_os = "linux")]`): shells out to `loginctl show-session --property=IdleHint --property=IdleSinceHintMonotonic` once a second. Works, but is a subprocess spawn every tick and requires `XDG_SESSION_ID`.

**Target:** Replace the subprocess with a D-Bus session bus connection via the [`zbus`](https://crates.io/crates/zbus) crate (pure Rust, no libdbus dependency — safer to add than the GTK-family crates that block cross-compilation here).

```toml
[target.'cfg(target_os = "linux")'.dependencies]
zbus = { version = "5", default-features = false, features = ["tokio"] }
```

### Design

- Connect once at startup to the **session** bus, get a proxy for `org.freedesktop.login1.Session` at the path `/org/freedesktop/login1/session/self` (or resolve via `org.freedesktop.login1.Manager.GetSessionByPID(getpid())` if `/session/self` isn't present on the distro — this varies; check both).
- Read `IdleHint` (bool) and `IdleSinceHint` (`u64`, **microseconds since epoch**, not monotonic — note this differs from the `IdleSinceHintMonotonic` property the current `loginctl` output already parses; use whichever the D-Bus property actually exposes, cross-check against `busctl --user introspect ...` or `qdbus` output on a real machine before writing the parser) via the standard `org.freedesktop.DBus.Properties.Get` interface.
- Subscribe to `PropertiesChanged` signals on that interface for `IdleHint`, instead of polling — this is the actual improvement over the current subprocess-every-second approach: idle transitions become event-driven.
- Keep the exact same **parsing function shape** already established (`linux_idle_seconds_from(properties: &str, uptime_seconds: f64) -> Option<u32>`) but adapt its inputs to whatever zbus's typed property getters return (likely `(bool, u64)` tuples rather than a raw properties-text blob) — write it as a pure function taking primitives so it stays unit-testable exactly like the current one, independent of the D-Bus transport.

### Wayland idle fallback: `ext-idle-notify-v1`

For compositors that don't run logind (rare, but some minimal setups), or as the more "correct" Wayland-native signal:

- Add `wayland-client` (v0.31+) as a dependency, `#[cfg(target_os = "linux")]`.
- Bind `wl_seat` and `ext_idle_notifier_v1` globals via the registry.
- Create an `ext_idle_notification_v1` with a fixed timeout (e.g. 5 minutes, matching the existing `report_idle` threshold in `pausio-core`), and listen for `idled`/`resumed` events.
- This requires running a small event loop on a dedicated thread (Wayland client connections are not `Send` across an arbitrary async runtime without care) — spawn a `std::thread` that owns the Wayland connection and idle notifier, and forwards `idled`/`resumed` transitions to the main app via an `mpsc` channel or a `tauri::async_runtime::spawn` bridge.
- **Compositor caveat to verify on real hardware:** GNOME Mutter's Wayland compositor did not implement `ext-idle-notify-v1` for a long time (historically routing through `org.gnome.Mutter.IdleMonitor` instead, a GNOME-specific D-Bus interface). KDE and wlroots-based compositors (Sway, Hyprland) do implement the standard protocol. A production implementation likely needs **three code paths**: logind D-Bus (broadest coverage, works on both X11 and Wayland since it's session-manager-level, not compositor-level — probably sufficient on its own and simpler than adding Wayland protocol bindings at all), `ext-idle-notify-v1` (wlroots/KDE Wayland), and `org.gnome.Mutter.IdleMonitor` (GNOME Wayland). Given logind D-Bus already covers both display server types, **recommend shipping the D-Bus path only** and treating direct Wayland protocol binding as a stretch goal, not a requirement — it adds two more failure-prone codepaths for marginal gain over what logind already provides.

## 2. Lock/unlock via logind D-Bus signals

**Current state:** `platform_session_locked()` polls `loginctl show-session --property=LockedHint` once a second; `session_monitor::install` is an empty no-op on Linux (`session_monitor.rs`), unlike the real `NSWorkspace` observers on macOS and `WTSRegisterSessionNotification` on Windows.

**Target:** Subscribe to the same `org.freedesktop.login1.Session` proxy's `Lock` and `Unlock` **signals** (not properties — logind emits these as distinct D-Bus signals, separate from the `LockedHint` property) via zbus. This is what should populate `session_monitor::install` on Linux for the first time, matching the event-driven pattern already used on macOS/Windows instead of polling.

- `session_monitor::SessionEvent::Locked` / `Unlocked` (already defined, used by macOS/Windows) should be emitted from the signal handler, reusing the exact same `handle_session_event` dispatch already in `src-tauri/src/lib.rs`.
- This removes the 1 Hz `loginctl` poll for lock state entirely once wired, and is more correct: signal-driven lock detection can't miss a lock/unlock that happens to fall between two 1-second polls (unlikely to matter in practice, but the event-driven form is strictly more correct and is also the pattern this codebase already uses everywhere else).

## 3. Overlay hardening on Wayland (`harden_break_overlay` is currently a no-op on Linux)

This is the **highest-risk, highest-uncertainty** item — do not attempt without access to a real Sway/GNOME/KDE session to click-test against.

- **X11:** achievable today without new dependencies — Tauri's `always_on_top` mostly works on X11 (confirmed by existing `tao` issue research: [tauri#3117](https://github.com/tauri-apps/tauri/issues/3117) is specifically about Wayland; X11 is unaffected). If the overlay still isn't reliably on top on some X11 window managers, the fallback is calling `_NET_WM_STATE_ABOVE` directly via `x11rb` (already a transitive dependency of `tauri-plugin-global-shortcut` on Linux — check `cargo tree` after this session's global-shortcut addition, it may already be in the dependency graph and free to use).
- **wlroots/KDE Wayland (Sway, Hyprland, Plasma):** requires [`gtk-layer-shell`](https://github.com/wmww/gtk-layer-shell) bound into the GTK window Tauri already creates under the hood (Tauri's Linux backend is GTK+WebKitGTK). This means reaching into the _raw_ `gtk::ApplicationWindow` Tauri's `WebviewWindow` wraps (via `window.gtk_window()` if Tauri exposes it — check `tauri::WebviewWindow` Linux-specific methods) and calling `gtk_layer_shell::init_for_window`, `set_layer(Layer::Overlay)`, `set_exclusive_zone(-1)`, and anchoring to all four edges to cover the whole output. The `gtk-layer-shell` crate provides safe Rust bindings; no raw FFI needed.
- **GNOME Mutter Wayland:** does **not** implement `wlr-layer-shell` (it's a wlroots-ecosystem protocol; GNOME deliberately doesn't support it). On GNOME Wayland there is no known way to force a window above all others short of the same limitations every other app has — the honest fallback is: keep `always_on_top` (best-effort, may not survive focus changes) and report this specific gap in the health report (`overlay_hardening_supported: false` when compositor is detected as GNOME Wayland — detectable via `XDG_CURRENT_DESKTOP` and `XDG_SESSION_TYPE` env vars, no permission needed to read those).
- **Compositor detection helper** (safe, testable, no FFI): a pure function `fn desktop_environment() -> (compositor: &str, session_type: &str)` reading `XDG_CURRENT_DESKTOP` / `XDG_SESSION_TYPE`, so the shell can choose X11 vs layer-shell vs "unsupported, report honestly" without any native calls. Write and unit-test this function **first**, independent of everything else — it's pure string parsing and needs no Linux machine to verify.

## 4. Suggested implementation order for the next session

1. Compositor/session-type detection helper (pure function, testable anywhere, no Linux needed to write correctly — do this even before setting up a Linux dev box).
2. logind D-Bus idle + lock/unlock via zbus (covers X11 and Wayland uniformly, replaces both remaining `loginctl` polls, removes the biggest reliability complaint in the audit).
3. X11 `_NET_WM_STATE_ABOVE` overlay hardening fallback (if `always_on_top` proves insufficient in testing).
4. `gtk-layer-shell` overlay for wlroots/KDE.
5. GNOME Wayland: document as unsupported, surface in health report, do not attempt a workaround.
6. New CI job: headless `sway` (wlroots reference compositor) via `Xvfb`-equivalent for Wayland, to actually exercise 2–4 in CI rather than relying on manual testing alone.

## 5. What this session did instead

- Left `platform_idle_seconds`/`platform_session_locked`/`harden_break_overlay` on Linux exactly as they were (working, subprocess/poll-based, no regression).
- Added the cross-platform `auto_detect_fullscreen` / `auto_detect_do_not_disturb` Settings toggles and the `platform_context_signal()` abstraction with a real Windows implementation; Linux's `platform_context_signal()` is a documented `None`-returning stub, exactly like `harden_break_overlay`'s existing Linux no-op — consistent with how this codebase already represents "not yet supported on this platform" rather than a fake success.
