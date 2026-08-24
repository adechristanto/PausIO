use tauri::AppHandle;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SessionEvent {
    Locked,
    Unlocked,
}

#[cfg(target_os = "macos")]
pub(crate) fn install(app: AppHandle) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceSessionDidBecomeActiveNotification,
        NSWorkspaceSessionDidResignActiveNotification,
    };
    use objc2_foundation::NSNotification;

    let center = NSWorkspace::sharedWorkspace().notificationCenter();
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
        let handle = app.clone();
        let callback = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            crate::handle_session_event(&handle, event);
        });
        // NSWorkspace's notification center owns the observer for the lifetime of
        // the process. PausIO installs these exactly once during application setup.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &callback)
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
