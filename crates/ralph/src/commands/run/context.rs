//! Task context and status management for run execution.
//!
//! Responsibilities:
//! - Format task JSON as prompt context for runners.
//! - Mark tasks as Doing with webhook notifications.
//!
//! Not handled here:
//! - Task selection (handled by `selection` module).
//! - Task updates during execution (handled by `task` command).
//!
//! Invariants/assumptions:
//! - Task JSON serialization must succeed for context generation.
//! - Webhook notifications are best-effort (failures logged, not fatal).

use crate::config;
use crate::contracts::Task;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::timeutil;
use crate::webhook;
use anyhow::{Context, Result};

/// Format task context as a prompt block to prevent task switching during execution.
pub(crate) fn task_context_for_prompt(task: &Task) -> Result<String> {
    let id = task.id.trim();
    let title = task.title.trim();
    let rendered =
        serde_json::to_string_pretty(task).context("serialize task JSON for prompt context")?;

    Ok(format!(
        r#"# CURRENT TASK (AUTHORITATIVE)

You MUST work on this exact task and no other task.
- Do NOT switch tasks based on queue order, "first todo", or "lowest ID".
- Ignore `.ralph/done.json` except as historical reference if explicitly needed.
- Do NOT change task status manually.

Task ID: {id}
Title: {title}

Raw task JSON (source of truth):
```json
{rendered}
```
"#,
    ))
}

/// Mark task as Doing and trigger webhook notification.
pub(crate) fn mark_task_doing(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    // Get task title before modification for webhook
    let task_title = queue_file
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .map(|t| t.title.clone())
        .unwrap_or_default();

    let now = timeutil::now_utc_rfc3339()?;
    queue::set_status(&mut queue_file, task_id, TaskStatus::Doing, &now, None)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Trigger webhook for task started
    webhook::notify_task_started(task_id, &task_title, &resolved.config.agent.webhook, &now);

    Ok(())
}
