//! Notification display rendering.
//!
//! Purpose:
//! - Notification display rendering.
//!
//! Responsibilities:
//! - Render task, failure, loop, and watch notifications through `notify-rust` when enabled.
//! - Keep notification text formatting in one place instead of duplicating it at call sites.
//!
//! Non-scope:
//! - Sound playback.
//! - Notification suppression or config resolution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Missing `notifications` feature degrades to a debug log and success result.
//! - Notification titles/bodies stay consistent across all callers.

use super::NotificationType;

pub(super) enum NotificationDisplayRequest<'a> {
    Task {
        kind: NotificationType,
        task_id: &'a str,
        task_title: &'a str,
    },
    Failure {
        task_id: &'a str,
        task_title: &'a str,
        error: &'a str,
    },
    Loop {
        tasks_total: usize,
        tasks_succeeded: usize,
        tasks_failed: usize,
    },
}

pub(super) fn show_watch_notification(count: usize, timeout_ms: u32) -> anyhow::Result<()> {
    let body = if count == 1 {
        "1 new task detected from code comments".to_string()
    } else {
        format!("{} new tasks detected from code comments", count)
    };
    show_notification("Ralph: Watch Mode", &body, timeout_ms)
}

pub(super) fn show_task_notification(
    notification_type: NotificationType,
    task_id: &str,
    task_title: &str,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    let (summary, body) = match notification_type {
        NotificationType::TaskComplete => (
            "Ralph: Task Complete",
            format!("{} - {}", task_id, task_title),
        ),
        NotificationType::TaskFailed => (
            "Ralph: Task Failed",
            format!("{} - {}", task_id, task_title),
        ),
        NotificationType::LoopComplete { .. } => {
            log::warn!("Loop notifications use the loop path; skipping task display");
            return Ok(());
        }
        NotificationType::WatchNewTasks => {
            log::warn!("Watch notifications use the watch path; skipping task display");
            return Ok(());
        }
    };
    show_notification(summary, &body, timeout_ms)
}

pub(super) fn show_failure_notification(
    task_id: &str,
    task_title: &str,
    error: &str,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    let error_summary = if error.len() > 100 {
        format!("{}...", &error[..97])
    } else {
        error.to_string()
    };
    show_notification(
        "Ralph: Task Failed",
        &format!("{} - {}\nError: {}", task_id, task_title, error_summary),
        timeout_ms,
    )
}

pub(super) fn show_loop_notification(
    tasks_total: usize,
    tasks_succeeded: usize,
    tasks_failed: usize,
    timeout_ms: u32,
) -> anyhow::Result<()> {
    show_notification(
        "Ralph: Loop Complete",
        &format!(
            "{} tasks completed ({} succeeded, {} failed)",
            tasks_total, tasks_succeeded, tasks_failed
        ),
        timeout_ms,
    )
}

#[cfg(feature = "notifications")]
fn show_notification(summary: &str, body: &str, timeout_ms: u32) -> anyhow::Result<()> {
    use notify_rust::{Notification, Timeout};

    super::prepare_platform_notification_delivery();

    Notification::new()
        .summary(summary)
        .body(body)
        .timeout(Timeout::Milliseconds(timeout_ms))
        .show()
        .map_err(|error| anyhow::anyhow!("Failed to show notification: {}", error))?;

    Ok(())
}

#[cfg(not(feature = "notifications"))]
fn show_notification(_summary: &str, _body: &str, _timeout_ms: u32) -> anyhow::Result<()> {
    log::debug!("Notifications feature not compiled in; skipping notification display");
    Ok(())
}
