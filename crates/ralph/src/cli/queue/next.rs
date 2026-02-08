//! Queue next subcommand.
//!
//! Responsibilities:
//! - Print the next runnable task ID (or ID+title with --with-title).
//! - Provide explanation of why no task is runnable with --explain.
//! - Optionally display ETA estimate from execution history with --with-eta.
//!
//! Does not handle:
//! - Task execution (see `crate::commands::run`).
//! - Queue mutations.
//! - Real-time progress tracking (handled by external UI clients).
//!
//! Invariants/assumptions:
//! - When no runnable task exists, prints next available ID (for script compatibility).
//! - Explanations go to stderr to preserve stdout contract.
//! - ETA is based on execution history only; missing history shows "n/a".

use std::io::Write;

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::cli::queue::shared::task_eta_display;
use crate::config::Resolved;
use crate::eta_calculator::EtaCalculator;
use crate::queue::operations::{
    NotRunnableReason, RunnableSelectionOptions, queue_runnability_report,
};
use crate::{outpututil, queue};

/// Arguments for `ralph queue next`.
#[derive(Args)]
pub struct QueueNextArgs {
    /// Include the task title after the ID.
    #[arg(long)]
    pub with_title: bool,

    /// Include an execution-history-based ETA estimate.
    #[arg(long)]
    pub with_eta: bool,

    /// Print an explanation when no runnable task is found.
    #[arg(long)]
    pub explain: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueNextArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Load ETA calculator if needed
    let eta_calculator = args.with_eta.then(|| {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        EtaCalculator::load(&cache_dir)
    });

    // Get runnable task (same logic as run one)
    if let Some(next) = queue::next_runnable_task(&queue_file, done_ref) {
        if args.with_eta {
            let calc = eta_calculator
                .as_ref()
                .expect("with_eta implies eta_calculator exists");
            let eta = task_eta_display(resolved, calc, next);

            if args.with_title {
                println!(
                    "{}\t{}",
                    outpututil::format_task_id_title(&next.id, &next.title),
                    eta
                );
            } else {
                println!("{}\t{}", outpututil::format_task_id(&next.id), eta);
            }
        } else if args.with_title {
            println!(
                "{}",
                outpututil::format_task_id_title(&next.id, &next.title)
            );
        } else {
            println!("{}", outpututil::format_task_id(&next.id));
        }

        if args.explain {
            eprintln!("Task {} is runnable (status: {:?})", next.id, next.status);
        }
        return Ok(());
    }

    // No runnable task - get next available ID
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let next_id = queue::next_id_across(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    // Print with ETA column if requested (stable column count)
    if args.with_eta {
        println!("{}\tn/a", next_id);
    } else {
        println!("{next_id}");
    }

    // If --explain, provide detailed explanation to stderr
    if args.explain {
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();

        writeln!(handle, "No runnable task found.")?;

        // Build runnability report with same options as queue next (no draft, no doing preference)
        let options = RunnableSelectionOptions::new(false, false);
        match queue_runnability_report(&queue_file, done_ref, options) {
            Ok(report) => {
                // Find first blocking task
                let mut found_blocker = false;
                for row in &report.tasks {
                    // Only consider Todo candidates (same as next_runnable_task logic)
                    if row.status != crate::contracts::TaskStatus::Todo || row.runnable {
                        continue;
                    }

                    if !row.reasons.is_empty() {
                        found_blocker = true;
                        write!(handle, "First blocking task: {} (", row.id)?;

                        // Print first reason concisely
                        match &row.reasons[0] {
                            NotRunnableReason::StatusNotRunnable { status } => {
                                writeln!(handle, "status: {})", status)?;
                            }
                            NotRunnableReason::DraftExcluded => {
                                writeln!(handle, "draft excluded)")?;
                            }
                            NotRunnableReason::UnmetDependencies { dependencies } => {
                                if dependencies.len() == 1 {
                                    match &dependencies[0] {
                                        crate::queue::operations::DependencyIssue::Missing { id } => {
                                            writeln!(handle, "missing dependency: {})", id)?;
                                        }
                                        crate::queue::operations::DependencyIssue::NotComplete { id, status } => {
                                            writeln!(handle, "dependency {} not done: status={})", id, status)?;
                                        }
                                    }
                                } else {
                                    writeln!(handle, "{} unmet dependencies)", dependencies.len())?;
                                }
                            }
                            NotRunnableReason::ScheduledStartInFuture {
                                scheduled_start, ..
                            } => {
                                writeln!(handle, "scheduled: {})", scheduled_start)?;
                            }
                        }
                        break;
                    }
                }

                if !found_blocker {
                    writeln!(
                        handle,
                        "No blocking tasks found (queue may be empty or all done)."
                    )?;
                }

                writeln!(handle)?;
                writeln!(handle, "Run 'ralph queue explain' for a full report.")?;

                // Add hints based on blockers
                if report.summary.blocked_by_dependencies > 0 {
                    writeln!(
                        handle,
                        "Run 'ralph queue graph --task <ID>' to see dependencies."
                    )?;
                }
                if report.summary.blocked_by_schedule > 0 {
                    writeln!(
                        handle,
                        "Run 'ralph queue list --scheduled' to see scheduled tasks."
                    )?;
                }
            }
            Err(e) => {
                writeln!(handle, "Could not generate runnability report: {}", e)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::QueueNextArgs;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: QueueNextArgs,
    }

    #[test]
    fn next_args_default() {
        let cli = TestCli::parse_from(["test"]);
        assert!(!cli.args.with_title);
        assert!(!cli.args.explain);
    }

    #[test]
    fn next_args_with_title() {
        let cli = TestCli::parse_from(["test", "--with-title"]);
        assert!(cli.args.with_title);
    }

    #[test]
    fn next_args_explain() {
        let cli = TestCli::parse_from(["test", "--explain"]);
        assert!(cli.args.explain);
    }

    #[test]
    fn next_args_both_flags() {
        let cli = TestCli::parse_from(["test", "--with-title", "--explain"]);
        assert!(cli.args.with_title);
        assert!(cli.args.explain);
    }

    #[test]
    fn next_args_with_eta() {
        let cli = TestCli::parse_from(["test", "--with-eta"]);
        assert!(cli.args.with_eta);
        assert!(!cli.args.with_title);
        assert!(!cli.args.explain);
    }

    #[test]
    fn next_args_with_eta_and_title() {
        let cli = TestCli::parse_from(["test", "--with-eta", "--with-title"]);
        assert!(cli.args.with_eta);
        assert!(cli.args.with_title);
        assert!(!cli.args.explain);
    }

    #[test]
    fn next_args_all_flags() {
        let cli = TestCli::parse_from(["test", "--with-eta", "--with-title", "--explain"]);
        assert!(cli.args.with_eta);
        assert!(cli.args.with_title);
        assert!(cli.args.explain);
    }
}
