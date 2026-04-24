//! Structured task mutation handler for `ralph task mutate`.
//!
//! Purpose:
//! - Structured task mutation handler for `ralph task mutate`.
//!
//! Responsibilities:
//! - Read a JSON task-mutation request from stdin or a file.
//! - Apply the request atomically through the shared queue transaction helper.
//! - Persist queue changes and print continuation-first mutation output from the shared machine document.
//!
//! Not handled here:
//! - Legacy field-by-field edit UX.
//! - Terminal archive moves across queue/done files.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Input JSON matches `TaskMutationRequest`.
//! - Queue mutations are lock-protected and use the shared queue operation layer.

use crate::cli::task::args::{TaskMutateArgs, TaskMutateFormatArg};
use crate::config;
use crate::contracts::MachineTaskMutationDocument;
use crate::queue;
use crate::queue::operations::{
    TaskMutationReport, TaskMutationRequest, apply_task_mutation_request,
};
use crate::timeutil;
use anyhow::{Context, Result, bail};
use std::fs;
use std::io::Read;

pub fn handle(args: &TaskMutateArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let raw = read_request(args).context("read task mutation request")?;
    let request = serde_json::from_str::<TaskMutationRequest>(&raw)
        .context("parse task mutation request json")?;

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task mutate", force)?;

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let now = timeutil::now_utc_rfc3339()?;

    let mut working = queue_file.clone();
    let report = apply_task_mutation_request(
        &mut working,
        done_ref,
        &request,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        resolved.config.queue.max_dependency_depth.unwrap_or(10),
    )?;

    if !args.dry_run {
        crate::undo::create_undo_snapshot(
            resolved,
            &format!("task mutate [{} task(s)]", report.tasks.len()),
        )?;
        queue::save_queue(&resolved.queue_path, &working)?;
    }

    let document = crate::cli::machine::build_task_mutation_document(&report, args.dry_run)?;
    print_report(&report, &document, args.format)?;
    Ok(())
}

fn read_request(args: &TaskMutateArgs) -> Result<String> {
    if let Some(path) = args.input.as_deref() {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            bail!("--input must be a non-empty path");
        }
        return fs::read_to_string(trimmed)
            .with_context(|| format!("read task mutation request from {}", trimmed));
    }

    let mut stdin = std::io::stdin().lock();
    let mut raw = String::new();
    stdin
        .read_to_string(&mut raw)
        .context("read task mutation request from stdin")?;
    if raw.trim().is_empty() {
        bail!("Task mutation request is empty. Pass --input or pipe JSON on stdin.");
    }
    Ok(raw)
}

fn print_report(
    report: &TaskMutationReport,
    document: &MachineTaskMutationDocument,
    format: TaskMutateFormatArg,
) -> Result<()> {
    match format {
        TaskMutateFormatArg::Text => {
            println!("{}", document.continuation.headline);
            println!("{}", document.continuation.detail);

            if let Some(blocking) = document
                .blocking
                .as_ref()
                .or(document.continuation.blocking.as_ref())
            {
                println!();
                println!(
                    "Operator state: {}",
                    format!("{:?}", blocking.status).to_lowercase()
                );
                println!("{}", blocking.message);
                if !blocking.detail.is_empty() {
                    println!("{}", blocking.detail);
                }
            }

            println!();
            println!("Applied edits:");
            for task in &report.tasks {
                println!("  - {}: {} edit(s)", task.task_id, task.applied_edits);
            }

            if !document.continuation.next_steps.is_empty() {
                println!();
                println!("Next:");
                for (index, step) in document.continuation.next_steps.iter().enumerate() {
                    println!("  {}. {} — {}", index + 1, step.command, step.detail);
                }
            }
        }
        TaskMutateFormatArg::Json => {
            println!("{}", serde_json::to_string_pretty(document)?);
        }
    }
    Ok(())
}
