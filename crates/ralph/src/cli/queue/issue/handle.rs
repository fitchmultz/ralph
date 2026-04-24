//! Command orchestration for `ralph queue issue`.
//!
//! Purpose:
//! - Command orchestration for `ralph queue issue`.
//!
//! Responsibilities:
//! - Route single-task and bulk GitHub issue publish commands.
//! - Keep execute-mode queue mutation fenced behind queue locks.
//! - Coordinate filter planning, confirmation, summary reporting, and queue saves.
//!
//! Not handled here:
//! - Clap type definitions.
//! - GitHub issue payload mutation internals.
//! - Low-level output formatting details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue validation/save only happens after successful execute-mode mutations.
//! - Bulk dry-run planning never mutates queue files or GitHub state.
//! - Non-interactive execute mode still requires `--force`.

use anyhow::{Result, bail};

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::git::check_gh_available;

use super::args::{
    QueueIssueArgs, QueueIssueCommand, QueueIssuePublishArgs, QueueIssuePublishManyArgs,
};
use super::common::{
    PublishItemResult, PublishManySummary, PublishMode, accumulate_publish_result,
    parse_publish_many_filters, resolve_publish_mode, select_publishable_task_ids,
};
use super::output::{
    confirm_execution, is_terminal_context, print_failures, print_publish_many_plan,
    print_publish_many_summary, print_publish_many_task_result, print_single_publish_result,
};
use super::publish::publish_task;

pub(crate) fn handle(resolved: &Resolved, force: bool, args: QueueIssueArgs) -> Result<()> {
    match args.command {
        QueueIssueCommand::Publish(args) => handle_publish(resolved, force, args),
        QueueIssueCommand::PublishMany(args) => handle_publish_many(resolved, force, args),
    }
}

pub(crate) fn handle_publish(
    resolved: &Resolved,
    force: bool,
    args: QueueIssuePublishArgs,
) -> Result<()> {
    let task_id = args.task_id.trim();
    if task_id.is_empty() {
        bail!("Task ID must be non-empty");
    }

    if args.dry_run {
        let (mut queue_file, _done_file) = load_and_validate_queues_read_only(resolved, false)?;
        let result = publish_task(
            resolved,
            &mut queue_file,
            task_id,
            PublishMode::DryRun,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        )?;

        return print_single_publish_result(
            &queue_file,
            task_id,
            result,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        );
    }

    check_gh_available()?;
    crate::queue::with_locked_queue_mutation(
        resolved,
        "queue issue publish",
        format!("queue issue publish {task_id}"),
        force,
        || {
            let (mut queue_file, _done_file) =
                crate::queue::load_and_validate_queues(resolved, false)?;
            let result = publish_task(
                resolved,
                &mut queue_file,
                task_id,
                PublishMode::Execute,
                &args.label,
                &args.assignee,
                args.repo.as_deref(),
            )?;

            if matches!(
                result,
                PublishItemResult::Created | PublishItemResult::Updated
            ) {
                crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
                crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
            }

            print_single_publish_result(
                &queue_file,
                task_id,
                result,
                &args.label,
                &args.assignee,
                args.repo.as_deref(),
            )
        },
    )
}

pub(crate) fn handle_publish_many(
    resolved: &Resolved,
    force: bool,
    args: QueueIssuePublishManyArgs,
) -> Result<()> {
    let mode = resolve_publish_mode(args.dry_run, args.execute)?;
    let filters = parse_publish_many_filters(&args)?;
    let (queue_for_plan, _done_file) = load_and_validate_queues_read_only(resolved, false)?;
    let selected_task_ids = select_publishable_task_ids(&queue_for_plan, &filters);

    if selected_task_ids.is_empty() {
        println!("No matching tasks found for publish-many filters.");
        return Ok(());
    }

    let (planned, plan_summary) = build_publish_many_plan(
        resolved,
        queue_for_plan,
        &selected_task_ids,
        &args.label,
        &args.assignee,
        args.repo.as_deref(),
    )?;

    print_publish_many_plan(&planned);
    print_publish_many_summary(&plan_summary, true);

    if matches!(mode, PublishMode::DryRun) {
        return Ok(());
    }

    if !force {
        if !is_terminal_context() {
            bail!(
                "Refusing to execute bulk publish in non-interactive context without --force. Use --dry-run first."
            );
        }
        if !confirm_execution(&plan_summary)? {
            bail!("Bulk publish cancelled by user");
        }
    }

    check_gh_available()?;
    crate::queue::with_locked_queue_mutation(
        resolved,
        "queue issue publish-many",
        "queue issue publish-many",
        force,
        || execute_publish_many(resolved, &selected_task_ids, &args),
    )
}

fn build_publish_many_plan(
    resolved: &Resolved,
    mut queue_file: crate::contracts::QueueFile,
    selected_task_ids: &[String],
    labels: &[String],
    assignees: &[String],
    repo: Option<&str>,
) -> Result<(Vec<(String, PublishItemResult)>, PublishManySummary)> {
    let mut summary = PublishManySummary {
        selected: selected_task_ids.len(),
        ..PublishManySummary::default()
    };
    let mut planned = Vec::with_capacity(selected_task_ids.len());

    for task_id in selected_task_ids {
        let result = publish_task(
            resolved,
            &mut queue_file,
            task_id,
            PublishMode::DryRun,
            labels,
            assignees,
            repo,
        )?;

        accumulate_publish_result(&mut summary, &result);
        planned.push((task_id.clone(), result));
    }

    Ok((planned, summary))
}

fn execute_publish_many(
    resolved: &Resolved,
    selected_task_ids: &[String],
    args: &QueueIssuePublishManyArgs,
) -> Result<()> {
    let (mut queue_file, _done_file) = crate::queue::load_and_validate_queues(resolved, false)?;
    let mut final_summary = PublishManySummary {
        selected: selected_task_ids.len(),
        ..PublishManySummary::default()
    };
    let mut failures = Vec::new();

    for task_id in selected_task_ids {
        let result = publish_task(
            resolved,
            &mut queue_file,
            task_id,
            PublishMode::Execute,
            &args.label,
            &args.assignee,
            args.repo.as_deref(),
        )
        .unwrap_or_else(PublishItemResult::Failed);

        if let PublishItemResult::Failed(err) = &result {
            failures.push((task_id.clone(), err.to_string()));
        }

        print_publish_many_task_result(task_id, &result);
        accumulate_publish_result(&mut final_summary, &result);
    }

    if final_summary.has_mutations() {
        crate::queue::validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
        crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
    }

    print_publish_many_summary(&final_summary, false);
    if failures.is_empty() {
        return Ok(());
    }

    print_failures(&failures);
    bail!(
        "publish-many completed with {} failed task(s).",
        final_summary.failed
    );
}
