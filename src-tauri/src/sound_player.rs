//! Plays a system sound directly, independent of any OS notification popup.
//! Used for cues (like the break-due banner and the break-end chime) that
//! must be heard even when a PausIO window — not a notification — is on
//! screen. PausIO ships no bundled audio: every option below names a sound
//! the operating system already owns.

use pausio_core::SystemSound;

/// Resolves a [`SystemSound`] to the sound name understood by
/// `tauri-plugin-notification` on this platform, for attaching to an actual
/// notification popup. Sound enablement is handled by the caller before this
/// mapping is requested.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub fn notification_sound_name(sound: SystemSound) -> &'static str {
    #[cfg(target_os = "windows")]
    {
        windows_toast_sound_name(sound)
    }
    #[cfg(target_os = "linux")]
    {
        linux_theme_name(sound)
    }
}

// tauri-winrt-notification's toast Sound vocabulary (IM, Mail, Reminder, SMS,
// Default, Alarm1-10, Call1-10) is closed and unrelated to the PlaySoundW
// aliases used by `play_system_sound`/`windows_playsound_alias`.
#[cfg(any(target_os = "windows", test))]
fn windows_toast_sound_name(sound: SystemSound) -> &'static str {
    match sound {
        SystemSound::Default => "Default",
        SystemSound::Chime => "IM",
        SystemSound::Ding => "Reminder",
        SystemSound::Alert => "Alarm",
        SystemSound::Complete => "Mail",
    }
}

#[cfg(any(target_os = "linux", test))]
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

#[cfg(any(target_os = "macos", test))]
fn macos_system_sound_name(sound: SystemSound) -> &'static str {
    match sound {
        SystemSound::Default => "Tink",
        SystemSound::Chime => "Glass",
        SystemSound::Ding => "Ping",
        SystemSound::Alert => "Sosumi",
        SystemSound::Complete => "Hero",
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

// Registry sound-scheme aliases under
// HKCU\AppEvents\Schemes\Apps\.Default\<Alias>\.Current — a different
// vocabulary from the toast-notification Sound enum used elsewhere.
#[cfg(any(target_os = "windows", test))]
fn windows_playsound_alias(sound: SystemSound) -> &'static str {
    match sound {
        SystemSound::Default => "SystemDefault",
        SystemSound::Chime => "SystemAsterisk",
        SystemSound::Ding => "SystemNotification",
        SystemSound::Alert => "SystemExclamation",
        SystemSound::Complete => "SystemQuestion",
    }
}

#[cfg(target_os = "windows")]
fn play_windows(sound: SystemSound) -> bool {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC};
    use windows::core::PCWSTR;

    let alias = windows_playsound_alias(sound);
    let mut wide: Vec<u16> = alias.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid, NUL-terminated UTF-16 buffer that outlives
    // this call; PlaySoundW with SND_ASYNC only needs it for the duration of
    // the call to queue playback.
    unsafe { PlaySoundW(PCWSTR(wide.as_mut_ptr()), None, SND_ALIAS | SND_ASYNC).as_bool() }
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

#[cfg(test)]
mod tests {
    use super::{
        linux_theme_name, macos_system_sound_name, windows_playsound_alias,
        windows_toast_sound_name,
    };
    use pausio_core::SystemSound;

    const ALL_SOUNDS: [SystemSound; 5] = [
        SystemSound::Default,
        SystemSound::Chime,
        SystemSound::Ding,
        SystemSound::Alert,
        SystemSound::Complete,
    ];

    /// Every mapping table must name a distinct sound per `SystemSound`
    /// variant. A collision would make two different settings choices play
    /// or notify identically, silently defeating the picker in Settings.
    fn assert_all_distinct(names: [&'static str; 5]) {
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(
                    names[i], names[j],
                    "sound names must be distinct per SystemSound variant, got duplicate {:?}",
                    names[i]
                );
            }
        }
    }

    #[test]
    fn macos_sound_names_are_distinct_per_variant() {
        assert_all_distinct(ALL_SOUNDS.map(macos_system_sound_name));
    }

    #[test]
    fn windows_playsound_aliases_are_distinct_per_variant() {
        assert_all_distinct(ALL_SOUNDS.map(windows_playsound_alias));
    }

    #[test]
    fn windows_toast_sound_names_are_distinct_per_variant() {
        assert_all_distinct(ALL_SOUNDS.map(windows_toast_sound_name));
    }

    #[test]
    fn linux_theme_names_are_distinct_per_variant() {
        assert_all_distinct(ALL_SOUNDS.map(linux_theme_name));
    }

    #[test]
    fn default_sound_maps_to_the_documented_names() {
        assert_eq!(macos_system_sound_name(SystemSound::Default), "Tink");
        assert_eq!(
            windows_playsound_alias(SystemSound::Default),
            "SystemDefault"
        );
        assert_eq!(windows_toast_sound_name(SystemSound::Default), "Default");
        assert_eq!(linux_theme_name(SystemSound::Default), "bell");
    }

    #[test]
    fn windows_playsound_aliases_and_toast_names_use_separate_vocabularies() {
        // These two Windows mappings are deliberately different closed
        // vocabularies (PlaySoundW registry aliases vs. toast Sound enum);
        // this guards against them accidentally being unified incorrectly.
        for sound in ALL_SOUNDS {
            assert_ne!(
                windows_playsound_alias(sound),
                windows_toast_sound_name(sound)
            );
        }
    }
}
