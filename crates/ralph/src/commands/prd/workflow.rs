//! PRD workflow orchestration.
//!
//! Purpose:
//! - PRD workflow orchestration.
//!
//! Responsibilities:
//! - Read PRD files, parse them, generate tasks, and persist or preview results.
//! - Keep queue lock/load/save behavior separate from parsing and generation logic.
//!
//! Not handled here:
//! - CLI parsing.
//! - Low-level markdown parsing details.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Dry-runs never mutate queue state.
//! - Queue insertion respects doing-task-first ordering.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::contracts::{TaskPriority, TaskStatus};
use crate::{config, queue, timeutil};

use super::generate::{generate_multi_tasks, generate_single_task};
use super::parse::parse_prd;

pub struct CreateOptions {
    pub path: PathBuf,
    pub multi: bool,
    pub dry_run: bool,
    pub priority: Option<TaskPriority>,
    pub tags: Vec<String>,
    pub draft: bool,
}

pub fn create_from_prd(
    resolved: &config::Resolved,
    opts: &CreateOptions,
    force: bool,
) -> Result<()> {
    if !opts.path.exists() {
        bail!(
            "PRD file not found: {}. Check the path and try again.",
            opts.path.display()
        );
    }

    let content = std::fs::read_to_string(&opts.path)
        .with_context(|| format!("Failed to read PRD file: {}", opts.path.display()))?;
    if content.trim().is_empty() {
        bail!("PRD file is empty: {}", opts.path.display());
    }

    let parsed = parse_prd(&content);
    if parsed.title.is_empty() {
        bail!(
            "Could not extract title from PRD: {}. Ensure the file has a # Heading at the start.",
            opts.path.display()
        );
    }

    let _queue_lock = if !opts.dry_run {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "prd create",
            force,
        )?)
    } else {
        None
    };

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };

    let insert_index = queue::suggest_new_task_insert_index(&queue_file);
    let now = timeutil::now_utc_rfc3339()?;
    let priority = opts.priority.unwrap_or(TaskPriority::Medium);
    let status = if opts.draft {
        TaskStatus::Draft
    } else {
        TaskStatus::Todo
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    let tasks = if opts.multi {
        generate_multi_tasks(
            &parsed,
            &now,
            priority,
            status,
            &opts.tags,
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?
    } else {
        vec![generate_single_task(
            &parsed,
            &now,
            priority,
            status,
            &opts.tags,
            &queue_file,
            done_ref,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )?]
    };

    if tasks.is_empty() {
        bail!(
            "No tasks generated from PRD: {}. Check the file format.",
            opts.path.display()
        );
    }

    if opts.dry_run {
        print_preview(&tasks);
        return Ok(());
    }

    let new_task_ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    for task in tasks {
        queue_file.tasks.insert(insert_index, task);
    }
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    println!("Created {} task(s) from PRD:", new_task_ids.len());
    for id in &new_task_ids {
        println!("  {}", id);
    }
    Ok(())
}

fn print_preview(tasks: &[crate::contracts::Task]) {
    println!("Dry run - would create {} task(s):", tasks.len());
    for task in tasks {
        println!("\n  ID: {}", task.id);
        println!("  Title: {}", task.title);
        println!("  Priority: {}", task.priority);
        println!("  Status: {}", task.status);
        if !task.tags.is_empty() {
            println!("  Tags: {}", task.tags.join(", "));
        }
        if let Some(request) = &task.request {
            println!("  Request: {}", request.lines().next().unwrap_or(request));
        }
    }
}
