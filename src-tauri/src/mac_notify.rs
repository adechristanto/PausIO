//! macOS notification delivery that can never stall the rest of PausIO.
//!
//! Every blocking call in the UserNotifications API can park its caller for an
//! unbounded time. `request_auth` waits for a person to answer a system
//! permission dialog — which, for a build macOS declines to register, is a
//! dialog that never appears. `get_notification_settings` and the send itself
//! wait on XPC to `usernotificationsd`. On top of that, the crate's blocking
//! send refuses outright with `MainThreadNotRunning` whenever the main run loop
//! is not asleep at that exact instant, which is routinely the case just after
//! PausIO has created or closed a window.
//!
//! None of that may touch the publisher thread. That thread is serial and owns
//! tick emission, tray updates, and break-overlay creation and teardown, so
//! parking it does not merely delay a notification: it stops breaks from ever
//! appearing on screen. So all UserNotifications work happens here, on a
//! dedicated thread that owns nothing, and callers only ever read a cached
//! answer to "will macOS actually draw a banner for us?".
//!
//! Correctness never depends on that answer. The due-break surfaces a person
//! acts on are PausIO's own windows (see `break_windows`), and the engine
//! either waits for that answer or starts the break on its own once the grace
//! period is up (see `TimerEngine::due_grace_seconds`), so a notification is
//! an invitation and the cached capability only decides whether PausIO also
//! needs to raise one of its own surfaces.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use pausio_core::SystemSound;

/// What macOS will do with a notification from PausIO right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    /// Not yet determined — the first refresh has not completed.
    Unknown,
    /// Authorized, with banners enabled: a notification will be seen.
    Available,
    /// Authorized, but the alert style is "None". macOS accepts the
    /// notification and files it in Notification Center without ever drawing
    /// it, so this is indistinguishable from silence for our purposes.
    AlertsOff,
    /// The person declined notifications for PausIO.
    Denied,
    /// macOS will not register this app for notifications at all. The usual
    /// cause is a build with no bundle identifier (`tauri dev`) or one signed
    /// ad-hoc with no Team ID, which never appears under
    /// System Settings › Notifications and never produces a dialog.
    Unavailable,
}

impl Capability {
    /// Whether a notification posted now would put something on screen. This is
    /// the only question callers should ask: everything else is diagnostics.
    pub(crate) fn will_be_seen(self) -> bool {
        self == Capability::Available
    }

    /// The stable string the desktop health report exposes to the UI. Each
    /// value has a different remedy, so they are deliberately not collapsed.
    pub(crate) fn as_health_state(self) -> &'static str {
        match self {
            Capability::Unknown => "not_determined",
            Capability::Available => "granted",
            Capability::AlertsOff => "alerts_off",
            Capability::Denied => "denied",
            Capability::Unavailable => "unavailable",
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Capability::Available,
            2 => Capability::AlertsOff,
            3 => Capability::Denied,
            4 => Capability::Unavailable,
            _ => Capability::Unknown,
        }
    }

    fn to_code(self) -> u8 {
        match self {
            Capability::Unknown => 0,
            Capability::Available => 1,
            Capability::AlertsOff => 2,
            Capability::Denied => 3,
            Capability::Unavailable => 4,
        }
    }
}

static CAPABILITY: AtomicU8 = AtomicU8::new(0);

/// Reads the cached capability. Never blocks, never touches macOS.
pub(crate) fn capability() -> Capability {
    Capability::from_code(CAPABILITY.load(Ordering::Relaxed))
}

/// Work handed to the notify thread. Everything here is allowed to be slow.
enum Job {
    Refresh,
    Post { title: String, body: String },
}

static QUEUE: OnceLock<std::sync::mpsc::Sender<Job>> = OnceLock::new();

fn queue() -> Option<&'static std::sync::mpsc::Sender<Job>> {
    QUEUE.get()
}

/// Starts the notify thread and kicks off the first capability probe, including
/// the one-time authorization request. Called from `setup`, so the request
/// happens while a person is plausibly still at the keyboard rather than in the
/// middle of the first break, and so a dialog nobody answers parks only this
/// thread.
pub(crate) fn install() {
    let (sender, receiver) = std::sync::mpsc::channel::<Job>();
    if QUEUE.set(sender).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("pausio-macos-notify".into())
        .spawn(move || {
            for job in receiver {
                // A panic inside the UserNotifications crate (it has several
                // `expect`s on poisoned global mutexes) must not take this
                // thread down: losing it would silently end all notifications
                // for the rest of the process lifetime.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(job)));
            }
        });
    refresh();
}

/// Re-probes authorization and banner settings in the background.
pub(crate) fn refresh() {
    if let Some(sender) = queue() {
        let _ = sender.send(Job::Refresh);
    }
}

/// Queues a plain notification and returns immediately.
///
/// The return value is the *cached* capability, not the outcome of this post:
/// there is no non-blocking way to learn the latter, and waiting for it would
/// block the tick loop on the timer's own mutex. Callers use it only to decide
/// whether PausIO needs to show one of its own surfaces as well.
pub(crate) fn post(title: &str, body: &str) -> Capability {
    let state = capability();
    // Post even when the cache says it will not be seen: a Notification Center
    // entry is still a record, and the cache may be stale in the person's
    // favour. What matters is that the caller is told not to rely on it.
    if let Some(sender) = queue() {
        let _ = sender.send(Job::Post {
            title: title.into(),
            body: body.into(),
        });
    }
    state
}

fn run(job: Job) {
    match job {
        Job::Refresh => {
            store(probe());
        }
        Job::Post { title, body } => {
            if !ensure_authorized() {
                return;
            }
            let _ = mac_usernotifications::Notification::new()
                .title(&title)
                .message(&body)
                .send_blocking();
        }
    }
}

/// Brings authorization to a usable state, refreshing the cache as a side
/// effect. Only ever called on the notify thread, where blocking is safe.
fn ensure_authorized() -> bool {
    let state = capability();
    if state == Capability::Unknown {
        let probed = probe();
        store(probed);
        return probed != Capability::Unavailable && probed != Capability::Denied;
    }
    state != Capability::Unavailable && state != Capability::Denied
}

fn probe() -> Capability {
    use mac_usernotifications::AuthorizationStatus;

    let Ok(settings) = mac_usernotifications::blocking::get_notification_settings() else {
        // `check_bundle` failing is the common case here: a binary with no
        // bundle identifier can never receive notifications.
        return Capability::Unavailable;
    };
    let settings = match settings.authorization_status {
        AuthorizationStatus::Authorized
        | AuthorizationStatus::Provisional
        | AuthorizationStatus::Ephemeral => settings,
        AuthorizationStatus::Denied => return Capability::Denied,
        AuthorizationStatus::NotDetermined | AuthorizationStatus::Unknown => {
            // Blocks until the person answers, or forever if macOS never asks.
            // Safe here and nowhere else.
            let granted = mac_usernotifications::blocking::request_auth().unwrap_or(false);
            let Ok(settings) = mac_usernotifications::blocking::get_notification_settings() else {
                return Capability::Unavailable;
            };
            match settings.authorization_status {
                AuthorizationStatus::Denied => return Capability::Denied,
                // A request that neither grants nor moves the status off
                // `NotDetermined` means macOS is not going to register this
                // app, however many times it is asked.
                AuthorizationStatus::NotDetermined | AuthorizationStatus::Unknown if !granted => {
                    return Capability::Unavailable;
                }
                _ => settings,
            }
        }
    };
    // Only `alert_enabled` decides whether anything is drawn. With the alert
    // style set to "None", `notification_center_enabled` stays Enabled and the
    // notification is silently filed away — treating that as delivered is what
    // turned a due break into a sound with no visible cue.
    if settings.alert_enabled == mac_usernotifications::NotificationSettingStatus::Enabled {
        Capability::Available
    } else {
        Capability::AlertsOff
    }
}

fn store(state: Capability) {
    CAPABILITY.store(state.to_code(), Ordering::Relaxed);
}

/// PausIO's own sound setting is independent of the system's per-app
/// notification sound, and the native path deliberately posts silent
/// notifications so the cue happens exactly once.
pub(crate) fn play_cue(sound: Option<SystemSound>) {
    if let Some(sound) = sound {
        let _ = crate::sound_player::play_system_sound(sound);
    }
}
