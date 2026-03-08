//! Queue explain subcommand.
//!
//! Responsibilities:
//! - Report why tasks are (not) runnable with human-readable text or JSON output.
//!
//! Does not handle:
//! - Task selection or execution (see `crate::commands::run`).
//! - Queue persistence or mutations.
//!
//! Invariants/assumptions:
//! - JSON output is versioned and stable for scripting.
//! - Text output includes actionable hints for next steps.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues_read_only;
use crate::cli::queue::shared::QueueReportFormat;
use crate::config::Resolved;
use crate::queue::operations::{RunnableSelectionOptions, queue_runnability_report};

/// Arguments for `ralph queue explain`.
#[derive(Args)]
#[command(
    about = "Explain why tasks are (not) runnable",
    after_long_help = "Examples:\n\
  ralph queue explain\n\
  ralph queue explain --format json\n\
  ralph queue explain --include-draft\n\
  ralph queue explain --format json --include-draft"
)]
pub struct QueueExplainArgs {
    /// Output format (text or json).
    #[arg(long, value_enum, default_value_t = QueueReportFormat::Text)]
    pub format: QueueReportFormat,

    /// Include draft tasks in the analysis.
    #[arg(long)]
    pub include_draft: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueExplainArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues_read_only(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let options = RunnableSelectionOptions::new(args.include_draft, true);
    let report = queue_runnability_report(&queue_file, done_ref, options)?;

    match args.format {
        QueueReportFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        QueueReportFormat::Text => {
            print_text_explanation(&report);
        }
    }

    Ok(())
}

fn print_text_explanation(report: &crate::queue::operations::QueueRunnabilityReport) {
    use crate::queue::operations::NotRunnableReason;

    // Summary header
    println!("Queue Runnability Report (generated at {})", report.now);
    println!();

    // Selection context
    println!(
        "Selection: include_draft={}, prefer_doing={}",
        report.selection.include_draft, report.selection.prefer_doing
    );

    match (
        report.selection.selected_task_id.as_deref(),
        report.selection.selected_task_status,
    ) {
        (Some(id), Some(status)) => {
            println!("Selected task: {} (status: {:?})", id, status);
        }
        (Some(id), None) => {
            // Shouldn't happen, but keep text output resilient.
            println!("Selected task: {} (status: unknown)", id);
        }
        (None, _) => {
            println!("Selected task: none (no runnable tasks found)");
        }
    }
    println!();

    // Summary counts
    println!("Summary:");
    println!("  Total tasks: {}", report.summary.total_active);
    println!(
        "  Candidates: {} (runnable: {})",
        report.summary.candidates_total, report.summary.runnable_candidates
    );
    if report.summary.blocked_by_dependencies > 0 {
        println!(
            "  Blocked by dependencies: {}",
            report.summary.blocked_by_dependencies
        );
    }
    if report.summary.blocked_by_schedule > 0 {
        println!(
            "  Blocked by schedule: {}",
            report.summary.blocked_by_schedule
        );
    }
    if report.summary.blocked_by_status_or_flags > 0 {
        println!(
            "  Blocked by status/flags: {}",
            report.summary.blocked_by_status_or_flags
        );
    }
    println!();

    // If no runnable tasks, show first few blockers
    if report.selection.selected_task_id.is_none() && report.summary.candidates_total > 0 {
        println!("Blocking reasons (first 10 candidates):");
        let mut shown = 0;
        for row in &report.tasks {
            // Only show candidates (Todo or Draft if include_draft)
            let is_candidate = row.status == crate::contracts::TaskStatus::Todo
                || (report.selection.include_draft
                    && row.status == crate::contracts::TaskStatus::Draft);
            if !is_candidate || row.runnable {
                continue;
            }

            println!("  {} (status: {:?}):", row.id, row.status);
            for reason in &row.reasons {
                match reason {
                    NotRunnableReason::StatusNotRunnable { status } => {
                        println!("    - Status prevents running: {}", status);
                    }
                    NotRunnableReason::DraftExcluded => {
                        println!("    - Draft tasks excluded (use --include-draft)");
                    }
                    NotRunnableReason::UnmetDependencies { dependencies } => {
                        println!("    - Blocked by unmet dependencies:");
                        for dep in dependencies {
                            match dep {
                                crate::queue::operations::DependencyIssue::Missing { id } => {
                                    println!("      * {}: dependency not found", id);
                                }
                                crate::queue::operations::DependencyIssue::NotComplete {
                                    id,
                                    status,
                                } => {
                                    println!(
                                        "      * {}: status is '{}' (must be done/rejected)",
                                        id, status
                                    );
                                }
                            }
                        }
                    }
                    NotRunnableReason::ScheduledStartInFuture {
                        scheduled_start,
                        seconds_until_runnable,
                        ..
                    } => {
                        let hours = seconds_until_runnable / 3600;
                        let minutes = (seconds_until_runnable % 3600) / 60;
                        if hours > 0 {
                            println!(
                                "    - Scheduled for future: {} (in {}h {}m)",
                                scheduled_start, hours, minutes
                            );
                        } else {
                            println!(
                                "    - Scheduled for future: {} (in {}m)",
                                scheduled_start, minutes
                            );
                        }
                    }
                }
            }

            shown += 1;
            if shown >= 10 {
                let remaining =
                    report.summary.candidates_total - report.summary.runnable_candidates - shown;
                if remaining > 0 {
                    println!("  ... and {} more blocked tasks", remaining);
                }
                break;
            }
        }
        println!();

        // Hints
        println!("Hints:");
        if report.summary.blocked_by_dependencies > 0 {
            println!("  - Run 'ralph queue graph --task <ID>' to visualize dependencies");
        }
        if report.summary.blocked_by_schedule > 0 {
            println!("  - Run 'ralph queue list --scheduled' to see scheduled tasks");
        }
        println!("  - Run 'ralph run one --dry-run' to see what would be selected");
    }

    // If there is a selected task, show its details
    if let Some(ref id) = report.selection.selected_task_id
        && let Some(row) = report.tasks.iter().find(|t| &t.id == id)
    {
        println!("Selected task details:");
        println!("  ID: {}", row.id);
        println!("  Status: {:?}", row.status);
        if row.runnable {
            println!("  Runnability: ready to run");
        } else {
            println!("  Runnability: NOT runnable");
            if !row.reasons.is_empty() {
                println!("  Reasons:");
                for reason in &row.reasons {
                    match reason {
                        NotRunnableReason::StatusNotRunnable { status } => {
                            println!("    - Status prevents running: {}", status);
                        }
                        NotRunnableReason::DraftExcluded => {
                            println!("    - Draft tasks excluded (use --include-draft)");
                        }
                        NotRunnableReason::UnmetDependencies { dependencies } => {
                            println!("    - Blocked by unmet dependencies:");
                            for dep in dependencies {
                                match dep {
                                    crate::queue::operations::DependencyIssue::Missing { id } => {
                                        println!("      * {}: dependency not found", id);
                                    }
                                    crate::queue::operations::DependencyIssue::NotComplete {
                                        id,
                                        status,
                                    } => {
                                        println!(
                                            "      * {}: status is '{}' (must be done/rejected)",
                                            id, status
                                        );
                                    }
                                }
                            }
                        }
                        NotRunnableReason::ScheduledStartInFuture {
                            scheduled_start,
                            seconds_until_runnable,
                            ..
                        } => {
                            let hours = seconds_until_runnable / 3600;
                            let minutes = (seconds_until_runnable % 3600) / 60;
                            if hours > 0 {
                                println!(
                                    "    - Scheduled for future: {} (in {}h {}m)",
                                    scheduled_start, hours, minutes
                                );
                            } else {
                                println!(
                                    "    - Scheduled for future: {} (in {}m)",
                                    scheduled_start, minutes
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueueExplainArgs;
    use crate::cli::queue::shared::QueueReportFormat;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: QueueExplainArgs,
    }

    #[test]
    fn explain_args_default_format_is_text() {
        let cli = TestCli::parse_from(["test"]);
        assert!(matches!(cli.args.format, QueueReportFormat::Text));
        assert!(!cli.args.include_draft);
    }

    #[test]
    fn explain_args_json_format() {
        let cli = TestCli::parse_from(["test", "--format", "json"]);
        assert!(matches!(cli.args.format, QueueReportFormat::Json));
    }

    #[test]
    fn explain_args_include_draft() {
        let cli = TestCli::parse_from(["test", "--include-draft"]);
        assert!(cli.args.include_draft);
    }
}
