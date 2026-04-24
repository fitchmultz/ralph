//! Notification configuration resolution.
//!
//! Purpose:
//! - Notification configuration resolution.
//!
//! Responsibilities:
//! - Define CLI override and runtime configuration types for notifications.
//! - Merge config-file values with CLI overrides into a resolved runtime config.
//!
//! Non-scope:
//! - Notification display or sound playback.
//! - UI activity detection beyond suppression flags.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Runtime config defaults remain explicit and stable.
//! - CLI override precedence is preserved over stored configuration.

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
    /// Enable desktop notifications when watch mode adds new tasks from comments.
    pub notify_on_watch_new_tasks: bool,
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
            notify_on_watch_new_tasks: true,
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
        ui_active && self.suppress_when_active
    }
}

/// Build a runtime NotificationConfig from config and CLI overrides.
///
/// Precedence: CLI overrides > config values > defaults.
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
    let notify_on_watch_new_tasks = config.notify_on_watch_new_tasks.unwrap_or(true);

    NotificationConfig {
        enabled: notify_on_complete
            || notify_on_fail
            || notify_on_loop_complete
            || notify_on_watch_new_tasks,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete,
        notify_on_watch_new_tasks,
        suppress_when_active: config.suppress_when_active.unwrap_or(true),
        sound_enabled: overrides
            .notify_sound
            .or(config.sound_enabled)
            .unwrap_or(false),
        sound_path: config.sound_path.clone(),
        timeout_ms: config.timeout_ms.unwrap_or(8000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_notification_config_uses_defaults() {
        let config = crate::contracts::NotificationConfig::default();
        let overrides = NotificationOverrides::default();
        let result = build_notification_config(&config, &overrides);

        assert!(result.enabled);
        assert!(result.notify_on_complete);
        assert!(result.notify_on_fail);
        assert!(result.notify_on_loop_complete);
        assert!(result.notify_on_watch_new_tasks);
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

        assert!(result.notify_on_complete);
        assert!(result.notify_on_fail);
        assert!(result.sound_enabled);
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

        assert!(!result.notify_on_complete);
        assert!(result.notify_on_fail);
        assert!(!result.suppress_when_active);
        assert_eq!(result.timeout_ms, 5000);
        assert_eq!(result.sound_path, Some("/path/to/sound.wav".to_string()));
    }

    #[test]
    fn build_notification_config_enabled_computed_correctly() {
        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(false),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            notify_on_watch_new_tasks: Some(false),
            ..Default::default()
        };
        let overrides = NotificationOverrides::default();
        let result = build_notification_config(&config, &overrides);
        assert!(!result.enabled);

        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(true),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            ..Default::default()
        };
        let result = build_notification_config(&config, &overrides);
        assert!(result.enabled);

        let config = crate::contracts::NotificationConfig {
            notify_on_complete: Some(false),
            notify_on_fail: Some(false),
            notify_on_loop_complete: Some(false),
            notify_on_watch_new_tasks: Some(true),
            ..Default::default()
        };
        let result = build_notification_config(&config, &overrides);
        assert!(result.enabled);
        assert!(result.notify_on_watch_new_tasks);
    }
}
