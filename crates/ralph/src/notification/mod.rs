//! Desktop notification system for task completion and failures.
//!
//! Responsibilities:
//! - Expose the notification configuration and runtime API.
//! - Coordinate notification delivery, suppression, and optional sound playback.
//! - Keep platform-specific display and sound logic isolated in focused submodules.
//!
//! Does NOT handle:
//! - Notification scheduling or queuing (callers trigger explicitly).
//! - Persistent notification history or logging.
//! - UI mode detection (callers should suppress if desired).
//! - Do Not Disturb detection (handled at call site if needed).
//!
//! Invariants:
//! - Sound playback failures do not fail the notification call.
//! - Notification failures are logged but do not fail the calling operation.
//! - Public call sites continue to use `crate::notification::{...}` without change.

mod config;
mod display;
mod sound;

pub use config::{NotificationConfig, NotificationOverrides, build_notification_config};
pub use sound::play_completion_sound;

use display::{
    NotificationDisplayRequest, show_failure_notification, show_loop_notification,
    show_task_notification, show_watch_notification,
};

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
    let request = match notification_type {
        NotificationType::TaskComplete => NotificationDisplayRequest::Task {
            kind: notification_type,
            task_id,
            task_title,
        },
        NotificationType::TaskFailed => NotificationDisplayRequest::Task {
            kind: notification_type,
            task_id,
            task_title,
        },
        NotificationType::LoopComplete {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        } => NotificationDisplayRequest::Loop {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        },
    };
    dispatch_notification(notification_type, request, config, ui_active);
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
    dispatch_notification(
        NotificationType::TaskFailed,
        NotificationDisplayRequest::Failure {
            task_id,
            task_title,
            error,
        },
        config,
        false,
    );
}

/// Send loop completion notification.
/// Silently logs errors but never fails the calling operation.
pub fn notify_loop_complete(
    tasks_total: usize,
    tasks_succeeded: usize,
    tasks_failed: usize,
    config: &NotificationConfig,
) {
    dispatch_notification(
        NotificationType::LoopComplete {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        },
        NotificationDisplayRequest::Loop {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        },
        config,
        false,
    );
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

    if let Err(error) = show_watch_notification(count, config.timeout_ms) {
        log::debug!("Failed to show watch notification: {}", error);
    }
    play_sound_if_enabled(config);
}

fn dispatch_notification(
    notification_type: NotificationType,
    request: NotificationDisplayRequest<'_>,
    config: &NotificationConfig,
    ui_active: bool,
) {
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

    let display_result = match request {
        NotificationDisplayRequest::Task {
            kind,
            task_id,
            task_title,
        } => show_task_notification(kind, task_id, task_title, config.timeout_ms),
        NotificationDisplayRequest::Failure {
            task_id,
            task_title,
            error,
        } => show_failure_notification(task_id, task_title, error, config.timeout_ms),
        NotificationDisplayRequest::Loop {
            tasks_total,
            tasks_succeeded,
            tasks_failed,
        } => show_loop_notification(
            tasks_total,
            tasks_succeeded,
            tasks_failed,
            config.timeout_ms,
        ),
    };

    if let Err(error) = display_result {
        log::debug!("Failed to show notification: {}", error);
    }
    play_sound_if_enabled(config);
}

fn play_sound_if_enabled(config: &NotificationConfig) {
    if config.sound_enabled
        && let Err(error) = play_completion_sound(config.sound_path.as_deref())
    {
        log::debug!("Failed to play sound: {}", error);
    }
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
}
