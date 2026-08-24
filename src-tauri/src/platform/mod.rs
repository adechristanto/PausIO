//! Platform idle and context detection.
//!
//! Three functions are implemented per supported platform and consumed by the tick loop in `lib.rs`:
//!   - `platform_idle_seconds()` — seconds since last HID input, or `None`.
//!   - `platform_context_signal()` — opt-in OS context for automatic deferral, or `None`.
//!
//! Linux additionally exposes `sync_linux_session_lock` / `platform_session_locked` because
//! lock events on Linux are polled rather than delivered as instantaneous OS callbacks.

#[cfg(any(target_os = "linux", test))]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(any(target_os = "windows", test))]
pub(crate) mod windows;
