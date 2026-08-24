//! Plays a system sound directly, independent of any OS notification popup.
//! Used for cues (like the short-break-end chime) that must be heard even
//! when a full-screen break overlay — not a notification — is on screen.
//! PausIO ships no bundled audio: every option below names a sound the
//! operating system already owns.

#[cfg(target_os = "macos")]
use pausio_core::SoundTheme;
use pausio_core::SystemSound;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakSoundMoment {
    /// No current call site: `events.rs` plays a cue only on `EngineEvent::Ended`,
    /// by deliberate design ("the pause must have actually finished, not merely
    /// started or been skipped early" -- see the comment at that call site).
    /// Kept, with its `macos_break_sound_name` mapping below, in case a start cue
    /// is ever added; not dead code to delete, just code with nothing calling it yet.
    #[allow(dead_code)]
    Start,
    End,
}

/// Resolves a [`SystemSound`] to the sound name understood by
/// `tauri-plugin-notification` on this platform, for attaching to an actual
/// notification popup. Sound enablement is handled by the caller before this
/// mapping is requested.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub fn notification_sound_name(sound: SystemSound) -> &'static str {
    #[cfg(target_os = "windows")]
    {
        // tauri-winrt-notification's toast Sound vocabulary (IM, Mail,
        // Reminder, SMS, Default, Alarm1-10, Call1-10) is closed and
        // unrelated to the PlaySoundW aliases used by `play_system_sound`.
        match sound {
            SystemSound::Default => "Default",
            SystemSound::Chime => "IM",
            SystemSound::Ding => "Reminder",
            SystemSound::Alert => "Alarm",
            SystemSound::Complete => "Mail",
        }
    }
    #[cfg(target_os = "linux")]
    {
        linux_theme_name(sound)
    }
}

#[cfg(target_os = "linux")]
fn linux_theme_name(sound: SystemSound) -> &'static str {
    // freedesktop sound-naming-spec names, widely present in stock sound
    // themes: http://0pointer.de/public/sound-naming-spec.html
    match sound {
        SystemSound::Default => "bell",
        SystemSound::Chime => "message-new-instant",
        SystemSound::Ding => "complete",
        SystemSound::Alert => "dialog-warning",
        SystemSound::Complete => "message",
    }
}

/// Plays `sound` immediately, fire-and-forget, with no notification popup.
/// Returns whether the operating system accepted the playback request, so the
/// settings preview can report a real failure instead of pretending it played.
pub fn play_system_sound(sound: SystemSound) -> bool {
    #[cfg(target_os = "macos")]
    return play_macos_named(macos_system_sound_name(sound), 1.0);
    #[cfg(target_os = "windows")]
    return play_windows(sound);
    #[cfg(target_os = "linux")]
    return play_linux(sound);
}

#[cfg(target_os = "macos")]
fn macos_system_sound_name(sound: SystemSound) -> &'static str {
    match sound {
        SystemSound::Default => "Tink",
        SystemSound::Chime => "Glass",
        SystemSound::Ding => "Ping",
        SystemSound::Alert => "Sosumi",
        SystemSound::Complete => "Hero",
    }
}

/// Plays the configured break cue natively on macOS. The webview can be
/// suspended while PausIO is tray-only, so Web Audio is not a reliable owner
/// of a process-wide reminder there.
#[cfg(target_os = "macos")]
pub fn play_break_sound(theme: SoundTheme, volume: u8, moment: BreakSoundMoment) -> bool {
    let Some(name) = macos_break_sound_name(theme, moment) else {
        return true;
    };
    play_macos_named(name, f32::from(volume.min(100)) / 100.0)
}

#[cfg(target_os = "macos")]
fn macos_break_sound_name(theme: SoundTheme, moment: BreakSoundMoment) -> Option<&'static str> {
    match (theme, moment) {
        (SoundTheme::Silence, _) => None,
        (SoundTheme::Chime, BreakSoundMoment::Start) => Some("Glass"),
        (SoundTheme::Chime, BreakSoundMoment::End) => Some("Hero"),
        (SoundTheme::Tone, BreakSoundMoment::Start) => Some("Ping"),
        (SoundTheme::Tone, BreakSoundMoment::End) => Some("Submarine"),
        (SoundTheme::Click, BreakSoundMoment::Start) => Some("Tink"),
        (SoundTheme::Click, BreakSoundMoment::End) => Some("Pop"),
    }
}

#[cfg(target_os = "macos")]
fn play_macos_named(name: &str, volume: f32) -> bool {
    use std::cell::RefCell;

    use objc2::rc::Retained;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSString;

    // NSSound playback is asynchronous. Keep the object retained on the
    // calling thread so dropping the local value cannot end a newly queued
    // cue. Both the publisher thread and command thread are long-lived.
    thread_local! {
        static ACTIVE_SOUND: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
    }

    let name = NSString::from_str(name);
    let Some(sound) = NSSound::soundNamed(&name) else {
        return false;
    };
    sound.setVolume(volume.clamp(0.0, 1.0));
    sound.setCurrentTime(0.0);
    ACTIVE_SOUND.with(|active| {
        if let Some(previous) = active.borrow_mut().take() {
            let _ = previous.stop();
        }
        let started = sound.play();
        if started {
            active.replace(Some(sound));
        }
        started
    })
}

#[cfg(target_os = "windows")]
fn play_windows(sound: SystemSound) -> bool {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
    use windows::core::PCWSTR;

    // Registry sound-scheme aliases under
    // HKCU\AppEvents\Schemes\Apps\.Default\<Alias>\.Current — a different
    // vocabulary from the toast-notification Sound enum used elsewhere.
    let alias = match sound {
        SystemSound::Default => "SystemDefault",
        SystemSound::Chime => "SystemAsterisk",
        SystemSound::Ding => "SystemNotification",
        SystemSound::Alert => "SystemExclamation",
        SystemSound::Complete => "SystemQuestion",
    };
    let mut wide: Vec<u16> = alias.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid, NUL-terminated UTF-16 buffer that outlives
    // this call; PlaySoundW with SND_ASYNC only needs it for the duration of
    // the call to queue playback.
    unsafe { PlaySoundW(PCWSTR(wide.as_mut_ptr()), None, SND_ALIAS | SND_ASYNC).as_bool() }
}

/// Plays the OS default system sound for a break that completed naturally.
/// Windows has no per-theme break cues (unlike macOS's per-theme `NSSound`
/// names) — every enabled `sound_theme` maps to this single OS default
/// alert. Enablement (`sound_theme != Silence`) is checked by the caller.
#[cfg(target_os = "windows")]
pub fn play_break_end_sound() -> bool {
    play_windows(SystemSound::Default)
}

#[cfg(target_os = "linux")]
fn play_linux(sound: SystemSound) -> bool {
    use std::process::{Command, Stdio};

    let theme_name = linux_theme_name(sound);
    let spawned = Command::new("canberra-gtk-play")
        .args(["-i", theme_name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if spawned.is_ok() {
        return true;
    }

    let path = format!("/usr/share/sounds/freedesktop/stereo/{theme_name}.oga");
    if std::path::Path::new(&path).exists() {
        return Command::new("paplay")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok();
    }
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn every_macos_break_theme_maps_to_an_installed_system_sound() {
        for theme in [SoundTheme::Chime, SoundTheme::Tone, SoundTheme::Click] {
            for moment in [BreakSoundMoment::Start, BreakSoundMoment::End] {
                let name = macos_break_sound_name(theme, moment).unwrap();
                let path = format!("/System/Library/Sounds/{name}.aiff");
                assert!(std::path::Path::new(&path).is_file(), "missing {path}");
            }
        }
        assert_eq!(
            macos_break_sound_name(SoundTheme::Silence, BreakSoundMoment::Start),
            None
        );
    }
}
