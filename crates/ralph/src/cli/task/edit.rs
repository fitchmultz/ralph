//! Task editing command handlers for `ralph task` subcommands.
//!
//! Responsibilities:
//! - Handle `field` command (set custom fields).
//! - Handle `edit` command (edit any task field).
//! - Handle `update` command (AI-powered field updates from repo state).
//!
//! Not handled here:
//! - Batch edit operations (see `batch.rs`).
//! - Task building or status changes (see `build.rs`, `status.rs`).
//!
//! Invariants/assumptions:
//! - Edit operations validate field values before applying.
//! - Dry-run mode previews changes without saving.
//! - Update command uses AI runner to analyze repository state.

use anyhow::{Result, bail};

use crate::agent;
use crate::cli::task::args::{TaskEditArgs, TaskFieldArgs, TaskUpdateArgs};
use crate::commands::task as task_cmd;
use crate::config;
use crate::queue;
use crate::queue::operations::{
    TaskFieldEdit, TaskMutationRequest, TaskMutationSpec, apply_task_mutation_request,
};
use crate::timeutil;

/// Handle the `field` command (set custom fields).
pub fn handle_field(args: &TaskFieldArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    // Resolve task IDs from explicit list or tag filter
    let task_ids =
        queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

    if task_ids.is_empty() {
        bail!("No tasks specified. Provide task IDs or use --tag-filter.");
    }

    if args.dry_run {
        // Preview mode: show diff without saving
        println!("Dry run - would update {} tasks:", task_ids.len());
        for task_id in &task_ids {
            let preview =
                queue::operations::preview_set_field(&queue_file, task_id, &args.key, &args.value)?;
            println!("  {}:", preview.task_id);
            println!("    Field: {}", preview.key);
            println!(
                "    Old: {}",
                preview.old_value.as_deref().unwrap_or("(not set)")
            );
            println!("    New: {}", preview.new_value);
        }
        println!("\nDry run complete. No changes made.");
        return Ok(());
    }

    // Normal mode: acquire lock and apply
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task field", force)?;

    // Create undo snapshot before mutation
    let task_ids_preview = task_ids.join(", ");
    crate::undo::create_undo_snapshot(
        resolved,
        &format!(
            "task field {}={} [{}]",
            args.key, args.value, task_ids_preview
        ),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    let result = queue::operations::batch_set_field(
        &mut queue_file,
        &task_ids,
        &args.key,
        &args.value,
        &now,
        false, // continue_on_error - default to atomic for CLI
    )?;

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    queue::operations::print_batch_results(
        &result,
        &format!("Field set '{}' = '{}'", args.key, args.value),
        false,
    );

    Ok(())
}

/// Handle the `edit` command (edit any task field).
pub fn handle_edit(args: &TaskEditArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Resolve task IDs from explicit list or tag filter
    let task_ids =
        queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

    if task_ids.is_empty() {
        bail!("No tasks specified. Provide task IDs or use --tag-filter.");
    }

    if args.dry_run {
        // Preview mode: show diff without saving
        println!("Dry run - would update {} tasks:", task_ids.len());
        for task_id in &task_ids {
            let preview = queue::preview_task_edit(
                &queue_file,
                done_ref,
                task_id,
                args.field.into(),
                &args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )?;
            println!("  {}:", preview.task_id);
            println!("    Field: {}", preview.field);
            println!("    Old: {}", preview.old_value);
            println!("    New: {}", preview.new_value);
            if !preview.warnings.is_empty() {
                println!("    Warnings:");
                for warning in &preview.warnings {
                    println!("      - [{}] {}", warning.task_id, warning.message);
                }
            }
        }
        println!("\nDry run complete. No changes made.");
        return Ok(());
    }

    // Normal mode: acquire lock and apply
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task edit", force)?;

    // Create undo snapshot before mutation
    let task_ids_preview = task_ids.join(", ");
    crate::undo::create_undo_snapshot(
        resolved,
        &format!("task edit {} [{}]", args.field.as_str(), task_ids_preview),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;

    let request = TaskMutationRequest {
        version: 1,
        atomic: true,
        tasks: task_ids
            .iter()
            .map(|task_id| TaskMutationSpec {
                task_id: task_id.clone(),
                expected_updated_at: None,
                edits: vec![TaskFieldEdit {
                    field: args.field.as_str().to_string(),
                    value: args.value.clone(),
                }],
            })
            .collect(),
    };

    let result = apply_task_mutation_request(
        &mut queue_file,
        Some(&done_file),
        &request,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    // Run auto-archive sweep for terminal tasks if configured and not disabled
    let mut archived_task_ids: Vec<String> = Vec::new();
    if !args.no_auto_archive
        && let Some(days) = resolved.config.queue.auto_archive_terminal_after_days
    {
        match queue::maybe_archive_terminal_tasks_in_memory(
            &mut queue_file,
            &mut done_file,
            &now,
            Some(days),
        ) {
            Ok(report) => {
                archived_task_ids = report.moved_ids;
            }
            Err(e) => {
                log::warn!("Auto-archive sweep failed: {}", e);
            }
        }
    }

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    if !archived_task_ids.is_empty() {
        queue::save_queue(&resolved.done_path, &done_file)?;
    }

    println!(
        "Applied field '{}' to {} task(s).",
        args.field.as_str(),
        result.tasks.len()
    );

    if !archived_task_ids.is_empty() {
        // List specific archived task IDs
        println!(
            "Auto-archived {} terminal task(s):",
            archived_task_ids.len()
        );
        for task_id in &archived_task_ids {
            println!("  - {}", task_id);
        }
    }

    Ok(())
}

/// Handle the `update` command (AI-powered field updates).
pub fn handle_update(
    args: &TaskUpdateArgs,
    resolved: &config::Resolved,
    force: bool,
) -> Result<()> {
    let valid_fields = ["scope", "evidence", "plan", "notes", "tags", "depends_on"];
    let fields_to_update = if args.fields.trim().is_empty() || args.fields.trim() == "all" {
        "scope,evidence,plan,notes,tags,depends_on".to_string()
    } else {
        for field in args.fields.split(',') {
            if !valid_fields.contains(&field.trim()) {
                bail!(
                    "Invalid field '{}'. Valid fields: {}",
                    field,
                    valid_fields.join(", ")
                );
            }
        }
        args.fields.clone()
    };

    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    let update_settings = task_cmd::TaskUpdateSettings {
        fields: fields_to_update,
        runner_override: overrides.runner,
        model_override: overrides.model,
        reasoning_effort_override: overrides.reasoning_effort,
        runner_cli_overrides: overrides.runner_cli,
        force,
        repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, resolved),
        dry_run: args.dry_run,
    };

    match args.task_id.as_deref() {
        Some(task_id) => task_cmd::update_task(resolved, task_id, &update_settings),
        None => task_cmd::update_all_tasks(resolved, &update_settings),
    }
}
