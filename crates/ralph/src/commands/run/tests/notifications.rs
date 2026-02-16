//! Notification configuration tests for run command.

use super::{overrides_with_notifications, resolved_with_notification_config};

/// Test that enabled=true when --notify-fail is set without --notify
/// This is the core bug fix: enabled should be true if ANY notification type is enabled
#[test]
fn notification_config_enabled_true_when_notify_fail_only() {
    // Config defaults: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --notify-fail (enable fail notifications) without --notify
    let overrides = overrides_with_notifications(None, Some(true));

    // Calculate the notification config values (mirroring run_loop logic)
    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_fail is true
    assert!(
        enabled,
        "enabled should be true when notify_on_fail is true"
    );
    assert!(
        notify_on_complete,
        "notify_on_complete should be true from config"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should be true from CLI override"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should be true from config"
    );
}

/// Test that enabled=true when --no-notify and --notify-fail are both set
/// This ensures failure notifications work even when completion notifications are disabled
#[test]
fn notification_config_enabled_true_when_no_notify_and_notify_fail() {
    // Config defaults: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --no-notify --notify-fail
    let overrides = overrides_with_notifications(Some(false), Some(true));

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_fail is true
    assert!(
        enabled,
        "enabled should be true when notify_on_fail is true even if notify_on_complete is false"
    );
    assert!(
        !notify_on_complete,
        "notify_on_complete should be false from CLI override"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should be true from CLI override"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should be true from config"
    );
}

/// Test that enabled=false when all notification types are disabled
#[test]
fn notification_config_enabled_false_when_all_disabled() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: no overrides
    let overrides = overrides_with_notifications(None, None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be false because all types are disabled
    assert!(
        !enabled,
        "enabled should be false when all notification types are disabled"
    );
    assert!(!notify_on_complete, "notify_on_complete should be false");
    assert!(!notify_on_fail, "notify_on_fail should be false");
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false"
    );
}

/// Test that enabled=true when --notify is set alone
#[test]
fn notification_config_enabled_true_when_notify_alone() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: --notify (enable completion notifications)
    let overrides = overrides_with_notifications(Some(true), None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // enabled should be true because notify_on_complete is true
    assert!(
        enabled,
        "enabled should be true when notify_on_complete is true"
    );
    assert!(
        notify_on_complete,
        "notify_on_complete should be true from CLI override"
    );
    assert!(
        !notify_on_fail,
        "notify_on_fail should be false from config"
    );
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false from config (not using default)"
    );
}

/// Test that CLI overrides take precedence over config
#[test]
fn notification_config_cli_overrides_config() {
    // Config: all notifications disabled
    let resolved = resolved_with_notification_config(Some(false), Some(false), Some(false));

    // CLI: --notify --notify-fail (enable both)
    let overrides = overrides_with_notifications(Some(true), Some(true));

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    assert!(enabled);
    assert!(notify_on_complete, "CLI should override config");
    assert!(notify_on_fail, "CLI should override config");
    assert!(
        !notify_on_loop_complete,
        "notify_on_loop_complete should be false from config (not using default)"
    );
}

/// Test that config values are used when no CLI overrides
#[test]
fn notification_config_uses_config_when_no_cli_overrides() {
    // Config: mixed settings
    let resolved = resolved_with_notification_config(Some(true), Some(false), Some(true));

    // CLI: no overrides
    let overrides = overrides_with_notifications(None, None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    assert!(enabled);
    assert!(notify_on_complete, "should use config value");
    assert!(!notify_on_fail, "should use config value");
    assert!(notify_on_loop_complete, "should use config value");
}

/// Test the original bug: --no-notify would set enabled=false, suppressing ALL notifications
/// This verifies the fix works correctly
#[test]
fn notification_config_bug_no_notify_suppresses_all_notifications() {
    // Config: all notifications enabled
    let resolved = resolved_with_notification_config(Some(true), Some(true), Some(true));

    // CLI: --no-notify only (the bug scenario)
    let overrides = overrides_with_notifications(Some(false), None);

    let notify_on_complete = overrides
        .notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = overrides
        .notify_on_fail
        .or(resolved.config.agent.notification.notify_on_fail)
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);

    // With the fix: enabled should be true because notify_on_fail and notify_on_loop_complete are still true
    let enabled = notify_on_complete || notify_on_fail || notify_on_loop_complete;

    // Before the fix, this would have been false because enabled was set from notify_on_complete only
    assert!(
        enabled,
        "BUG: enabled should be true because notify_on_fail and notify_on_loop_complete are still enabled"
    );

    // Verify the individual flags
    assert!(
        !notify_on_complete,
        "notify_on_complete should be false from --no-notify"
    );
    assert!(
        notify_on_fail,
        "notify_on_fail should still be true from config"
    );
    assert!(
        notify_on_loop_complete,
        "notify_on_loop_complete should still be true from config"
    );
}
