//! Notification sound playback.
//!
//! Responsibilities:
//! - Play optional notification sounds with platform-specific implementations.
//! - Route subprocess-backed playback through managed command helpers.
//!
//! Does NOT handle:
//! - Notification display or suppression.
//! - Sound-path configuration resolution.
//!
//! Invariants:
//! - Windows custom sounds remain `.wav`-only through WinMM.
//! - Unsupported platforms degrade to a debug log without failing callers.

use std::path::Path;

use crate::runutil::{ManagedCommand, TimeoutClass, execute_checked_command};

/// Play completion sound using platform-specific mechanisms.
pub fn play_completion_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        play_macos_sound(custom_path)
    }

    #[cfg(target_os = "linux")]
    {
        play_linux_sound(custom_path)
    }

    #[cfg(target_os = "windows")]
    {
        play_windows_sound(custom_path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = custom_path;
        log::debug!("Sound playback not supported on this platform");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn play_macos_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    let sound_path = custom_path.unwrap_or("/System/Library/Sounds/Glass.aiff");
    ensure_sound_exists(sound_path)?;
    run_media_command(
        "afplay",
        &[sound_path],
        "play notification sound with afplay",
    )
}

#[cfg(target_os = "linux")]
fn play_linux_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    if let Some(path) = custom_path {
        ensure_sound_exists(path)?;
        if run_media_command("paplay", &[path], "play notification sound with paplay").is_ok() {
            return Ok(());
        }
        return run_media_command("aplay", &[path], "play notification sound with aplay");
    }

    if run_media_command(
        "canberra-gtk-play",
        &["--id=message"],
        "play default notification sound",
    )
    .is_ok()
    {
        return Ok(());
    }

    log::debug!(
        "Could not play default notification sound (canberra-gtk-play not available or failed)"
    );
    Ok(())
}

fn run_media_command(program: &str, args: &[&str], description: &str) -> anyhow::Result<()> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    execute_checked_command(ManagedCommand::new(
        command,
        description,
        TimeoutClass::MediaPlayback,
    ))
    .map(|_| ())
}

fn ensure_sound_exists(path: &str) -> anyhow::Result<()> {
    if Path::new(path).exists() {
        return Ok(());
    }
    Err(anyhow::anyhow!("Sound file not found: {}", path))
}

#[cfg(target_os = "windows")]
fn play_windows_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    if let Some(path) = custom_path {
        ensure_sound_exists(path)?;

        if path.ends_with(".wav") || path.ends_with(".WAV") {
            if let Ok(()) = play_sound_winmm(path) {
                return Ok(());
            }
        }

        return Err(anyhow::anyhow!(
            "Windows custom notification sounds must be .wav files"
        ));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn play_sound_winmm(path: &str) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_SYNC};

    let wide_path = Path::new(path)
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();

    // SAFETY: PlaySoundW accepts a valid null-terminated UTF-16 file path pointer and flags.
    // The pointer remains valid for the duration of the synchronous call.
    let result = unsafe {
        PlaySoundW(
            wide_path.as_ptr(),
            std::ptr::null_mut(),
            SND_FILENAME | SND_SYNC,
        )
    };

    if result == 0 {
        return Err(anyhow::anyhow!("PlaySoundW failed"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn run_media_command_surfaces_process_failure() {
        let err = run_media_command(
            "/bin/sh",
            &["-c", "printf 'media failed' >&2; exit 4"],
            "play test media",
        )
        .expect_err("expected media failure");

        let text = err.to_string();
        assert!(text.contains("play test media failed"));
        assert!(text.contains("media failed"));
    }

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        #[test]
        fn play_windows_sound_missing_file() {
            let result = play_windows_sound(Some("/nonexistent/path/sound.wav"));
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not found"));
        }

        #[test]
        fn play_windows_sound_none_path() {
            let result = play_windows_sound(None);
            assert!(result.is_ok());
        }

        #[test]
        fn play_windows_sound_wav_file_exists() {
            let mut temp_file = NamedTempFile::with_suffix(".wav").unwrap();
            let wav_header: Vec<u8> = vec![
                0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6D,
                0x74, 0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x44, 0xAC, 0x00, 0x00,
                0x88, 0x58, 0x01, 0x00, 0x02, 0x00, 0x10, 0x00, 0x64, 0x61, 0x74, 0x61, 0x00, 0x00,
                0x00, 0x00,
            ];
            temp_file.write_all(&wav_header).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_str().unwrap();
            if let Err(error) = play_windows_sound(Some(path)) {
                log::debug!("Sound playback failed in test (expected in CI): {}", error);
            }
        }

        #[test]
        fn play_windows_sound_non_wav_is_rejected() {
            let mut temp_file = NamedTempFile::with_suffix(".mp3").unwrap();
            let mp3_header: Vec<u8> = vec![0xFF, 0xFB, 0x90, 0x00];
            temp_file.write_all(&mp3_header).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_str().unwrap();
            let err = play_windows_sound(Some(path)).unwrap_err();
            assert!(err.to_string().contains(".wav"));
        }
    }
}
