use tauri::{AppHandle, Manager};

use crate::state::{EngineState, lock_engine};

/// Tracks whether the main window is currently visible, so the
/// once-a-second `timer:tick` broadcast can skip it while it is tucked
/// away in the tray instead of waking a webview nobody is looking at
/// every second indefinitely.
pub(crate) static MAIN_WINDOW_VISIBLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Held so the tick loop can disable Quit for the duration of a break: `always_on_top`
/// does not stop Cmd+Q from terminating the process out from under a non-dismissible
/// overlay. The custom menu keeps the standard App/Edit/Window submenus (so Cmd+C/V/X/A
/// still work in Settings' text inputs) and only replaces the Quit item with one this
/// module can toggle.
pub(crate) fn set_quit_enabled(enabled: bool) {
    if let Some(items) = crate::tray_menu::TRAY_MENU_ITEMS.get() {
        let _ = items.quit.set_enabled(enabled);
    }
    #[cfg(target_os = "macos")]
    if let Some(item) = crate::platform::macos::QUIT_MENU_ITEM.get() {
        let _ = item.set_enabled(enabled);
    }
}

/// Pushes a fresh full snapshot to one window label. Called whenever a
/// window transitions from hidden to visible: while hidden it was excluded
/// from the once-a-second `timer:tick` broadcast (see `emit_tick`), so its
/// countdown can be stale by up to a second without this.
pub(crate) fn push_state_resync(app: &AppHandle, label: &str) {
    use tauri::Emitter;
    if let Some(engine) = app.try_state::<EngineState>() {
        let snapshot = lock_engine(&engine.0).snapshot();
        let _ = app.emit_to(label, "state:changed", snapshot);
    }
}

pub(crate) fn show_main_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = app.set_dock_visibility(true);
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    MAIN_WINDOW_VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
    push_state_resync(app, "main");
}

pub(crate) fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        #[cfg(target_os = "macos")]
        {
            if let Ok(pointer) = window.ns_window() {
                unsafe {
                    let native = &*pointer.cast::<objc2_app_kit::NSWindow>();
                    native.orderOut(None);
                }
            }
        }
    }
    MAIN_WINDOW_VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = app.set_dock_visibility(false);
        let _ = app.run_on_main_thread(|| {
            if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
                let ns_app = objc2_app_kit::NSApplication::sharedApplication(mtm);
                ns_app.hide(None);
            }
        });
    }
}

pub(crate) fn request_quit(app: &AppHandle) {
    let breaking = app.try_state::<EngineState>().is_some_and(|state| {
        matches!(
            lock_engine(&state.0).snapshot().phase,
            pausio_protocol::TimerPhase::Breaking { .. }
        )
    });
    if !breaking {
        app.exit(0);
    }
}

pub(crate) fn install_main_window_lifecycle(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide_main_window(&handle);
        }
    });
}
