use std::time::Duration;

use pausio_core::DisplayTarget;
use pausio_core::Locale;
use tauri::window::Color;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::main_window::{MAIN_WINDOW_VISIBLE, hide_main_window};

#[cfg(target_os = "linux")]
use crate::platform::linux::{harden_break_overlay, soften_break_overlay};
#[cfg(target_os = "macos")]
use crate::platform::macos::{harden_break_overlay, soften_break_overlay};
#[cfg(target_os = "windows")]
use crate::platform::windows::{harden_break_overlay, soften_break_overlay};
#[cfg(all(
    desktop,
    not(any(target_os = "macos", target_os = "linux", target_os = "windows"))
))]
fn harden_break_overlay(_window: &tauri::WebviewWindow<tauri::Wry>) {}
#[cfg(all(
    desktop,
    not(any(target_os = "macos", target_os = "linux", target_os = "windows"))
))]
fn soften_break_overlay(_window: &tauri::WebviewWindow<tauri::Wry>) {}

/// Bumped whenever overlays are torn down, so a watchdog armed for one break can tell
/// that it is stale (the break ended normally, or a newer break has already superseded
/// the one it was guarding).
pub(crate) static OVERLAY_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

const OVERLAY_WATCHDOG_GRACE_SECONDS: u64 = 10;
const PROMPT_EDGE_INSET_LOGICAL: f64 = 20.0;

pub(crate) fn overlay_watchdog_deadline(break_seconds: u32) -> Duration {
    Duration::from_secs(u64::from(break_seconds).saturating_add(OVERLAY_WATCHDOG_GRACE_SECONDS))
}

pub(crate) fn close_break_prompt(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("break-prompt") {
        let _ = window.close();
    }
    if !MAIN_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        hide_main_window(app);
    }
}

pub(crate) fn close_break_overlays(app: &AppHandle) {
    // Bumped *before* the teardown loop, not after. Destroying the focused
    // overlay makes the platform deliver `Focused(false)` to its own handler,
    // which re-hardens the window — on Windows that re-asserts
    // `MarkFullscreenWindow(hwnd, true)` for a window that is about to stop
    // existing, and the shell then keeps the taskbar retracted with nothing left
    // to release it. The handler compares against this counter, so raising it
    // first makes every surviving handler for this generation a no-op.
    OVERLAY_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    for (label, window) in app.webview_windows() {
        if label.starts_with("break-overlay-") {
            soften_break_overlay(&window);
            // `destroy`, not `close`: `close` fires CloseRequested, which the overlay's
            // own handler now vetoes (see show_break_overlays). Only this — engine-driven
            // — teardown path may take the shield down.
            let _ = window.destroy();
        }
    }
    if !MAIN_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        hide_main_window(app);
    }
}

pub(crate) fn close_break_windows(app: &AppHandle) {
    close_break_prompt(app);
    close_nudge_toasts(app);
    close_break_overlays(app);
    if !MAIN_WINDOW_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        hide_main_window(app);
    }
}

/// Last-resort release for the shield. It is deliberately not dismissible, which means a
/// stalled or dead tick loop would otherwise leave the machine unusable with no keyboard
/// escape. This fires `break_seconds + grace` after the break began and tears the shield
/// down regardless of engine state. It never touches the engine mutex: a wedged or
/// poisoned mutex is precisely the failure this exists to survive. It unblocks the screen
/// and nothing more — the engine itself is left in whatever state it was in.
pub(crate) fn spawn_overlay_watchdog(app: AppHandle, generation: u64, break_seconds: u32) {
    use std::sync::atomic::Ordering;

    let deadline = overlay_watchdog_deadline(break_seconds);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(deadline).await;
        if OVERLAY_GENERATION.load(Ordering::SeqCst) != generation {
            return; // The break ended normally, or a newer break superseded this one.
        }
        close_break_overlays(&app);
    });
}

/// Takes `locale` rather than reading it back through `current_locale`: the
/// caller already has the settings in hand from the `EngineView` it is emitting,
/// so re-reading it would mean taking the engine mutex again for a value that
/// was just read from it — and any lock taken on the publisher thread is a lock
/// the main event loop can end up waiting behind.
pub(crate) fn show_break_prompt(app: &AppHandle, locale: Locale) {
    if let Some(window) = app.get_webview_window("break-prompt") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Ok(window) = WebviewWindowBuilder::new(
        app,
        "break-prompt",
        WebviewUrl::App("?view=break-prompt".into()),
    )
    .title(crate::i18n::window_title_prompt(locale))
    .inner_size(460.0, 250.0)
    .min_inner_size(420.0, 230.0)
    .max_inner_size(500.0, 290.0)
    .background_color(Color(11, 13, 18, 0))
    .transparent(true)
    .decorations(false)
    .resizable(false)
    .closable(false)
    .minimizable(false)
    .maximizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .visible(false)
    .build() else {
        return;
    };
    position_prompt_on_main_monitor(app, &window, 460, 250);
    #[cfg(target_os = "macos")]
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.show();
    let _ = window.set_focus();
}

/// How long a nudge toast stays on screen. A nudge is advisory — it should read
/// like a notification banner, not like something waiting to be dealt with.
const NUDGE_TOAST_SECONDS: u64 = 7;
const NUDGE_TOAST_WIDTH: u32 = 360;
const NUDGE_TOAST_HEIGHT: u32 = 96;

/// PausIO's stand-in for a notification banner, used when macOS will not draw
/// one — an unregisterable build, a denied permission, or an alert style of
/// "None". Without it the blink, posture, and hydration reminders produced
/// nothing a sighted person could see: the only fallback was a screen-reader
/// announcement inside the main window, which is usually hidden in the tray.
///
/// Deliberately not focusable and not interactive. A gentle reminder that steals
/// keyboard focus from whatever someone is typing into is worse than no reminder
/// at all.
pub(crate) fn show_nudge_toast(app: &AppHandle, locale: Locale, nudge: &str) {
    // Each nudge replaces the last rather than stacking: three reminders can
    // come due in the same tick, and a column of toasts is an interruption.
    close_nudge_toasts(app);
    let label = format!("nudge-toast-{}", next_nudge_generation());
    // The locale travels in the URL because this window needs nothing else from
    // the backend. Left to fetch settings over IPC like the other views, the
    // toast would render its one line in English and swap to the real language a
    // frame later — visible, and avoidable, since the caller already has it.
    let lang = match locale {
        Locale::De => "de",
        Locale::En => "en",
    };
    let Ok(window) = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::App(format!("?view=nudge-toast&nudge={nudge}&locale={lang}").into()),
    )
    .title(crate::i18n::window_title_prompt(locale))
    .inner_size(f64::from(NUDGE_TOAST_WIDTH), f64::from(NUDGE_TOAST_HEIGHT))
    .background_color(Color(11, 13, 18, 0))
    .transparent(true)
    .decorations(false)
    .resizable(false)
    .closable(false)
    .minimizable(false)
    .maximizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .focusable(false)
    .shadow(false)
    .visible(false)
    .build() else {
        return;
    };
    position_prompt_on_main_monitor(app, &window, NUDGE_TOAST_WIDTH, NUDGE_TOAST_HEIGHT);
    #[cfg(target_os = "macos")]
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.show();

    let closing = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(NUDGE_TOAST_SECONDS)).await;
        let _ = closing.destroy();
    });
}

/// Labels are never reused, for the same reason break overlays stopped reusing
/// theirs: `destroy` only queues teardown, and a build that fails natively
/// leaves its label registered for the process lifetime — either would make
/// every later toast collide with a window that is gone or going.
fn next_nudge_generation() -> u64 {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Tears down any nudge toast still on screen. Called when a break takes over,
/// so an advisory reminder never sits on top of the break shield.
pub(crate) fn close_nudge_toasts(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with("nudge-toast") {
            let _ = window.destroy();
        }
    }
}

fn position_prompt_on_main_monitor(
    app: &AppHandle,
    window: &tauri::WebviewWindow<tauri::Wry>,
    width: u32,
    height: u32,
) {
    let monitor = app
        .get_webview_window("main")
        .and_then(|main| main.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        let _ = window.center();
        return;
    };
    let scale = monitor.scale_factor();
    let physical_width = (f64::from(width) * scale).round() as u32;
    let physical_height = (f64::from(height) * scale).round() as u32;
    let physical_inset = (PROMPT_EDGE_INSET_LOGICAL * scale).round() as u32;
    let work_area = monitor.work_area();
    let (x, y) = bottom_right_position(
        (work_area.position.x, work_area.position.y),
        (work_area.size.width, work_area.size.height),
        (physical_width, physical_height),
        physical_inset,
    );
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

pub(crate) fn bottom_right_position(
    work_area_position: (i32, i32),
    work_area_size: (u32, u32),
    window_size: (u32, u32),
    inset: u32,
) -> (i32, i32) {
    (
        work_area_position.0
            + work_area_size
                .0
                .saturating_sub(window_size.0)
                .saturating_sub(inset) as i32,
        work_area_position.1
            + work_area_size
                .1
                .saturating_sub(window_size.1)
                .saturating_sub(inset) as i32,
    )
}

/// `locale` is a parameter for the same reason as in `show_break_prompt`: the
/// only caller is `emit`, which is holding the engine mutex.
pub(crate) fn show_break_overlays(
    app: &AppHandle,
    locale: Locale,
    break_seconds: u32,
    target: DisplayTarget,
) {
    use std::sync::atomic::Ordering;
    #[cfg(target_os = "macos")]
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    #[cfg(target_os = "windows")]
    use tauri::window::{Effect, EffectsBuilder};

    close_break_prompt(app);
    // An advisory reminder must never be left floating over the break shield.
    close_nudge_toasts(app);
    close_break_overlays(app);
    // Read after the close above, which bumps the counter — this run's watchdog must not
    // be pre-empted by the teardown that just happened for the previous break.
    let generation = OVERLAY_GENERATION.load(Ordering::SeqCst);

    let Some(main) = app.get_webview_window("main") else {
        show_break_prompt(app, locale);
        return;
    };
    let Ok(mut monitors) = main.available_monitors() else {
        show_break_prompt(app, locale);
        return;
    };
    let current_position = main
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| *monitor.position());
    monitors.sort_by_key(|monitor| {
        if Some(*monitor.position()) == current_position {
            0
        } else {
            1
        }
    });

    match target {
        DisplayTarget::All => {}
        DisplayTarget::Active => {
            // "Active" means wherever the user's attention actually is, not the main
            // window's monitor — the window is very often hidden in the tray. Prefer the
            // monitor under the cursor; only fall back to the main window's monitor (already
            // sorted first, above) if the cursor position can't be resolved on this platform.
            let cursor_monitor = app.cursor_position().ok().and_then(|cursor| {
                let x = cursor.x as i32;
                let y = cursor.y as i32;
                monitors.iter().position(|monitor| {
                    let position = monitor.position();
                    let size = monitor.size();
                    x >= position.x
                        && x < position.x + size.width as i32
                        && y >= position.y
                        && y < position.y + size.height as i32
                })
            });
            match cursor_monitor {
                Some(index) => monitors = vec![monitors.swap_remove(index)],
                None => monitors.truncate(1),
            }
        }
        DisplayTarget::Primary => {
            let primary_position = main
                .primary_monitor()
                .ok()
                .flatten()
                .map(|monitor| *monitor.position());
            if let Some(primary_position) = primary_position {
                monitors.retain(|monitor| *monitor.position() == primary_position);
            } else {
                monitors.truncate(1);
            }
        }
        DisplayTarget::NotificationOnly => return,
    }

    let mut raised = 0usize;
    for (index, monitor) in monitors.into_iter().enumerate() {
        // The generation is part of the label, so a break never reuses one.
        //
        // Two failure modes made reuse unsafe, and both were silent. `destroy`
        // only *queues* teardown — the label is released when the runtime
        // delivers `Destroyed` on the main loop — so the close above has not
        // taken effect by the time this runs, and a same-label build would fail
        // with `WindowLabelAlreadyExists`. Worse, `build` returns `Ok` before
        // the native window exists, and a native creation failure leaves the
        // label registered for the rest of the process lifetime; from then on
        // every future break would collide with a window that does not exist and
        // raise no shield at all. A fresh label cannot collide with either.
        let label = format!("break-overlay-{generation}-{index}");
        let url = format!("?view=break-overlay&display={index}");
        // Position and size belong to the builder, not to post-build setters.
        // On macOS `set_position`/`set_size` hop through the main *dispatch
        // queue* while `show` runs inline on the main thread, so the setters
        // land a runloop iteration too late: the shield was ordered to the
        // screensaver level at tao's default ~800x600 and only then moved to
        // cover the monitor. Converting each monitor's own physical geometry
        // through its own scale factor also keeps a mixed-DPI setup correct,
        // which round-tripping physical values through the *window's* current
        // scale factor did not.
        let scale = monitor.scale_factor();
        let origin = monitor.position().to_logical::<f64>(scale);
        let extent = monitor.size().to_logical::<f64>(scale);
        let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
            .position(origin.x, origin.y)
            .inner_size(extent.width, extent.height)
            .title(crate::i18n::window_title_overlay(locale))
            .decorations(false)
            .resizable(false)
            .closable(false)
            .minimizable(false)
            .maximizable(false)
            .skip_taskbar(true)
            .shadow(false)
            .always_on_top(true)
            // Every shield is focusable now that none of them carry a control: if the
            // primary overlay fails to build, a secondary can still hold key status
            // instead of leaving it with the application behind the shield.
            .focusable(true)
            .background_color(Color(7, 11, 20, 255))
            .transparent(true)
            .visible(false);
        #[cfg(target_os = "macos")]
        let builder = builder.visible_on_all_workspaces(true).effects(
            EffectsBuilder::new()
                .effect(Effect::FullScreenUI)
                .state(EffectState::Active)
                .build(),
        );
        #[cfg(target_os = "windows")]
        let builder = builder.effects(
            EffectsBuilder::new()
                .effect(Effect::Acrylic)
                .color(Color(8, 11, 19, 210))
                .build(),
        );
        let window = match builder.build() {
            Ok(window) => window,
            Err(_) => continue,
        };
        // Before `show`, so the shield is never painted for a frame at the floating level.
        harden_break_overlay(&window);

        let guarded = window.clone();
        window.on_window_event(move |event| match event {
            // Alt+F4, Cmd+W and any other user-initiated close request. Engine-driven
            // teardown goes through `destroy`, which never reaches this handler.
            tauri::WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
            // Another application taking focus must not also take the z-order back.
            // Re-assert natively: calling Tauri's `set_always_on_top` here would drop the
            // macOS window level back to NSFloatingWindowLevel (3). Skipped once this
            // overlay's generation is over: teardown itself unfocuses the window, and
            // re-hardening a window that is being destroyed is what left the Windows
            // taskbar retracted after a break ended.
            tauri::WindowEvent::Focused(false) => {
                if OVERLAY_GENERATION.load(Ordering::SeqCst) == generation {
                    harden_break_overlay(&guarded);
                }
            }
            // Resolution or scale change mid-break: re-cover the monitor.
            tauri::WindowEvent::ScaleFactorChanged { .. } => {
                if OVERLAY_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                if let Ok(Some(monitor)) = guarded.current_monitor() {
                    // Logical, for the same mixed-DPI reason as the builder above.
                    let scale = monitor.scale_factor();
                    let _ = guarded.set_position(monitor.position().to_logical::<f64>(scale));
                    let _ = guarded.set_size(monitor.size().to_logical::<f64>(scale));
                }
                harden_break_overlay(&guarded);
            }
            _ => {}
        });

        let _ = window.show();
        if index == 0 {
            let _ = window.set_focus();
        }
        raised += 1;
    }

    if raised == 0 {
        // A break with no shield and no cue is a break that silently did not
        // happen. Every reason we get here is invisible from the engine's side
        // — no monitors reported, or the runtime refusing to build the windows
        // — so fall back to the prompt rather than letting the phase run out
        // behind whatever the person was already looking at.
        show_break_prompt(app, locale);
    }
    spawn_overlay_watchdog(app.clone(), generation, break_seconds);
}
