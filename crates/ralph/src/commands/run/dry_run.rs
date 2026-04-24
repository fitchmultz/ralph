//! Dry-run selection for `ralph run one --dry-run` and `ralph run loop --dry-run`.
//!
//! Purpose:
//! - Dry-run selection for `ralph run one --dry-run` and `ralph run loop --dry-run`.
//!
//! Responsibilities:
//! - Perform task selection without acquiring queue lock or modifying files.
//! - Explain why tasks are blocked using runnability reports.
//!
//! Not handled here:
//! - Actual task execution (see `run_one`).
//! - Queue lock management (see `queue_lock`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Dry-run mode must not modify queue/done files or start runner sessions.
//! - Selection messaging must keep key phrases stable (tests look for exact strings).

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::queue::RunnableSelectionOptions;
use crate::queue::operations::queue_runnability_report;
use anyhow::Result;

use super::selection::select_run_one_task_index;

/// Dry-run selection for `ralph run one --dry-run`.
///
/// Performs task selection without:
/// - Acquiring queue lock
/// - Marking task as doing
/// - Starting any runner session
/// - Modifying queue/done files
pub fn dry_run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    target_task_id: Option<&str>,
) -> Result<()> {
    // Load queue and done (no lock in dry-run mode).
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };

    let include_draft = agent_overrides.include_draft.unwrap_or(false);

    // Match run-time validation behavior without mutating anything.
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    queue::log_warnings(&warnings);

    // Try to select a task
    let selected = select_run_one_task_index(&queue_file, done_ref, target_task_id, include_draft)?;

    if let Some(idx) = selected {
        let task = &queue_file.tasks[idx];
        println!("Dry run: would run {} (status: {:?})", task.id, task.status);

        // Also show any blockers (in case task is runnable but has caveats)
        if task.status == TaskStatus::Doing {
            println!("  Note: Task is already in 'doing' status (resuming).");
        }
        return Ok(());
    }

    // No task selected - explain why
    println!("Dry run: no task would be run.");
    println!();

    // Count candidates
    let candidates: Vec<_> = queue_file
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Todo || (include_draft && t.status == TaskStatus::Draft)
        })
        .collect();

    if candidates.is_empty() {
        // Keep this phrase stable; some tests look for it.
        if include_draft {
            println!("No todo or draft tasks found.");
        } else {
            println!("No todo tasks found.");
        }
        return Ok(());
    }

    // Build runnability report to explain blockers
    let options = RunnableSelectionOptions::new(include_draft, true);
    match queue_runnability_report(&queue_file, done_ref, options) {
        Ok(report) => {
            if report.summary.runnable_candidates > 0 {
                // This shouldn't happen if selection returned None, but handle it
                println!(
                    "Warning: runnability report found {} runnable candidates but selection returned none.",
                    report.summary.runnable_candidates
                );
            } else {
                println!("Blockers preventing task execution:");

                if report.summary.blocked_by_dependencies > 0 {
                    println!(
                        "  - {} task(s) blocked by unmet dependencies",
                        report.summary.blocked_by_dependencies
                    );
                }
                if report.summary.blocked_by_schedule > 0 {
                    println!(
                        "  - {} task(s) blocked by future schedule",
                        report.summary.blocked_by_schedule
                    );
                }
                if report.summary.blocked_by_status_or_flags > 0 {
                    println!(
                        "  - {} task(s) blocked by status/flags (e.g., draft excluded)",
                        report.summary.blocked_by_status_or_flags
                    );
                }

                // Show first blocking task
                println!();
                for row in &report.tasks {
                    let is_candidate = row.status == TaskStatus::Todo
                        || (include_draft && row.status == TaskStatus::Draft);
                    if !is_candidate || row.runnable || row.reasons.is_empty() {
                        continue;
                    }

                    println!("First blocking task: {} (status: {:?})", row.id, row.status);
                    for reason in &row.reasons {
                        match reason {
                            crate::queue::operations::NotRunnableReason::UnmetDependencies { dependencies } => {
                                if dependencies.len() == 1 {
                                    match &dependencies[0] {
                                        crate::queue::operations::DependencyIssue::Missing { id } => {
                                            println!("  - Missing dependency: {}", id);
                                        }
                                        crate::queue::operations::DependencyIssue::NotComplete { id, status } => {
                                            println!("  - Dependency {} not complete (status: {})", id, status);
                                        }
                                    }
                                } else {
                                    println!("  - {} unmet dependencies", dependencies.len());
                                }
                            }
                            crate::queue::operations::NotRunnableReason::ScheduledStartInFuture { scheduled_start, seconds_until_runnable, .. } => {
                                let hours = seconds_until_runnable / 3600;
                                let minutes = (seconds_until_runnable % 3600) / 60;
                                if hours > 0 {
                                    println!("  - Scheduled for: {} (in {}h {}m)",
                                        scheduled_start, hours, minutes);
                                } else {
                                    println!("  - Scheduled for: {} (in {}m)",
                                        scheduled_start, minutes);
                                }
                            }
                            crate::queue::operations::NotRunnableReason::DraftExcluded => {
                                println!("  - Draft tasks excluded (use --include-draft)");
                            }
                            crate::queue::operations::NotRunnableReason::StatusNotRunnable { status } => {
                                println!("  - Status prevents running: {}", status);
                            }
                        }
                    }
                    break;
                }
            }
        }
        Err(e) => {
            println!("Could not generate runnability report: {}", e);
        }
    }

    println!();
    println!("Run 'ralph queue explain' for a full report.");

    Ok(())
}

/// Dry-run selection for `ralph run loop --dry-run`.
///
/// Reports the first selection only since subsequent tasks depend on outcomes.
pub fn dry_run_loop(resolved: &config::Resolved, agent_overrides: &AgentOverrides) -> Result<()> {
    println!("Dry run: simulating run loop (reporting first selection only).");
    println!("Note: Subsequent tasks depend on outcomes of earlier tasks.");
    println!();

    dry_run_one(resolved, agent_overrides, None)
}
