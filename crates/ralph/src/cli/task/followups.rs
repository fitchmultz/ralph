//! Handler for `ralph task followups ...` commands.
//!
//! Purpose:
//! - Handler for `ralph task followups ...` commands.
//!
//! Responsibilities:
//! - Lock queue state before applying proposal-backed queue growth.
//! - Delegate validation/materialization to queue operations.
//! - Render human-readable or JSON apply reports.
//!
//! Not handled here:
//! - Proposal schema semantics beyond CLI option mapping.
//! - Worker prompt guidance or parallel integration orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Successful non-dry-run applies create undo before saving queue changes.
//! - Dry runs never save queue changes or remove proposal files.

use anyhow::Result;

use crate::cli::task::args::{TaskFollowupsArgs, TaskFollowupsCommand, TaskFollowupsFormatArg};
use crate::config;
use crate::queue::{self, FollowupApplyOptions, FollowupApplyReport};

pub fn handle(args: &TaskFollowupsArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    match &args.command {
        TaskFollowupsCommand::Apply(args) => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "task followups apply", force)?;
            let report = queue::apply_followups_file(
                resolved,
                &FollowupApplyOptions {
                    task_id: args.task.as_str(),
                    input_path: args.input.as_deref(),
                    dry_run: args.dry_run,
                    create_undo: true,
                    remove_proposal: true,
                },
            )?;
            print_report(&report, args.format)
        }
    }
}

fn print_report(report: &FollowupApplyReport, format: TaskFollowupsFormatArg) -> Result<()> {
    match format {
        TaskFollowupsFormatArg::Text => print_text_report(report),
        TaskFollowupsFormatArg::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
            Ok(())
        }
    }
}

fn print_text_report(report: &FollowupApplyReport) -> Result<()> {
    let verb = if report.dry_run {
        "Would create"
    } else {
        "Applied"
    };
    let count = report.created_tasks.len();
    println!(
        "{verb} {count} follow-up task(s) for {}.",
        report.source_task_id
    );
    if count == 0 {
        println!("No follow-up tasks were proposed.");
        return Ok(());
    }

    for task in &report.created_tasks {
        if task.depends_on.is_empty() {
            println!("  - {} [{}]: {}", task.task_id, task.key, task.title);
        } else {
            println!(
                "  - {} [{}]: {} (depends_on: {})",
                task.task_id,
                task.key,
                task.title,
                task.depends_on.join(", ")
            );
        }
    }
    Ok(())
}
