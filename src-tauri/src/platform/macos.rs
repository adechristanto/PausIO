/// System-wide (HID) idle seconds via CoreGraphics: a public, permission-free API
/// that reads the idle counter in-process, with no subprocess spawn and no IPC.
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

/// Fullscreen detection via `CGWindowListCopyWindowInfo` — a public,
/// permission-free CoreGraphics API (same trust tier as the idle-time call
/// above): the frontmost normal-layer (0) window's bounds are compared
/// against every active display's bounds, and an exact match is treated as
/// fullscreen.
///
/// Focus/Do Not Disturb detection is deliberately *not* implemented: there is
/// no public, permission-free API for it on modern macOS, and the unofficial
/// `Assertions.json` route is known to false-positive. Reporting `None` for
/// that case honestly, rather than silently doing nothing, is why
/// `auto_context_dnd_supported` in the desktop health report is `false` on
/// macOS while `auto_context_fullscreen_supported` is `true`.
pub(crate) fn platform_context_signal() -> Option<pausio_protocol::ContextReason> {
    unsafe {
        let windows = cg_window_list::CGWindowListCopyWindowInfo(
            cg_window_list::K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY
                | cg_window_list::K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            cg_window_list::K_CG_NULL_WINDOW_ID,
        );
        if windows.is_null() {
            return None;
        }
        let result = cg_window_list::frontmost_window_is_fullscreen(windows);
        cg_window_list::CFRelease(windows);
        result.then_some(pausio_protocol::ContextReason::Fullscreen)
    }
}

/// Minimal, hand-written FFI surface for the one CoreGraphics/CoreFoundation
/// call chain `platform_context_signal` needs, rather than a new dependency
/// for a handful of stable C ABI calls.
#[allow(non_upper_case_globals, non_snake_case)]
mod cg_window_list {
    use std::ffi::c_void;

    type CFIndex = isize;
    type CFStringRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CGWindowID = u32;
    type CGDirectDisplayID = u32;
    type CGWindowListOption = u32;
    type CGError = i32;
    type Boolean = u8;

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    pub(super) const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: CGWindowListOption = 1 << 0;
    pub(super) const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: CGWindowListOption = 1 << 4;
    pub(super) const K_CG_NULL_WINDOW_ID: CGWindowID = 0;
    const K_CF_NUMBER_SINT64_TYPE: i32 = 4;
    // The frontmost, ordinary application window sits at CGWindowLevel 0;
    // menu bar, dock, and overlay windows use non-zero layers.
    const NORMAL_WINDOW_LAYER: i64 = 0;
    const MAX_DISPLAYS: u32 = 16;
    // CGWindowListCopyWindowInfo bounds are integer-rounded, so an exact
    // floating-point match is not reliable across every display config.
    const BOUNDS_EPSILON: f64 = 1.0;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        pub(super) fn CGWindowListCopyWindowInfo(
            option: CGWindowListOption,
            relative_to_window: CGWindowID,
        ) -> CFArrayRef;
        fn CGRectMakeWithDictionaryRepresentation(
            dict: CFDictionaryRef,
            rect: *mut CGRect,
        ) -> Boolean;
        fn CGDisplayBounds(display: CGDirectDisplayID) -> CGRect;
        fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut CGDirectDisplayID,
            display_count: *mut u32,
        ) -> CGError;
        static kCGWindowLayer: CFStringRef;
        static kCGWindowBounds: CFStringRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: CFIndex) -> *const c_void;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(
            number: *const c_void,
            the_type: i32,
            value_ptr: *mut c_void,
        ) -> Boolean;
        pub(super) fn CFRelease(cf: CFTypeRef);
    }

    fn window_layer(dict: CFDictionaryRef) -> Option<i64> {
        unsafe {
            let value = CFDictionaryGetValue(dict, kCGWindowLayer);
            if value.is_null() {
                return None;
            }
            let mut out: i64 = 0;
            (CFNumberGetValue(
                value,
                K_CF_NUMBER_SINT64_TYPE,
                &mut out as *mut i64 as *mut c_void,
            ) != 0)
                .then_some(out)
        }
    }

    fn window_bounds(dict: CFDictionaryRef) -> Option<CGRect> {
        unsafe {
            let value = CFDictionaryGetValue(dict, kCGWindowBounds);
            if value.is_null() {
                return None;
            }
            let mut rect = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize {
                    width: 0.0,
                    height: 0.0,
                },
            };
            (CGRectMakeWithDictionaryRepresentation(value, &mut rect) != 0).then_some(rect)
        }
    }

    fn active_display_bounds() -> Vec<CGRect> {
        unsafe {
            let mut ids = [0u32; MAX_DISPLAYS as usize];
            let mut count: u32 = 0;
            if CGGetActiveDisplayList(MAX_DISPLAYS, ids.as_mut_ptr(), &mut count) != 0 {
                return Vec::new();
            }
            ids[..count as usize]
                .iter()
                .map(|&id| CGDisplayBounds(id))
                .collect()
        }
    }

    fn rects_equal(a: &CGRect, b: &CGRect) -> bool {
        (a.origin.x - b.origin.x).abs() < BOUNDS_EPSILON
            && (a.origin.y - b.origin.y).abs() < BOUNDS_EPSILON
            && (a.size.width - b.size.width).abs() < BOUNDS_EPSILON
            && (a.size.height - b.size.height).abs() < BOUNDS_EPSILON
    }

    /// `windows` is ordered frontmost-first, so the first normal-layer entry
    /// is the frontmost application window; its bounds are compared against
    /// every active display.
    pub(super) fn frontmost_window_is_fullscreen(windows: CFArrayRef) -> bool {
        unsafe {
            let count = CFArrayGetCount(windows);
            let displays = active_display_bounds();
            for i in 0..count {
                let dict = CFArrayGetValueAtIndex(windows, i);
                if dict.is_null() || window_layer(dict) != Some(NORMAL_WINDOW_LAYER) {
                    continue;
                }
                return window_bounds(dict)
                    .is_some_and(|bounds| displays.iter().any(|d| rects_equal(d, &bounds)));
            }
            false
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{BOUNDS_EPSILON, CGPoint, CGRect, CGSize, rects_equal};

        fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
            CGRect {
                origin: CGPoint { x, y },
                size: CGSize { width, height },
            }
        }

        #[test]
        fn identical_rects_are_equal() {
            let a = rect(0.0, 0.0, 1920.0, 1080.0);
            let b = rect(0.0, 0.0, 1920.0, 1080.0);
            assert!(rects_equal(&a, &b));
        }

        #[test]
        fn rects_within_the_rounding_epsilon_are_equal() {
            // CGWindowListCopyWindowInfo bounds are integer-rounded, so a
            // window reported a fraction of a point off from the display's
            // exact bounds must still be treated as fullscreen.
            let display = rect(0.0, 0.0, 1920.0, 1080.0);
            let window = rect(0.4, -0.4, 1920.4, 1079.6);
            assert!(rects_equal(&display, &window));
        }

        #[test]
        fn rects_at_exactly_the_epsilon_boundary_are_not_equal() {
            // `rects_equal` uses a strict `<`, so a difference of exactly
            // BOUNDS_EPSILON must not be treated as a match.
            let display = rect(0.0, 0.0, 1920.0, 1080.0);
            let window = rect(BOUNDS_EPSILON, 0.0, 1920.0, 1080.0);
            assert!(!rects_equal(&display, &window));
        }

        #[test]
        fn a_maximized_but_non_fullscreen_window_is_not_equal_to_the_display() {
            // A window that merely fills most of the screen (e.g. maximized
            // with visible menu bar/dock) must not be mistaken for
            // fullscreen — this is the core false-positive this comparison
            // exists to avoid.
            let display = rect(0.0, 0.0, 1920.0, 1080.0);
            let maximized = rect(0.0, 25.0, 1920.0, 1030.0);
            assert!(!rects_equal(&display, &maximized));
        }

        #[test]
        fn a_smaller_secondary_display_is_not_confused_with_a_larger_one() {
            let primary = rect(0.0, 0.0, 2560.0, 1440.0);
            let secondary = rect(2560.0, 0.0, 1920.0, 1080.0);
            assert!(!rects_equal(&primary, &secondary));
        }
    }
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
