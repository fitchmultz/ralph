//! Dry-run presenters for `ralph task batch`.
//!
//! Purpose:
//! - Dry-run presenters for `ralph task batch`.
//!
//! Responsibilities:
//! - Provide stable preview output for each batch operation.
//! - Keep the main batch handler free of formatting noise.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::contracts::TaskStatus;

pub(super) fn simple(action: &str, task_ids: &[String]) {
    println!("{action}");
    for task_id in task_ids {
        println!("  - {}", task_id);
    }
    println!("\nDry run complete. No changes made.");
}

pub(super) fn status(status: TaskStatus, task_ids: &[String]) {
    simple(
        &format!(
            "Dry run - would update {} tasks to status '{}':",
            task_ids.len(),
            status
        ),
        task_ids,
    );
}

pub(super) fn terminal_status(status: TaskStatus, task_ids: &[String]) {
    simple(
        &format!(
            "Dry run - would mark {} tasks as {} and archive them:",
            task_ids.len(),
            status
        ),
        task_ids,
    );
}

pub(super) fn field(key: &str, value: &str, task_ids: &[String]) {
    simple(
        &format!(
            "Dry run - would set field '{}' = '{}' on {} tasks:",
            key,
            value,
            task_ids.len()
        ),
        task_ids,
    );
}

pub(super) fn clone_tasks(status: TaskStatus, title_prefix: Option<&str>, task_ids: &[String]) {
    println!(
        "Dry run - would clone {} tasks with status '{}':",
        task_ids.len(),
        status
    );
    for task_id in task_ids {
        let prefix_info = title_prefix
            .map(|p| format!(" [prefix: '{}']", p))
            .unwrap_or_default();
        println!("  - {}{}", task_id, prefix_info);
    }
    println!("\nDry run complete. No changes made.");
}

pub(super) fn split_tasks(
    count: usize,
    status: TaskStatus,
    distribute_plan: bool,
    task_ids: &[String],
) {
    println!(
        "Dry run - would split {} tasks into {} children each with status '{}':",
        task_ids.len(),
        count,
        status
    );
    for task_id in task_ids {
        let dist_info = if distribute_plan {
            " [distribute plan]"
        } else {
            ""
        };
        println!("  - {}{}", task_id, dist_info);
    }
    println!("\nDry run complete. No changes made.");
}

pub(super) fn plan_items(action: &str, plan_items: &[String], task_ids: &[String]) {
    println!(
        "Dry run - would {} {} plan items to {} tasks:",
        action,
        plan_items.len(),
        task_ids.len()
    );
    println!("Plan items to {}:", action);
    for item in plan_items {
        println!("  - {}", item);
    }
    println!("\nTarget tasks:");
    for task_id in task_ids {
        println!("  - {}", task_id);
    }
    println!("\nDry run complete. No changes made.");
}
