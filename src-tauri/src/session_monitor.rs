use tauri::AppHandle;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SessionEvent {
    Locked,
    Unlocked,
}

/// Two distinct macOS signals feed the same handler, because they describe two
/// different absences and neither implies the other.
///
/// `NSWorkspaceSessionDidResignActive` fires when the *user session* switches
/// out — fast user switching. Apple's own documentation describes it that way,
/// and it does **not** fire when the screen locks with the same user still
/// logged in. Observing only that notification is why the screen-lock rewind
/// never ran on macOS: `handle_session_event(Locked)` was simply never reached
/// for an ordinary Ctrl+Cmd+Q.
///
/// `com.apple.screenIsLocked` is the actual screen-lock signal, delivered on the
/// *distributed* notification center rather than the workspace one. It is not
/// part of Apple's published API surface, but it has been stable since OS X 10.9
/// and needs no entitlement — PausIO is not sandboxed (`entitlements.plist`
/// grants nothing; the hardened runtime alone does not block distributed
/// notifications). If Apple ever stops posting it, the failure is silent and
/// degrades to the previous behaviour: the countdown just does not rewind.
///
/// Both are kept. Overlapping delivery is already safe: `begin_lock` returns
/// `false` when a lock is in flight and `finish_unlock` returns `None` when one
/// is not, so a duplicate edge is dropped rather than double-counted (see
/// `SessionLockState`, and the regression test beside it).
#[cfg(target_os = "macos")]
pub(crate) fn install(app: AppHandle) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceSessionDidBecomeActiveNotification,
        NSWorkspaceSessionDidResignActiveNotification,
    };
    use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};

    // `addObserverForName_object_queue_usingBlock` with a `None` queue runs the
    // block synchronously on the posting thread. `handle_session_event` only
    // takes the engine mutex and enqueues on the publisher channel — it never
    // waits on native UI dispatch — so this is safe from any thread, and is the
    // same contract the tick loop and tray callbacks already rely on.
    let callback_for = |event: SessionEvent| {
        let handle = app.clone();
        RcBlock::new(move |_notification: NonNull<NSNotification>| {
            crate::handle_session_event(&handle, event);
        })
    };

    // Both centers are observed for the lifetime of the process; PausIO installs
    // these exactly once during application setup, so the observers are
    // deliberately leaked rather than tracked for removal.
    let workspace_center = NSWorkspace::sharedWorkspace().notificationCenter();
    for (name, event) in [
        (
            unsafe { NSWorkspaceSessionDidResignActiveNotification },
            SessionEvent::Locked,
        ),
        (
            unsafe { NSWorkspaceSessionDidBecomeActiveNotification },
            SessionEvent::Unlocked,
        ),
    ] {
        let observer = unsafe {
            workspace_center.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &callback_for(event),
            )
        };
        std::mem::forget(observer);
    }

    let distributed_center = NSDistributedNotificationCenter::defaultCenter();
    for (name, event) in [
        ("com.apple.screenIsLocked", SessionEvent::Locked),
        ("com.apple.screenIsUnlocked", SessionEvent::Unlocked),
    ] {
        let observer = unsafe {
            distributed_center.addObserverForName_object_queue_usingBlock(
                Some(&NSString::from_str(name)),
                None,
                None,
                &callback_for(event),
            )
        };
        std::mem::forget(observer);
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn install(app: AppHandle) {
    use std::sync::OnceLock;
    use windows::{
        Win32::{
            Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
            System::{
                LibraryLoader::GetModuleHandleW,
                RemoteDesktop::{
                    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
                    WTSUnRegisterSessionNotification,
                },
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_MESSAGE, MSG,
                RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY,
                WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
            },
        },
        core::w,
    };

    static APP: OnceLock<AppHandle> = OnceLock::new();

    unsafe extern "system" fn session_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_WTSSESSION_CHANGE {
            if let Some(app) = APP.get() {
                match wparam.0 as u32 {
                    WTS_SESSION_LOCK => crate::handle_session_event(app, SessionEvent::Locked),
                    WTS_SESSION_UNLOCK => crate::handle_session_event(app, SessionEvent::Unlocked),
                    _ => {}
                }
            }
            return LRESULT(0);
        }
        if message == WM_DESTROY {
            let _ = unsafe { WTSUnRegisterSessionNotification(hwnd) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    let _ = APP.set(app);
    std::thread::spawn(|| unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let class_name = w!("PausIOSessionMonitor");
        let class = WNDCLASSW {
            hInstance: HINSTANCE(module.0),
            lpszClassName: class_name,
            lpfnWndProc: Some(session_window_proc),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        let Ok(window) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!(""),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(HINSTANCE(module.0)),
            None,
        ) else {
            return;
        };
        if WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION).is_err() {
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn install(_app: AppHandle) {}
