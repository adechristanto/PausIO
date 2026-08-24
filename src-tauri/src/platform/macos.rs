/// System-wide (HID) idle seconds via CoreGraphics — a public, permission-free
/// API. Previously this forked `ioreg -c IOHIDSystem` on every 1-second tick
/// (~86,400 process spawns/day just for an idle check); this reads the same
/// idle counter in-process with no subprocess and no IPC.
pub(crate) fn platform_idle_seconds() -> Option<u32> {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: u32) -> f64;
    }
    // state_id 1 = kCGEventSourceStateHIDSystemState (system-wide, matches the
    // old IOHIDSystem HIDIdleTime scope); event_type u32::MAX = kCGAnyInputEventType.
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(1, u32::MAX) };
    (seconds.is_finite() && seconds >= 0.0).then(|| seconds.min(u32::MAX as f64) as u32)
}

/// Reliable fullscreen/Focus detection on macOS needs either the
/// Accessibility permission or careful CoreGraphics window-list traversal
/// (`CGWindowListCopyWindowInfo`), and the unofficial `Assertions.json` route
/// for Focus state is known to false-positive. Both are deliberately
/// deferred rather than shipped unverified; automatic context detection is
/// unsupported on macOS for now, and this is reported honestly in the
/// desktop health report rather than silently doing nothing.
pub(crate) fn platform_context_signal() -> Option<pausio_protocol::ContextReason> {
    None
}

/// Raises a break overlay above the Dock and menu bar. Tauri's `always_on_top` maps to
/// NSFloatingWindowLevel (3), which sits below NSDockWindowLevel (20) and
/// NSMainMenuWindowLevel (24), so the shield would otherwise be painted over by both.
/// NSScreenSaverWindowLevel (1000) clears both while staying below the assistive-technology
/// level (1500), keeping VoiceOver panels reachable during a break.
///
/// AppKit is main-thread-only, and this is reached from the tick loop's tokio worker, so
/// the work is dispatched rather than called directly.
pub(crate) fn harden_break_overlay(window: &tauri::WebviewWindow<tauri::Wry>) {
    use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let handle = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(pointer) = handle.ns_window() else {
            return;
        };
        unsafe {
            let native = &*pointer.cast::<NSWindow>();
            native.setLevel(NSScreenSaverWindowLevel);
            // Assigned wholesale rather than merged: the builder's
            // `visible_on_all_workspaces(true)` only ORs in CanJoinAllSpaces, and any
            // leftover Managed/FullScreenPrimary bits would let the shield be shuffled
            // between Spaces. Stationary pins it through Space transitions;
            // FullScreenAuxiliary lets it draw over another app's fullscreen Space.
            native.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            // Must stay false: a shield that forwards clicks to the windows underneath is
            // not a shield.
            native.setIgnoresMouseEvents(false);
        }
    });
}

pub(crate) fn soften_break_overlay(_window: &tauri::WebviewWindow<tauri::Wry>) {}

/// Held so the tick loop can disable Quit for the duration of a break: `always_on_top`
/// does not stop Cmd+Q from terminating the process out from under a non-dismissible
/// overlay. The custom menu keeps the standard App/Edit/Window submenus (so Cmd+C/V/X/A
/// still work in Settings' text inputs) and only replaces the Quit item with one this
/// module can toggle.
pub(crate) static QUIT_MENU_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> =
    std::sync::OnceLock::new();

pub(crate) fn build_app_menu(
    app: &tauri::AppHandle,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, SubmenuBuilder};
    let locale = crate::tray_menu::current_locale(app);
    let quit = MenuItem::with_id(
        app,
        "quit-app",
        crate::i18n::tray_quit(locale),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let _ = QUIT_MENU_ITEM.set(quit.clone());

    let app_menu = SubmenuBuilder::new(app, "PausIO")
        .about(None)
        .separator()
        .item(&quit)
        .build()?;
    let edit_menu = SubmenuBuilder::new(app, crate::i18n::menu_edit(locale))
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let window_menu = SubmenuBuilder::new(app, crate::i18n::menu_window(locale))
        .minimize()
        .fullscreen()
        .build()?;

    Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])
}

pub(crate) fn configure_macos_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // The main app is a normal document-style desktop window. The previous
    // overlay hardening accidentally applied to it too, disabling macOS's
    // green traffic-light zoom/full-screen affordance while leaving resize on.
    // Keep the restrictive policy exclusively on prompt and overlay windows.
    let _ = window.set_maximizable(true);
}
