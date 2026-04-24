//! User-facing output and confirmation helpers for `ralph queue issue`.
//!
//! Purpose:
//! - User-facing output and confirmation helpers for `ralph queue issue`.
//!
//! Responsibilities:
//! - Render dry-run previews and bulk publish summaries.
//! - Prompt for interactive bulk execution confirmation.
//! - Centralize stdout/stderr-facing text so handlers stay orchestration-focused.
//!
//! Not handled here:
//! - GitHub issue mutation.
//! - Queue selection/filter parsing.
//! - Queue persistence.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview output must preserve existing CLI text/contracts.
//! - Confirmation prompts flush stdout before reading input.
//! - Failed publish results are surfaced as errors instead of formatted success output.

use anyhow::{Context, Result};
use std::io::{self, BufRead, IsTerminal, Write};

use crate::contracts::QueueFile;

use super::common::{
    GITHUB_ISSUE_URL_KEY, PublishItemResult, PublishManySummary, fetch_custom_field, find_task,
};

pub(super) fn print_single_publish_result(
    queue: &QueueFile,
    task_id: &str,
    result: PublishItemResult,
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<()> {
    let task = find_task(queue, task_id)?;
    let title = format!("{}: {}", task.id, task.title);
    let body = super::super::export::render_task_as_github_issue_body(task);

    println!("=== DRY RUN ===");
    println!("Target task: {task_id}");
    println!("Title: {title}");
    println!();
    println!("Body:");
    println!("{body}");

    match result {
        PublishItemResult::Created => println!("Would create new GitHub issue."),
        PublishItemResult::Updated => {
            let existing_url = fetch_custom_field(&task.custom_fields, GITHUB_ISSUE_URL_KEY)
                .unwrap_or_else(|| "unknown".to_string());
            println!("Would update existing issue: {existing_url}");
        }
        PublishItemResult::SkippedUnchanged => {
            println!("Would skip task; issue payload is already synced.");
        }
        PublishItemResult::Failed(err) => return Err(err),
    }

    if let Some(repo) = repo {
        println!("Target repo: {repo}");
    }
    if !labels.is_empty() {
        println!("Labels: {}", labels.join(", "));
    }
    if !assignees.is_empty() {
        println!("Assignees: {}", assignees.join(", "));
    }

    Ok(())
}

pub(super) fn print_publish_many_plan(results: &[(String, PublishItemResult)]) {
    println!("publish-many plan for {} task(s):", results.len());
    for (task_id, result) in results {
        if let PublishItemResult::Failed(err) = result {
            println!("  [{}] {task_id}: {err}", result.label());
        } else {
            println!("  [{}] {task_id}", result.label());
        }
    }
}

pub(super) fn print_publish_many_task_result(task_id: &str, result: &PublishItemResult) {
    if let PublishItemResult::Failed(err) = result {
        println!("  [{}] {task_id}: {err}", result.label());
    } else {
        println!("  [{}] {task_id}", result.label());
    }
}

pub(super) fn print_publish_many_summary(summary: &PublishManySummary, dry_run: bool) {
    let mode = if dry_run { "dry-run" } else { "execution" };
    println!(
        "publish-many {mode} summary: selected={} created={} updated={} skipped={} failed={}",
        summary.selected, summary.created, summary.updated, summary.skipped, summary.failed,
    );
}

pub(super) fn print_failures(failures: &[(String, String)]) {
    println!();
    println!("Failures:");
    for (task_id, reason) in failures {
        println!("  {task_id}: {reason}");
    }
}

pub(super) fn confirm_execution(summary: &PublishManySummary) -> Result<bool> {
    println!("About to execute {} task(s):", summary.selected);
    println!("  create: {}", summary.created);
    println!("  update: {}", summary.updated);
    println!("  skip: {}", summary.skipped);
    print!("Proceed with publish-many execution? [y/N]: ");
    io::stdout().flush().context("flush confirmation prompt")?;

    let mut response = String::new();
    io::stdin()
        .lock()
        .read_line(&mut response)
        .context("read confirmation input")?;
    Ok(matches!(
        response.trim().to_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(super) fn is_terminal_context() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}
