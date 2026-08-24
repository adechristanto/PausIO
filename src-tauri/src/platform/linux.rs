#[cfg(target_os = "linux")]
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use pausio_protocol::ContextReason;

/// Maps systemd-logind's session-level idle properties to an elapsed duration.
/// This intentionally consumes only a coarse desktop-session state; it never
/// asks which application is active or inspects any user content.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn linux_idle_seconds_from(properties: &str, uptime_seconds: f64) -> Option<u32> {
    let idle_hint = properties
        .lines()
        .find_map(|line| line.strip_prefix("IdleHint="))?
        .trim();
    if idle_hint != "yes" {
        return Some(0);
    }
    let idle_since_micros = properties
        .lines()
        .find_map(|line| line.strip_prefix("IdleSinceHintMonotonic="))?
        .trim()
        .parse::<u64>()
        .ok()?;
    let now_micros = (uptime_seconds.max(0.0) * 1_000_000.0).floor() as u64;
    Some((now_micros.saturating_sub(idle_since_micros) / 1_000_000).min(u32::MAX as u64) as u32)
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn linux_session_locked_from(properties: &str) -> Option<bool> {
    match properties
        .lines()
        .find_map(|line| line.strip_prefix("LockedHint="))?
        .trim()
    {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// A single `loginctl` sample, shared by both idle and lock polling so the
/// tick loop forks at most one process every [`LOGINCTL_POLL_INTERVAL`]
/// instead of two subprocesses every second.
#[cfg(target_os = "linux")]
pub(crate) struct LoginctlSample {
    pub idle_seconds_at_fetch: Option<u32>,
    pub locked: Option<bool>,
    pub fetched_at: Instant,
}

#[cfg(target_os = "linux")]
pub(crate) static LOGINCTL_CACHE: Mutex<Option<LoginctlSample>> = Mutex::new(None);

/// Trade-off: lock/unlock detection can lag by up to this long on Linux
/// (previously instantaneous, at the cost of two `loginctl` process spawns
/// every second). Acceptable given the 5-minute idle-pause threshold and
/// that locks typically last minutes, not seconds.
#[cfg(target_os = "linux")]
pub(crate) const LOGINCTL_POLL_INTERVAL: Duration = Duration::from_secs(10);

#[cfg(target_os = "linux")]
pub(crate) fn fetch_loginctl_sample() -> Option<LoginctlSample> {
    let session_id = std::env::var("XDG_SESSION_ID").ok()?;
    let properties = std::process::Command::new("loginctl")
        .args([
            "show-session",
            &session_id,
            "--property=IdleHint",
            "--property=IdleSinceHintMonotonic",
            "--property=LockedHint",
        ])
        .output()
        .ok()?;
    if !properties.status.success() {
        return None;
    }
    let text = String::from_utf8(properties.stdout).ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    Some(LoginctlSample {
        idle_seconds_at_fetch: linux_idle_seconds_from(&text, uptime),
        locked: linux_session_locked_from(&text),
        fetched_at: Instant::now(),
    })
}

/// Returns `(idle_seconds_at_last_fetch, locked, fetched_at)`, refreshing the
/// shared cache with one `loginctl` call when it is missing or stale.
#[cfg(target_os = "linux")]
pub(crate) fn loginctl_cached() -> Option<(Option<u32>, Option<bool>, Instant)> {
    let mut cache = LOGINCTL_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stale = cache
        .as_ref()
        .is_none_or(|sample| sample.fetched_at.elapsed() >= LOGINCTL_POLL_INTERVAL);
    if stale {
        *cache = fetch_loginctl_sample();
    }
    cache.as_ref().map(|sample| {
        (
            sample.idle_seconds_at_fetch,
            sample.locked,
            sample.fetched_at,
        )
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn platform_idle_seconds() -> Option<u32> {
    let (idle_at_fetch, _, fetched_at) = loginctl_cached()?;
    match idle_at_fetch? {
        0 => Some(0),
        idle => {
            Some((u64::from(idle) + fetched_at.elapsed().as_secs()).min(u32::MAX as u64) as u32)
        }
    }
}

/// No portable, permission-free signal exists across desktop environments
/// (X11 vs Wayland, GNOME vs KDE vs wlroots) for fullscreen or Do Not
/// Disturb state. Automatic context detection is unsupported on Linux for
/// now; reported honestly in the desktop health report.
#[cfg(target_os = "linux")]
pub(crate) fn platform_context_signal() -> Option<ContextReason> {
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn platform_session_locked() -> Option<bool> {
    loginctl_cached()?.1
}

#[cfg(target_os = "linux")]
pub(crate) fn sync_linux_session_lock(app: &tauri::AppHandle) {
    use tauri::Manager;

    let Some(locked) = platform_session_locked() else {
        return;
    };
    let Some(lock_state) = app.try_state::<crate::state::SessionLockState>() else {
        return;
    };
    let was_locked = lock_state.is_locked();
    if locked != was_locked {
        crate::handle_session_event(
            app,
            if locked {
                crate::session_monitor::SessionEvent::Locked
            } else {
                crate::session_monitor::SessionEvent::Unlocked
            },
        );
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn harden_break_overlay(_window: &tauri::WebviewWindow<tauri::Wry>) {}
#[cfg(target_os = "linux")]
pub(crate) fn soften_break_overlay(_window: &tauri::WebviewWindow<tauri::Wry>) {}
