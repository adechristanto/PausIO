use pausio_protocol::ContextReason;

#[cfg(target_os = "windows")]
pub(crate) fn platform_idle_seconds() -> Option<u32> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetLastInputInfo(&mut input) }.as_bool() {
        Some(unsafe { GetTickCount() }.wrapping_sub(input.dwTime) / 1_000)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn platform_context_signal() -> Option<ContextReason> {
    use windows::Win32::UI::Shell::SHQueryUserNotificationState;
    let state = unsafe { SHQueryUserNotificationState() }.ok()?;
    windows_context_reason_from(state.0)
}

/// One documented Windows API call reports fullscreen games, presentations,
/// and Focus Assist ("quiet time") in a single value. `QUNS_BUSY` is
/// deliberately left unmapped: it also covers an ordinary maximized window,
/// which is not a reliable enough signal to defer a break over.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_context_reason_from(state: i32) -> Option<ContextReason> {
    match state {
        3 | 4 => Some(ContextReason::Fullscreen), // QUNS_RUNNING_D3D_FULL_SCREEN, QUNS_PRESENTATION_MODE
        6 => Some(ContextReason::DoNotDisturb),   // QUNS_QUIET_TIME (Focus Assist)
        _ => None,
    }
}

/// `HWND_TOPMOST` alone is not sufficient: the taskbar is itself topmost, and z-order among
/// topmost windows is decided by activation order, so the taskbar can still win. Asking the
/// shell to retract it via `ITaskbarList2::MarkFullscreenWindow` is what actually covers it.
#[cfg(target_os = "windows")]
pub(crate) fn harden_break_overlay(window: &tauri::WebviewWindow<tauri::Wry>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    let handle = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(hwnd) = handle.hwnd() else {
            return;
        };
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        mark_fullscreen(hwnd, true);
    });
}

/// Releases the shell's fullscreen mark before teardown. Skipping this leaves the
/// taskbar retracted after the break has already ended, which reads as a frozen
/// desktop even though PausIO is fine.
///
/// The window handle is resolved here rather than inside the closure: the caller
/// destroys the window immediately after this returns, and `run_on_main_thread`
/// only *queues* the closure, so by the time it runs `window.hwnd()` can already
/// be gone — and then the mark would never be released. The raw `HWND` stays
/// valid for the shell call because `MarkFullscreenWindow(_, false)` only needs
/// the shell to forget the window, not the window to still exist.
#[cfg(target_os = "windows")]
pub(crate) fn soften_break_overlay(window: &tauri::WebviewWindow<tauri::Wry>) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    // `HWND` wraps a raw pointer and is therefore not `Send`; the handle itself
    // is just an opaque shell-visible token, so it crosses as an integer.
    let raw = hwnd.0 as isize;
    let _ = window.run_on_main_thread(move || {
        mark_fullscreen(
            windows::Win32::Foundation::HWND(raw as *mut std::ffi::c_void),
            false,
        )
    });
}

#[cfg(target_os = "windows")]
pub(crate) fn mark_fullscreen(hwnd: windows::Win32::Foundation::HWND, fullscreen: bool) {
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    use windows::Win32::UI::Shell::{ITaskbarList2, TaskbarList};

    // COM is already initialised on the event-loop thread, which is where this runs.
    unsafe {
        let Ok(taskbar) = CoCreateInstance::<_, ITaskbarList2>(&TaskbarList, None, CLSCTX_ALL)
        else {
            return;
        };
        if taskbar.HrInit().is_ok() {
            let _ = taskbar.MarkFullscreenWindow(hwnd, fullscreen);
        }
    }
}
