//! Desktop notification system for task completion and failures.
//!
//! Responsibilities:
//! - Send cross-platform desktop notifications via notify-rust.
//! - Play optional sound alerts using platform-specific mechanisms.
//! - Provide graceful degradation when notification systems are unavailable.
//! - Support different notification types: task success, task failure, loop completion.
//!
//! Does NOT handle:
//! - Notification scheduling or queuing (callers trigger explicitly).
//! - Persistent notification history or logging.
//! - UI mode detection (callers should suppress if desired).
//! - Do Not Disturb detection (handled at call site if needed).
//!
//! Invariants:
//! - Sound playback failures don't fail the notification.
//! - Notification failures are logged but don't fail the calling operation.
//! - All platform-specific code is isolated per target OS.

use std::path::Path;

/// CLI overrides for notification settings.
/// Fields are `Option<bool>` to distinguish "not set" from explicit false.
#[derive(Debug, Clone, Default)]
pub struct NotificationOverrides {
    /// Override notify_on_complete from CLI.
    pub notify_on_complete: Option<bool>,
    /// Override notify_on_fail from CLI.
    pub notify_on_fail: Option<bool>,
    /// Override sound_enabled from CLI.
    pub notify_sound: Option<bool>,
}

/// Build a runtime NotificationConfig from config and CLI overrides.
///
/// Precedence: CLI overrides > config values > defaults.
///
/// # Arguments
/// * `config` - The notification config from resolved configuration
/// * `overrides` - CLI overrides for notification settings
///
/// # Returns
/// A fully-resolved NotificationConfig ready for use at runtime.
pub fn build_notification_config(
    config: &crate::contracts::NotificationConfig,
    overrides: &NotificationOverrides,
) -> NotificationConfig {
    let notify_on_complete = overrides
        .notify_on_complete
        .or(config.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(config.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = config.notify_on_loop_complete.unwrap_or(true);
    // enabled acts as a global on/off switch - true if ANY notification type is enabled
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    NotificationConfig {
        enabled,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete,
        suppress_when_active: config.suppress_when_active.unwrap_or(true),
        sound_enabled: overrides
            .notify_sound
            .or(config.sound_enabled)
            .unwrap_or(false),
        sound_path: config.sound_path.clone(),
        timeout_ms: config.timeout_ms.unwrap_or(8000),
    }
}

/// Configuration for desktop notifications.
#[derive(Debug, Clone, Default)]
pub struct NotificationConfig {
    /// Enable desktop notifications on task completion (legacy field).
    pub enabled: bool,
    /// Enable desktop notifications on task completion.
    pub notify_on_complete: bool,
    /// Enable desktop notifications on task failure.
    pub notify_on_fail: bool,
    /// Enable desktop notifications when loop mode completes.
    pub notify_on_loop_complete: bool,
    /// Suppress notifications when a foreground UI client is active.
    pub suppress_when_active: bool,
    /// Enable sound alerts with notifications.
    pub sound_enabled: bool,
    /// Custom sound file path (platform-specific format).
    /// If not set, uses platform default sounds.
    pub sound_path: Option<String>,
    /// Notification timeout in milliseconds (default: 8000).
    pub timeout_ms: u32,
}

impl NotificationConfig {
    /// Create a new config with sensible defaults.
    pub fn new() -> Self {
        Self {
            enabled: true,
            notify_on_complete: true,
            notify_on_fail: true,
            notify_on_loop_complete: true,
            suppress_when_active: true,
            sound_enabled: false,
            sound_path: None,
            timeout_ms: 8000,
        }
    }

    /// Check if notifications should be suppressed based on UI state.
    pub fn should_suppress(&self, ui_active: bool) -> bool {
        if !self.enabled {
            return true;
        }
        if ui_active && self.suppress_when_active {
            return true;
        }
        false
    }
}

/// Types of notifications that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
    /// Task completed successfully.
    TaskComplete,
    /// Task failed.
    TaskFailed,
    /// Loop mode completed with summary.
    LoopComplete {
        tasks_total: usize,
        tasks_succeeded: usize,
        tasks_failed: usize,
    },
}

/// Send a notification based on the notification type.
/// Silently logs errors but never fails the calling operation.
///
/// # Arguments
/// * `notification_type` - The type of notification to send
/// * `task_id` - The task identifier (for task-specific notifications)
/// * `task_title` - The task title (for task-specific notifications)
/// * `config` - Notification configuration
/// * `ui_active` - Whether a foreground UI client is currently active (for suppression)
pub fn send_notification(
    notification_type: NotificationType,
    task_id: &str,
    task_title: &str,
    config: &NotificationConfig,
    ui_active: bool,
) {
    // Check if this notification type is enabled
    let type_enabled = match notification_type {
        NotificationType::TaskComplete => config.notify_on_complete,
        NotificationType::TaskFailed => config.notify_on_fail,
        NotificationType::LoopComplete { .. } => config.notify_on_loop_complete,
    };

    if !type_enabled {
        log::debug!(
            "Notification type {:?} disabled; skipping",
            notification_type
        );
        return;
    }

    if config.should_suppress(ui_active) {
        log::debug!("Notifications suppressed (UI active or globally disabled)");
        return;
    }

    // Build and show notification
    if let Err(e) =
        show_notification_typed(notification_type, task_id, task_title, config.timeout_ms)
    {
        log::debug!("Failed to show notification: {}", e);
    }

    // Play sound if enabled
    if config.sound_enabled
        && let Err(e) = play_completion_sound(config.sound_path.as_deref())
    {
        log::debug!("Failed to play sound: {}", e);
    }
}

/// Send task completion notification.
/// Silently logs errors but never fails the calling operation.
pub fn notify_task_complete(task_id: &str, task_title: &str, config: &NotificationConfig) {
    send_notification(
        NotificationType::TaskComplete,
        task_id,
        task_title,
        config,
        false,
    );
}

/// Send task completion notification with UI awareness.
/// Silently logs errors but never fails the calling operation.
pub fn notify_task_complete_with_context(
    task_id: &str,
    task_title: &str,
    config: &NotificationConfig,
    ui_active: bool,
) {
    send_notification(
        NotificationType::TaskComplete,
        task_id,
        task_title,
        config,
        ui_active,
    );
}

/// Send task failure notification.
/// Silently logs errors but never fails the calling operation.
pub fn notify_task_failed(
    task_id: &str,
    task_title: &str,
    error: &str,
    config: &NotificationConfig,
) {
    if !config.notify_on_fail {
        log::debug!("Failure notifications disabled; skipping");
        return;
    }

    if config.should_suppress(false) {
        log::debug!("Notifications suppressed (globally disabled)");
        return;
    }

    // Build and show notification
    if let Err(e) = show_notification_failure(task_id, task_title, error, config.timeout_ms) {
        log::debug!("Failed to show failure notification: {}", e);
    }

    // Play sound if enabled
    if config.sound_enabled
        && let Err(e) = play_completion_sound(config.sound_path.as_deref())
    {
        log::debug!("Failed to play sound: {}", e);
    }
}

/// Send loop completion notification.
/// Silently logs errors but never fails the calling operation.
pub fn notify_loop_complete(
    tasks_total: usize,
    tasks_succeeded: usize,
    tasks_failed: usize,
    config: &NotificationConfig,
) {
    if !config.notify_on_loop_complete {
        log::debug!("Loop completion notifications disabled; skipping");
        return;
    }

    if config.should_suppress(false) {
        log::debug!("Notifications suppressed (globally disabled)");
        return;
    }

    // Build and show notification
    if let Err(e) = show_notification_loop(
        tasks_total,
        tasks_succeeded,
        tasks_failed,
        config.timeout_ms,
    ) {
        log::debug!("Failed to show loop notification: {}", e);
    }

    // Play sound if enabled
    if config.sound_enabled
        && let Err(e) = play_completion_sound(config.sound_path.as_deref())
    {
        log::debug!("Failed to play sound: {}", e);
    }
}

/// Send watch mode notification for newly detected tasks.
/// Silently logs errors but never fails the calling operation.
pub fn notify_watch_new_task(count: usize, config: &NotificationConfig) {
    if !config.enabled {
        log::debug!("Notifications disabled; skipping");
        return;
    }

    if config.should_suppress(false) {
        log::debug!("Notifications suppressed (globally disabled)");
        return;
    }

    // Build and show notification
    if let Err(e) = show_notification_watch(count, config.timeout_ms) {
        log::debug!("Failed to show watch notification: {}", e);
    }

    // Play sound if enabled
    if config.sound_enabled
        && let Err(e) = play_completion_sound(config.sound_path.as_deref())
    {
        log::debug!("Failed to play sound: {}", e);
    }
}

#[cfg(feature = "notifications")]
fn show_notification_watch(count: usize, timeout_ms: u32) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout};

    let body = if count == 1 {
        "1 new task detected from code comments".to_string()
    } else {
        format!("{} new tasks detected from code comments", count)
    };

    Notification::new()
        .summary("Ralph: Watch Mode")
        .body(&body)
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()
        .map_err(|e| anyhow::anyhow!("Failed to show notification: {}", e))?;

    Ok(())
}

#[cfg(not(feature = "notifications"))]
fn show_notification_watch(_count: usize, _timeout_ms: u32) -> anyhow::Result<()> {
    log::debug!("Notifications feature not compiled in; skipping notification display");
    Ok(())
}

#[cfg(feature = "notifications")]
fn show_notification_typed(
    notification_type: NotificationType,
    task_id: &str,
    task_title: &str,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout};

    let (summary, body) = match notification_type {
        NotificationType::TaskComplete => (
            "Ralph: Task Complete",
            format!("{} - {}", task_id, task_title),
        ),
        NotificationType::TaskFailed => (
            "Ralph: Task Failed",
            format!("{} - {}", task_id, task_title),
        ),
        NotificationType::LoopComplete {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        } => (
            "Ralph: Loop Complete",
            format!(
                "{} tasks completed ({} succeeded, {} failed)",
                tasks_total, tasks_succeeded, tasks_failed
            ),
        ),
    };

    Notification::new()
        .summary(summary)
        .body(&body)
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()
        .map_err(|e| anyhow::anyhow!("Failed to show notification: {}", e))?;

    Ok(())
}

#[cfg(not(feature = "notifications"))]
fn show_notification_typed(
    _notification_type: NotificationType,
    _task_id: &str,
    _task_title: &str,
    _timeout_ms: u32,
) -> anyhow::Result<()> {
    log::debug!("Notifications feature not compiled in; skipping notification display");
    Ok(())
}

#[cfg(feature = "notifications")]
fn show_notification_failure(
    task_id: &str,
    task_title: &str,
    error: &str,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout};

    // Truncate error message to fit notification display
    let error_summary = if error.len() > 100 {
        format!("{}...", &error[..97])
    } else {
        error.to_string()
    };

    Notification::new()
        .summary("Ralph: Task Failed")
        .body(&format!(
            "{} - {}\nError: {}",
            task_id, task_title, error_summary
        ))
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()
        .map_err(|e| anyhow::anyhow!("Failed to show notification: {}", e))?;

    Ok(())
}

#[cfg(not(feature = "notifications"))]
fn show_notification_failure(
    _task_id: &str,
    _task_title: &str,
    _error: &str,
    _timeout_ms: u32,
) -> anyhow::Result<()> {
    log::debug!("Notifications feature not compiled in; skipping notification display");
    Ok(())
}

#[cfg(feature = "notifications")]
fn show_notification_loop(
    tasks_total: usize,
    tasks_succeeded: usize,
    tasks_failed: usize,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout};

    Notification::new()
        .summary("Ralph: Loop Complete")
        .body(&format!(
            "{} tasks completed ({} succeeded, {} failed)",
            tasks_total, tasks_succeeded, tasks_failed
        ))
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()
        .map_err(|e| anyhow::anyhow!("Failed to show notification: {}", e))?;

    Ok(())
}

#[cfg(not(feature = "notifications"))]
fn show_notification_loop(
    _tasks_total: usize,
    _tasks_succeeded: usize,
    _tasks_failed: usize,
    _timeout_ms: u32,
) -> anyhow::Result<()> {
    log::debug!("Notifications feature not compiled in; skipping notification display");
    Ok(())
}

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
        log::debug!("Sound playback not supported on this platform");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn play_macos_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    let sound_path = if let Some(path) = custom_path {
        path.to_string()
    } else {
        "/System/Library/Sounds/Glass.aiff".to_string()
    };

    if !Path::new(&sound_path).exists() {
        return Err(anyhow::anyhow!("Sound file not found: {}", sound_path));
    }

    let output = std::process::Command::new("afplay")
        .arg(&sound_path)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute afplay: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("afplay failed: {}", stderr));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn play_linux_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    if let Some(path) = custom_path {
        // Try paplay first (PulseAudio), fall back to aplay (ALSA)
        if Path::new(path).exists() {
            let result = std::process::Command::new("paplay").arg(path).output();
            if let Ok(output) = result {
                if output.status.success() {
                    return Ok(());
                }
            }

            // Fall back to aplay
            let output = std::process::Command::new("aplay")
                .arg(path)
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to execute aplay: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("aplay failed: {}", stderr));
            }
            return Ok(());
        } else {
            return Err(anyhow::anyhow!("Sound file not found: {}", path));
        }
    }

    // No custom path - try to play default notification sound via canberra-gtk-play
    let result = std::process::Command::new("canberra-gtk-play")
        .arg("--id=message")
        .output();

    if let Ok(output) = result {
        if output.status.success() {
            return Ok(());
        }
    }

    // If canberra-gtk-play fails or isn't available, that's okay - just log it
    log::debug!(
        "Could not play default notification sound (canberra-gtk-play not available or failed)"
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn play_windows_sound(custom_path: Option<&str>) -> anyhow::Result<()> {
    if let Some(path) = custom_path {
        let path_obj = Path::new(path);
        if !path_obj.exists() {
            return Err(anyhow::anyhow!("Sound file not found: {}", path));
        }

        // Try winmm PlaySound first for .wav files
        if path.ends_with(".wav") || path.ends_with(".WAV") {
            if let Ok(()) = play_sound_winmm(path) {
                return Ok(());
            }
        }

        // Fall back to PowerShell MediaPlayer for other formats or if winmm fails
        if let Ok(()) = play_sound_powershell(path) {
            return Ok(());
        }

        return Err(anyhow::anyhow!(
            "Failed to play sound with all available methods"
        ));
    }

    // No custom path - Windows toast notification handles default sound
    Ok(())
}

#[cfg(target_os = "windows")]
fn play_sound_winmm(path: &str) -> anyhow::Result<()> {
    use std::ffi::CString;
    use windows_sys::Win32::Media::Audio::{PlaySoundA, SND_FILENAME, SND_SYNC};

    let c_path = CString::new(path).map_err(|e| anyhow::anyhow!("Invalid path encoding: {}", e))?;

    // SAFETY: PlaySoundA is a Windows API that accepts a valid null-terminated C string
    // pointer (c_path.as_ptr()) and flags. The SND_FILENAME flag tells it to treat the
    // pointer as a file path. The pointer is valid for the duration of the call.
    let result = unsafe {
        PlaySoundA(
            c_path.as_ptr(),
            std::ptr::null_mut(),
            SND_FILENAME | SND_SYNC,
        )
    };

    if result == 0 {
        return Err(anyhow::anyhow!("PlaySoundA failed"));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn play_sound_powershell(path: &str) -> anyhow::Result<()> {
    let script = format!(
        "$player = New-Object System.Media.SoundPlayer '{}'; $player.PlaySync()",
        path.replace('\'', "''")
    );

    let output = std::process::Command::new("powershell.exe")
        .arg("-Command")
        .arg(&script)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute PowerShell: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "PowerShell sound playback failed: {}",
            stderr
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_config_default_values() {
        let config = NotificationConfig::new();
        assert!(config.enabled);
        assert!(config.notify_on_complete);
        assert!(config.notify_on_fail);
        assert!(config.notify_on_loop_complete);
        assert!(config.suppress_when_active);
        assert!(!config.sound_enabled);
        assert!(config.sound_path.is_none());
        assert_eq!(config.timeout_ms, 8000);
    }

    #[test]
    fn notify_task_complete_disabled_does_nothing() {
        let config = NotificationConfig {
            enabled: false,
            notify_on_complete: false,
            notify_on_fail: false,
            notify_on_loop_complete: false,
            suppress_when_active: true,
            sound_enabled: true,
            sound_path: None,
            timeout_ms: 8000,
        };
        // Should not panic or fail
        notify_task_complete("RQ-0001", "Test task", &config);
    }

    #[test]
    fn notification_config_can_be_customized() {
        let config = NotificationConfig {
            enabled: true,
            notify_on_complete: true,
            notify_on_fail: false,
            notify_on_loop_complete: true,
            suppress_when_active: false,
            sound_enabled: true,
            sound_path: Some("/path/to/sound.wav".to_string()),
            timeout_ms: 5000,
        };
        assert!(config.enabled);
        assert!(config.notify_on_complete);
        assert!(!config.notify_on_fail);
        assert!(config.notify_on_loop_complete);
        assert!(!config.suppress_when_active);
        assert!(config.sound_enabled);
        assert_eq!(config.sound_path, Some("/path/to/sound.wav".to_string()));
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn build_notification_config_uses_defaults() {
        let config = crate::contracts::NotificationConfig::default();
        let overrides = NotificationOverrides::default();
        let result = build_notification_config(&config, &overrides);

        assert!(result.enabled);
        assert!(result.notify_on_complete);
        assert!(result.notify_on_fail);
        assert!(result.notify_on_loop_complete);
        assert!(result.suppress_when_active);
        assert!(!result.sound_enabled);
        assert!(result.sound_path.is_none());
        assert_eq!(result.timeout_ms, 8000);
    }

    #[test]
    fn build_notification_config_overrides_take_precedence() {
        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(false),
            notify_on_fail: Some(false),
            sound_enabled: Some(false),
            ..Default::default()
        };
        let overrides = NotificationOverrides {
            notify_on_complete: Some(true),
            notify_on_fail: Some(true),
            notify_sound: Some(true),
        };
        let result = build_notification_config(&config, &overrides);

        assert!(result.notify_on_complete); // override wins
        assert!(result.notify_on_fail); // override wins
        assert!(result.sound_enabled); // override wins
    }

    #[test]
    fn build_notification_config_config_used_when_no_override() {
        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(false),
            notify_on_fail: Some(true),
            suppress_when_active: Some(false),
            timeout_ms: Some(5000),
            sound_path: Some("/path/to/sound.wav".to_string()),
            ..Default::default()
        };
        let overrides = NotificationOverrides::default();
        let result = build_notification_config(&config, &overrides);

        assert!(!result.notify_on_complete); // from config
        assert!(result.notify_on_fail); // from config
        assert!(!result.suppress_when_active); // from config
        assert_eq!(result.timeout_ms, 5000); // from config
        assert_eq!(result.sound_path, Some("/path/to/sound.wav".to_string()));
    }

    #[test]
    fn build_notification_config_enabled_computed_correctly() {
        // If all notification types are disabled, enabled should be false
        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(false),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            ..Default::default()
        };
        let overrides = NotificationOverrides::default();
        let result = build_notification_config(&config, &overrides);
        assert!(!result.enabled);

        // If any notification type is enabled, enabled should be true
        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(true),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            ..Default::default()
        };
        let result = build_notification_config(&config, &overrides);
        assert!(result.enabled);
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
            // Should succeed (no custom sound requested)
            let result = play_windows_sound(None);
            assert!(result.is_ok());
        }

        #[test]
        fn play_windows_sound_wav_file_exists() {
            // Create a minimal valid WAV file header
            let mut temp_file = NamedTempFile::with_suffix(".wav").unwrap();
            // RIFF WAV header (44 bytes minimum)
            let wav_header: Vec<u8> = vec![
                // RIFF chunk
                0x52, 0x49, 0x46, 0x46, // "RIFF"
                0x24, 0x00, 0x00, 0x00, // file size - 8
                0x57, 0x41, 0x56, 0x45, // "WAVE"
                // fmt chunk
                0x66, 0x6D, 0x74, 0x20, // "fmt "
                0x10, 0x00, 0x00, 0x00, // chunk size (16)
                0x01, 0x00, // audio format (PCM)
                0x01, 0x00, // num channels (1)
                0x44, 0xAC, 0x00, 0x00, // sample rate (44100)
                0x88, 0x58, 0x01, 0x00, // byte rate
                0x02, 0x00, // block align
                0x10, 0x00, // bits per sample (16)
                // data chunk
                0x64, 0x61, 0x74, 0x61, // "data"
                0x00, 0x00, 0x00, 0x00, // data size
            ];
            temp_file.write_all(&wav_header).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_str().unwrap();
            // Should not error on file existence check
            // Actual playback may fail in CI without audio subsystem
            if let Err(e) = play_windows_sound(Some(path)) {
                log::debug!("Sound playback failed in test (expected in CI): {}", e);
            }
        }

        #[test]
        fn play_windows_sound_non_wav_uses_powershell() {
            // Create a dummy mp3 file (just a header, not a real mp3)
            let mut temp_file = NamedTempFile::with_suffix(".mp3").unwrap();
            // MP3 sync word (not a full valid header, but enough for path validation)
            let mp3_header: Vec<u8> = vec![0xFF, 0xFB, 0x90, 0x00];
            temp_file.write_all(&mp3_header).unwrap();
            temp_file.flush().unwrap();

            let path = temp_file.path().to_str().unwrap();
            // Should attempt PowerShell fallback for non-WAV files
            // Result depends on whether PowerShell is available
            if let Err(e) = play_windows_sound(Some(path)) {
                log::debug!("Sound playback failed in test (expected in CI): {}", e);
            }
        }
    }
}
