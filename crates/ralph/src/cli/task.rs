//! `ralph task ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap structures for task-related commands.
//! - Route task subcommands to queue operations and task building.
//!
//! Not handled here:
//! - Queue persistence details (see `crate::queue`).
//! - Locking semantics (see `crate::lock`).
//! - Runner execution internals.
//!
//! Invariants/assumptions:
//! - Callers resolve configuration before executing commands.
//! - Queue mutations are protected by locks when required.

use std::io::IsTerminal;

use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};

use crate::cli::queue::{show_task, QueueShowFormat};
use crate::contracts::TaskStatus;
use crate::queue::TaskEditKey;
use crate::{agent, commands::task as task_cmd, completions, config, lock, queue, timeutil};

pub fn handle_task(args: TaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        Some(TaskCommand::Ready(args)) => {
            let _queue_lock =
                crate::queue::acquire_queue_lock(&resolved.repo_root, "task ready", force)?;
            let mut queue_file = crate::queue::load_queue(&resolved.queue_path)?;
            let now = crate::timeutil::now_utc_rfc3339()?;
            crate::queue::promote_draft_to_todo(
                &mut queue_file,
                &args.task_id,
                &now,
                args.note.as_deref(),
            )?;
            crate::queue::save_queue(&resolved.queue_path, &queue_file)?;
            log::info!("Task {} marked ready (draft -> todo).", args.task_id);
            Ok(())
        }

        Some(TaskCommand::Status(args)) => {
            let status: TaskStatus = args.status.into();

            match status {
                TaskStatus::Done | TaskStatus::Rejected => {
                    // For terminal statuses, we need to handle each task individually
                    // because complete_task involves moving tasks to done.json
                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;
                    let queue_file = queue::load_queue(&resolved.queue_path)?;
                    let now = timeutil::now_utc_rfc3339()?;
                    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

                    // Resolve task IDs from explicit list or tag filter
                    let task_ids = queue::operations::resolve_task_ids(
                        &queue_file,
                        &args.task_ids,
                        &args.tag_filter,
                    )?;

                    if task_ids.is_empty() {
                        bail!("No tasks specified. Provide task IDs or use --tag-filter.");
                    }

                    let mut results = Vec::new();
                    let mut succeeded = 0;
                    let mut failed = 0;

                    for task_id in &task_ids {
                        match queue::complete_task(
                            &resolved.queue_path,
                            &resolved.done_path,
                            task_id,
                            status,
                            &now,
                            args.note
                                .as_deref()
                                .map(|n| vec![n.to_string()])
                                .as_deref()
                                .unwrap_or(&[]),
                            &resolved.id_prefix,
                            resolved.id_width,
                            max_depth,
                        ) {
                            Ok(()) => {
                                results.push((task_id.clone(), true, None));
                                succeeded += 1;
                            }
                            Err(e) => {
                                results.push((task_id.clone(), false, Some(e.to_string())));
                                failed += 1;
                            }
                        }
                    }

                    // Print results
                    if failed > 0 {
                        println!("Completed with errors:");
                        for (task_id, success, error) in &results {
                            if *success {
                                println!("  ✓ {}: {} and archived", task_id, status);
                            } else {
                                println!(
                                    "  ✗ {}: failed - {}",
                                    task_id,
                                    error.as_deref().unwrap_or("unknown error")
                                );
                            }
                        }
                        println!(
                            "Completed: {}/{} tasks {} successfully.",
                            succeeded,
                            task_ids.len(),
                            status
                        );
                    } else {
                        println!("Successfully marked {} tasks as {}:", succeeded, status);
                        for (task_id, _, _) in &results {
                            println!("  ✓ {}", task_id);
                        }
                    }

                    Ok(())
                }
                TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;
                    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
                    let now = timeutil::now_utc_rfc3339()?;

                    // Resolve task IDs from explicit list or tag filter
                    let task_ids = queue::operations::resolve_task_ids(
                        &queue_file,
                        &args.task_ids,
                        &args.tag_filter,
                    )?;

                    if task_ids.is_empty() {
                        bail!("No tasks specified. Provide task IDs or use --tag-filter.");
                    }

                    let result = queue::operations::batch_set_status(
                        &mut queue_file,
                        &task_ids,
                        status,
                        &now,
                        args.note.as_deref(),
                        false, // continue_on_error - default to atomic for CLI
                    )?;

                    queue::save_queue(&resolved.queue_path, &queue_file)?;
                    queue::operations::print_batch_results(
                        &result,
                        &format!("Status update to {}", status),
                        false,
                    );

                    Ok(())
                }
            }
        }

        Some(TaskCommand::Done(args)) => complete_task_or_signal(
            &resolved,
            &args.task_id,
            TaskStatus::Done,
            &args.note,
            force,
            "task done",
        ),

        Some(TaskCommand::Reject(args)) => complete_task_or_signal(
            &resolved,
            &args.task_id,
            TaskStatus::Rejected,
            &args.note,
            force,
            "task reject",
        ),

        Some(TaskCommand::Field(args)) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task field", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;

            // Resolve task IDs from explicit list or tag filter
            let task_ids =
                queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

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

        Some(TaskCommand::Edit(args)) => {
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
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_apply_edit(
                &mut queue_file,
                done_ref,
                &task_ids,
                args.field.into(),
                &args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
                false, // continue_on_error - default to atomic for CLI
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Edit field '{}'", args.field.as_str()),
                false,
            );

            Ok(())
        }

        Some(TaskCommand::Update(args)) => {
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
                repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, &resolved),
                dry_run: args.dry_run,
            };

            match args.task_id.as_deref() {
                Some(task_id) => task_cmd::update_task(&resolved, task_id, &update_settings),
                None => task_cmd::update_all_tasks(&resolved, &update_settings),
            }
        }

        Some(TaskCommand::Build(args)) => {
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.repo_prompt,
                        &resolved,
                    ),
                    template_hint: args.template,
                    template_target: args.target,
                },
            )
        }

        Some(TaskCommand::Template(template_args)) => {
            handle_template_command(&resolved, template_args)
        }

        Some(TaskCommand::BuildRefactor(args)) | Some(TaskCommand::Refactor(args)) => {
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            task_cmd::build_refactor_tasks(
                &resolved,
                task_cmd::TaskBuildRefactorOptions {
                    threshold: args.threshold,
                    path: args.path,
                    dry_run: args.dry_run,
                    batch: args.batch.into(),
                    extra_tags: args.tags.unwrap_or_default(),
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.repo_prompt,
                        &resolved,
                    ),
                },
            )
        }

        Some(TaskCommand::Show(args)) => show_task(&resolved, &args.task_id, args.format),

        Some(TaskCommand::Clone(args)) => {
            let status: TaskStatus = args.status.unwrap_or(TaskStatusArg::Draft).into();

            // Load both queue and done files
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done_file)
            };

            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            // Build clone options
            let clone_opts = queue::operations::CloneTaskOptions::new(
                &args.task_id,
                status,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
            )
            .with_title_prefix(args.title_prefix.as_deref())
            .with_max_depth(max_depth);

            // Perform the clone operation
            let (new_id, cloned_task) = queue::operations::clone_task(
                &mut queue_file.clone(), // Clone for dry run check
                done_ref,
                &clone_opts,
            )?;

            if args.dry_run {
                println!(
                    "Dry run - would clone task {} to new task {}:",
                    args.task_id, new_id
                );
                println!("  Title: {}", cloned_task.title);
                println!("  Status: {}", cloned_task.status);
                println!("  Priority: {}", cloned_task.priority);
                if !cloned_task.tags.is_empty() {
                    println!("  Tags: {}", cloned_task.tags.join(", "));
                }
                if !cloned_task.scope.is_empty() {
                    println!("  Scope: {}", cloned_task.scope.join(", "));
                }
                return Ok(());
            }

            // Acquire lock and perform actual clone
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task clone", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let (new_id, cloned_task) =
                queue::operations::clone_task(&mut queue_file, done_ref, &clone_opts)?;

            // Insert at appropriate position
            let insert_at = queue::operations::suggest_new_task_insert_index(&queue_file);
            queue_file.tasks.insert(insert_at, cloned_task);

            // Save queue
            queue::save_queue(&resolved.queue_path, &queue_file)?;

            log::info!(
                "Cloned task {} to new task {} (status: {})",
                args.task_id,
                new_id,
                status
            );
            println!("Created new task {} from clone of {}", new_id, args.task_id);

            Ok(())
        }

        Some(TaskCommand::Batch(args)) => handle_batch_command(&resolved, args, force),

        Some(TaskCommand::Schedule(args)) => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "task schedule", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            // Handle clear operation
            let value = if args.clear {
                String::new()
            } else if let Some(when) = args.when {
                // Parse relative time or RFC3339
                timeutil::parse_relative_time(&when)?
            } else {
                bail!("Either provide a timestamp/expression or use --clear to remove scheduling.");
            };

            queue::apply_task_edit(
                &mut queue_file,
                None,
                &args.task_id,
                TaskEditKey::ScheduledStart,
                &value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;

            if args.clear {
                log::info!("Task {} scheduling cleared.", args.task_id);
                println!("Task {} scheduling cleared.", args.task_id);
            } else {
                log::info!("Task {} scheduled for {}.", args.task_id, value);
                println!("Task {} scheduled for {}.", args.task_id, value);
            }

            Ok(())
        }

        Some(TaskCommand::Relate(args)) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task relate", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            let relation = args.relation.trim().to_lowercase();
            let edit_key = match relation.as_str() {
                "blocks" => TaskEditKey::Blocks,
                "relates_to" | "relates" => TaskEditKey::RelatesTo,
                "duplicates" | "duplicate" => TaskEditKey::Duplicates,
                _ => bail!(
                    "Invalid relationship type: '{}'. Expected one of: blocks, relates_to, duplicates.",
                    args.relation
                ),
            };

            // For blocks and relates_to, append to the list
            // For duplicates, set the value directly
            let value = if matches!(edit_key, TaskEditKey::Duplicates) {
                args.other_task_id.clone()
            } else {
                // Get existing values and append
                let task = queue_file
                    .tasks
                    .iter()
                    .find(|t| t.id.trim() == args.task_id.trim())
                    .ok_or_else(|| anyhow::anyhow!("Task not found: {}", args.task_id))?;

                let existing: Vec<String> = match edit_key {
                    TaskEditKey::Blocks => task.blocks.clone(),
                    TaskEditKey::RelatesTo => task.relates_to.clone(),
                    _ => vec![],
                };

                let mut new_list = existing;
                if !new_list.contains(&args.other_task_id) {
                    new_list.push(args.other_task_id.clone());
                }
                new_list.join(", ")
            };

            queue::apply_task_edit(
                &mut queue_file,
                None,
                &args.task_id,
                edit_key,
                &value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;

            let relation_desc = match edit_key {
                TaskEditKey::Blocks => "blocks",
                TaskEditKey::RelatesTo => "relates to",
                TaskEditKey::Duplicates => "duplicates",
                _ => &args.relation,
            };

            log::info!(
                "Task {} now {} {}.",
                args.task_id,
                relation_desc,
                args.other_task_id
            );
            println!(
                "Task {} now {} {}.",
                args.task_id, relation_desc, args.other_task_id
            );

            Ok(())
        }

        Some(TaskCommand::Blocks(args)) => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task blocks", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            // Get existing blocks
            let task = queue_file
                .tasks
                .iter()
                .find(|t| t.id.trim() == args.task_id.trim())
                .ok_or_else(|| anyhow::anyhow!("Task not found: {}", args.task_id))?;

            let mut new_blocks = task.blocks.clone();
            for blocked_id in &args.blocked_task_ids {
                if !new_blocks.contains(blocked_id) {
                    new_blocks.push(blocked_id.clone());
                }
            }

            let value = new_blocks.join(", ");

            queue::apply_task_edit(
                &mut queue_file,
                None,
                &args.task_id,
                TaskEditKey::Blocks,
                &value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;

            log::info!(
                "Task {} now blocks: {}.",
                args.task_id,
                args.blocked_task_ids.join(", ")
            );
            println!(
                "Task {} now blocks: {}.",
                args.task_id,
                args.blocked_task_ids.join(", ")
            );

            Ok(())
        }

        Some(TaskCommand::MarkDuplicate(args)) => {
            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "task duplicate", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            queue::apply_task_edit(
                &mut queue_file,
                None,
                &args.task_id,
                TaskEditKey::Duplicates,
                &args.original_task_id,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;

            log::info!(
                "Task {} marked as duplicate of {}.",
                args.task_id,
                args.original_task_id
            );
            println!(
                "Task {} marked as duplicate of {}.",
                args.task_id, args.original_task_id
            );

            Ok(())
        }

        None => {
            let args = args.build;
            let request = task_cmd::read_request_from_args_or_stdin(&args.request)?;

            // Interactive template selection if no template specified and running in TTY
            let (template_hint, template_target) =
                if args.template.is_none() && std::io::stdin().is_terminal() {
                    match prompt_template_selection(&resolved.repo_root)? {
                        Some((name, target)) => (Some(name), target),
                        None => (None, args.target),
                    }
                } else {
                    (args.template, args.target)
                };

            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: args.runner.clone(),
                model: args.model.clone(),
                effort: args.effort.clone(),
                repo_prompt: args.repo_prompt,
                runner_cli: args.runner_cli.clone(),
            })?;

            task_cmd::build_task(
                &resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: args.tags,
                    hint_scope: args.scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.repo_prompt,
                        &resolved,
                    ),
                    template_hint,
                    template_target,
                },
            )
        }
    }
}

/// Prompt user to select a template interactively
///
/// Returns Some((template_name, target_path)) if a template was selected,
/// None if the user chose to skip.
fn prompt_template_selection(
    repo_root: &std::path::Path,
) -> Result<Option<(String, Option<String>)>> {
    use std::io::Write;

    let templates = crate::template::list_templates(repo_root);

    println!("\nAvailable templates:");
    println!();
    for (i, template) in templates.iter().enumerate() {
        let source_label = match template.source {
            crate::template::TemplateSource::Custom(_) => "(custom)",
            crate::template::TemplateSource::Builtin(_) => "(built-in)",
        };
        println!(
            "  {}. {:12} {:10} {}",
            i + 1,
            template.name,
            source_label,
            template.description
        );
    }
    println!();
    println!("Enter number to select a template, or press Enter to skip:");
    print!("> ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        return Ok(None);
    }

    // Parse selection
    match input.parse::<usize>() {
        Ok(num) if num > 0 && num <= templates.len() => {
            let selected = &templates[num - 1];
            let template_name = selected.name.clone();

            // Ask for target if template supports variables
            let needs_target = matches!(
                template_name.as_str(),
                "add-tests"
                    | "refactor-performance"
                    | "fix-error-handling"
                    | "add-docs"
                    | "security-audit"
            );

            if needs_target {
                println!();
                println!("Enter target file/path for template variables (or press Enter to skip):");
                print!("> ");
                std::io::stdout().flush()?;

                let mut target_input = String::new();
                std::io::stdin().read_line(&mut target_input)?;
                let target = target_input.trim();

                if target.is_empty() {
                    Ok(Some((template_name, None)))
                } else {
                    Ok(Some((template_name, Some(target.to_string()))))
                }
            } else {
                Ok(Some((template_name, None)))
            }
        }
        _ => {
            println!("Invalid selection, skipping template.");
            Ok(None)
        }
    }
}

fn complete_task_or_signal(
    resolved: &config::Resolved,
    task_id: &str,
    status: TaskStatus,
    notes: &[String],
    force: bool,
    _lock_label: &str,
) -> Result<()> {
    let lock_dir = lock::queue_lock_dir(&resolved.repo_root);
    // Only use completion signal mode if the current process is actually being supervised
    // (i.e., running as a descendant of the supervisor process). This distinguishes between:
    // - An agent running inside a supervised session (should use completion signals)
    // - A user manually running commands while a supervisor is active (should complete directly)
    if lock::is_current_process_supervised(&lock_dir)? {
        let signal = completions::CompletionSignal {
            task_id: task_id.to_string(),
            status,
            notes: notes.to_vec(),
        };
        let path = completions::write_completion_signal(&resolved.repo_root, &signal)?;
        log::info!(
            "Running under supervision - wrote completion signal at {}",
            path.display()
        );
        return Ok(());
    }

    // Use "task" label to enable shared lock mode, allowing this command to work
    // concurrently with a supervising process (like `ralph run loop`).
    // This matches the behavior of `ralph task build`.
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task", force)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        notes,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    log::info!(
        "Task {} completed (status: {}) and moved to done archive.",
        task_id,
        status
    );
    Ok(())
}

/// Handle batch task operations.
fn handle_batch_command(
    resolved: &config::Resolved,
    args: TaskBatchArgs,
    force: bool,
) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    match args.operation {
        BatchOperation::Status(status_args) => {
            let status: TaskStatus = status_args.status.into();

            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &status_args.task_ids,
                &status_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            // For terminal statuses, use complete_task for each task
            match status {
                TaskStatus::Done | TaskStatus::Rejected => {
                    if args.dry_run {
                        println!(
                            "Dry run - would mark {} tasks as {} and archive them:",
                            task_ids.len(),
                            status
                        );
                        for task_id in &task_ids {
                            println!("  - {}", task_id);
                        }
                        println!("\nDry run complete. No changes made.");
                        return Ok(());
                    }

                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
                    let mut results = Vec::new();
                    let mut succeeded = 0;
                    let mut failed = 0;

                    for task_id in &task_ids {
                        match queue::complete_task(
                            &resolved.queue_path,
                            &resolved.done_path,
                            task_id,
                            status,
                            &now,
                            status_args
                                .note
                                .as_deref()
                                .map(|n| vec![n.to_string()])
                                .as_deref()
                                .unwrap_or(&[]),
                            &resolved.id_prefix,
                            resolved.id_width,
                            max_depth,
                        ) {
                            Ok(()) => {
                                results.push((task_id.clone(), true, None));
                                succeeded += 1;
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                results.push((task_id.clone(), false, Some(err_msg.clone())));
                                failed += 1;

                                if !args.continue_on_error {
                                    bail!(
                                        "Batch operation failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                                        task_id,
                                        err_msg
                                    );
                                }
                            }
                        }
                    }

                    // Print results
                    if failed > 0 {
                        println!("Completed with errors:");
                        for (task_id, success, error) in &results {
                            if *success {
                                println!("  ✓ {}: {} and archived", task_id, status);
                            } else {
                                println!(
                                    "  ✗ {}: failed - {}",
                                    task_id,
                                    error.as_deref().unwrap_or("unknown error")
                                );
                            }
                        }
                        println!(
                            "Completed: {}/{} tasks {} successfully.",
                            succeeded,
                            task_ids.len(),
                            status
                        );
                    } else {
                        println!("Successfully marked {} tasks as {}:", succeeded, status);
                        for (task_id, _, _) in &results {
                            println!("  ✓ {}", task_id);
                        }
                    }

                    Ok(())
                }
                TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
                    if args.dry_run {
                        println!(
                            "Dry run - would update {} tasks to status '{}':",
                            task_ids.len(),
                            status
                        );
                        for task_id in &task_ids {
                            println!("  - {}", task_id);
                        }
                        println!("\nDry run complete. No changes made.");
                        return Ok(());
                    }

                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
                    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

                    let result = queue::operations::batch_set_status(
                        &mut queue_file,
                        &task_ids,
                        status,
                        &now,
                        status_args.note.as_deref(),
                        args.continue_on_error,
                    )?;

                    queue::save_queue(&resolved.queue_path, &queue_file)?;
                    queue::operations::print_batch_results(
                        &result,
                        &format!("Status update to {}", status),
                        false,
                    );

                    Ok(())
                }
            }
        }
        BatchOperation::Field(field_args) => {
            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &field_args.task_ids,
                &field_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            if args.dry_run {
                println!(
                    "Dry run - would set field '{}' = '{}' on {} tasks:",
                    field_args.key,
                    field_args.value,
                    task_ids.len()
                );
                for task_id in &task_ids {
                    println!("  - {}", task_id);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_set_field(
                &mut queue_file,
                &task_ids,
                &field_args.key,
                &field_args.value,
                &now,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Field set '{}' = '{}'", field_args.key, field_args.value),
                false,
            );

            Ok(())
        }
        BatchOperation::Edit(edit_args) => {
            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &edit_args.task_ids,
                &edit_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            if args.dry_run {
                println!(
                    "Dry run - would edit field '{}' to '{}' on {} tasks:",
                    edit_args.field.as_str(),
                    edit_args.value,
                    task_ids.len()
                );
                for task_id in &task_ids {
                    let preview = queue::preview_task_edit(
                        &queue_file,
                        done_ref,
                        task_id,
                        edit_args.field.into(),
                        &edit_args.value,
                        &now,
                        &resolved.id_prefix,
                        resolved.id_width,
                        max_depth,
                    )?;
                    println!("  {}:", preview.task_id);
                    println!("    Old: {}", preview.old_value);
                    println!("    New: {}", preview.new_value);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_apply_edit(
                &mut queue_file,
                done_ref,
                &task_ids,
                edit_args.field.into(),
                &edit_args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Edit field '{}'", edit_args.field.as_str()),
                false,
            );

            Ok(())
        }
    }
}

#[derive(Args)]
#[command(
    about = "Create and build tasks from freeform requests",
    subcommand_required = false,
    after_long_help = "Examples:\n ralph task \"Add tests for the new queue logic\"\n ralph task --runner opencode --model gpt-5.2 \"Fix CLI help strings\"\n ralph task --runner kimi --model kimi-for-coding \"Add tests for X\"\n ralph task --runner pi --model gpt-5.2 \"Add tests for X\"\n ralph task --template add-tests src/cli/task.rs \"Add unit tests for task module\"\n ralph task --template refactor-performance src/bottleneck.rs \"Optimize hot path\"\n ralph task --template fix-error-handling src/api.rs \"Fix error handling\"\n ralph task template list\n ralph task template show add-tests\n ralph task template build add-tests src/module.rs \"Add tests\"\n ralph task show RQ-0001\n ralph task show RQ-0001 --format compact\n ralph task ready RQ-0005\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task update\n ralph task update RQ-0001\n ralph task update --fields scope,evidence RQ-0001\n ralph task edit title \"Refine queue edit\" RQ-0001\n ralph task field severity high RQ-0003\n ralph task done --note \"Finished work\" RQ-0001\n ralph task reject --note \"No longer needed\" RQ-0002\n ralph task clone RQ-0001\n ralph task clone RQ-0001 --status todo\n ralph task clone RQ-0001 --title-prefix \"[Follow-up] \"\n ralph task duplicate RQ-0001 --dry-run\n ralph task build \"(explicit build subcommand still works)\""
)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub command: Option<TaskCommand>,

    #[command(flatten)]
    pub build: TaskBuildArgs,
}

#[derive(Subcommand)]
pub enum TaskCommand {
    /// Build a new task from a natural language request.
    #[command(
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nExamples:\n ralph task \"Add integration tests for run one\"\n ralph task --tags cli,rust \"Refactor queue parsing\"\n ralph task --scope crates/ralph \"Fix TUI rendering bug\"\n ralph task --runner opencode --model gpt-5.2 \"Add docs for OpenCode setup\"\n ralph task --runner gemini --model gemini-3-flash-preview \"Draft risk checklist\"\n ralph task --runner pi --model gpt-5.2 \"Draft risk checklist\"\n ralph task --runner codex --model gpt-5.2-codex --effort high \"Fix queue validation\"\n ralph task --approval-mode auto-edits --runner claude \"Audit approvals\"\n ralph task --sandbox disabled --runner codex \"Audit sandbox\"\n ralph task --repo-prompt plan \"Audit error handling\"\n ralph task --repo-prompt off \"Quick typo fix\"\n echo \"Triage flaky CI\" | ralph task --runner codex --model gpt-5.2-codex --effort medium\n\nExplicit subcommand:\n ralph task build \"Add integration tests for run one\""
    )]
    Build(TaskBuildArgs),

    /// Automatically create refactoring tasks for large files.
    #[command(
        alias = "ref",
        after_long_help = "Examples:\n ralph task refactor\n ralph task refactor --threshold 700\n ralph task refactor --path crates/ralph/src/cli\n ralph task refactor --dry-run --threshold 500\n ralph task refactor --batch never\n ralph task refactor --tags urgent,technical-debt\n ralph task ref --threshold 800"
    )]
    Refactor(TaskBuildRefactorArgs),

    /// Automatically create refactoring tasks for large files (alternative to 'refactor').
    #[command(
        name = "build-refactor",
        after_long_help = "Alternative command name. Prefer 'ralph task refactor'.\n\nExamples:\n ralph task build-refactor\n ralph task build-refactor --threshold 700"
    )]
    BuildRefactor(TaskBuildRefactorArgs),

    /// Show a task by ID (queue + done).
    #[command(
        alias = "details",
        after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task details RQ-0001 --format compact"
    )]
    Show(TaskShowArgs),

    /// Promote a draft task to todo.
    #[command(
        after_long_help = "Examples:\n ralph task ready RQ-0005\n ralph task ready --note \"Ready for implementation\" RQ-0005"
    )]
    Ready(TaskReadyArgs),

    /// Update a task's status (draft, todo, doing, done, rejected).
    ///
    /// Note: terminal statuses (done, rejected) complete and archive the task.
    #[command(
        after_long_help = "Examples:\n ralph task status doing RQ-0001\n ralph task status doing --note \"Starting work\" RQ-0001\n ralph task status todo --note \"Back to backlog\" RQ-0001\n ralph task status done RQ-0001\n ralph task status rejected --note \"Invalid request\" RQ-0002"
    )]
    Status(TaskStatusArgs),

    /// Complete a task as done and move it to the done archive.
    #[command(
        after_long_help = "Examples:\n ralph task done RQ-0001\n ralph task done --note \"Finished work\" --note \"make ci green\" RQ-0001"
    )]
    Done(TaskDoneArgs),

    /// Complete a task as rejected and move it to the done archive.
    #[command(
        alias = "rejected",
        after_long_help = "Examples:\n ralph task reject RQ-0002\n ralph task reject --note \"No longer needed\" RQ-0002"
    )]
    Reject(TaskRejectArgs),

    /// Set a custom field on a task.
    #[command(
        after_long_help = "Examples:\n ralph task field severity high RQ-0001\n ralph task field complexity \"O(n log n)\" RQ-0002"
    )]
    Field(TaskFieldArgs),

    /// Edit any task field (default or custom).
    #[command(
        after_long_help = "Examples:\n ralph task edit title \"Clarify CLI edit\" RQ-0001\n ralph task edit status doing RQ-0001\n ralph task edit priority high RQ-0001\n ralph task edit tags \"cli, rust\" RQ-0001\n ralph task edit custom_fields \"severity=high, owner=ralph\" RQ-0001\n ralph task edit request \"\" RQ-0001\n ralph task edit completed_at \"2026-01-20T12:00:00Z\" RQ-0001\n ralph task edit --dry-run title \"Preview change\" RQ-0001"
    )]
    Edit(TaskEditArgs),

    /// Update existing task fields based on current repository state.
    #[command(
        after_long_help = "Runner selection:\n - Override runner/model/effort for this invocation using flags.\n - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n - Override approval/sandbox/verbosity/plan-mode via flags.\n - Unsupported options follow --unsupported-option-policy.\n\nField selection:\n - By default, all updatable fields are refreshed: scope, evidence, plan, notes, tags, depends_on.\n - Use --fields to specify which fields to update.\n\nTask selection:\n - Omit TASK_ID to update every task in the active queue.\n\nExamples:\n ralph task update\n ralph task update RQ-0001\n ralph task update --fields scope,evidence,plan RQ-0001\n ralph task update --runner opencode --model gpt-5.2 RQ-0001\n ralph task update --approval-mode auto-edits --runner claude RQ-0001\n ralph task update --repo-prompt plan RQ-0001\n ralph task update --repo-prompt off --fields scope,evidence RQ-0001\n ralph task update --fields tags RQ-0042\n ralph task update --dry-run RQ-0001"
    )]
    Update(TaskUpdateArgs),

    /// Manage task templates for common task types.
    #[command(
        after_long_help = "Examples:\n ralph task template list\n ralph task template show bug\n ralph task template show add-tests\n ralph task template build bug \"Fix login timeout\"\n ralph task template build add-tests src/module.rs \"Add tests for module\"\n ralph task template build refactor-performance src/bottleneck.rs \"Optimize performance\"\n\nAvailable templates:\n - bug: Bug fix with reproduction steps and regression tests\n - feature: New feature with design, implementation, and documentation\n - refactor: Code refactoring with behavior preservation\n - test: Test addition or improvement\n - docs: Documentation update or creation\n - add-tests: Add tests for existing code with coverage verification\n - refactor-performance: Optimize performance with profiling and benchmarking\n - fix-error-handling: Fix error handling with proper types and context\n - add-docs: Add documentation for a specific file or module\n - security-audit: Security audit with vulnerability checks"
    )]
    Template(TaskTemplateArgs),

    /// Clone an existing task to create a new task from it.
    #[command(
        alias = "duplicate",
        after_long_help = "Examples:\n ralph task clone RQ-0001\n ralph task clone RQ-0001 --status todo\n ralph task clone RQ-0001 --title-prefix \"[Follow-up] \"\n ralph task clone RQ-0001 --dry-run\n ralph task duplicate RQ-0001"
    )]
    Clone(TaskCloneArgs),

    /// Perform batch operations on multiple tasks efficiently.
    #[command(
        after_long_help = "Examples:\n ralph task batch status doing RQ-0001 RQ-0002 RQ-0003\n ralph task batch status done --tag-filter ready\n ralph task batch field priority high --tag-filter urgent\n ralph task batch edit tags \"reviewed\" --tag-filter rust\n ralph task batch --dry-run status doing --tag-filter cli\n ralph task batch --continue-on-error status doing RQ-0001 RQ-0002 RQ-9999"
    )]
    Batch(TaskBatchArgs),

    /// Schedule a task to start after a specific time.
    #[command(
        after_long_help = "Examples:\n ralph task schedule RQ-0001 '2026-02-01T09:00:00Z'\n ralph task schedule RQ-0001 'tomorrow 9am'\n ralph task schedule RQ-0001 'in 2 hours'\n ralph task schedule RQ-0001 'next monday'\n ralph task schedule RQ-0001 --clear"
    )]
    Schedule(TaskScheduleArgs),

    /// Add a relationship between tasks.
    #[command(
        after_long_help = "Examples:\n ralph task relate RQ-0001 blocks RQ-0002\n ralph task relate RQ-0001 relates_to RQ-0003\n ralph task relate RQ-0001 duplicates RQ-0004"
    )]
    Relate(TaskRelateArgs),

    /// Mark a task as blocking another task (shorthand for 'relate <task> blocks <blocked>').
    #[command(
        after_long_help = "Examples:\n ralph task blocks RQ-0001 RQ-0002\n ralph task blocks RQ-0001 RQ-0002 RQ-0003"
    )]
    Blocks(TaskBlocksArgs),

    /// Mark a task as a duplicate of another task (shorthand for 'relate <task> duplicates <original>').
    #[command(
        name = "mark-duplicate",
        after_long_help = "Examples:\n ralph task mark-duplicate RQ-0001 RQ-0002"
    )]
    MarkDuplicate(TaskMarkDuplicateArgs),
}

#[derive(Args)]
pub struct TaskBuildArgs {
    /// Freeform request text; if omitted, reads from stdin.
    #[arg(value_name = "REQUEST")]
    pub request: Vec<String>,

    /// Optional hint tags (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub tags: String,

    /// Optional hint scope (passed to the task builder prompt).
    #[arg(long, default_value = "")]
    pub scope: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Template to use for pre-filling task fields (bug, feature, refactor, test, docs,
    /// add-tests, refactor-performance, fix-error-handling, add-docs, security-audit).
    #[arg(short = 't', long, value_name = "TEMPLATE")]
    pub template: Option<String>,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used with --template to auto-fill template variables.
    #[arg(long, value_name = "PATH")]
    pub target: Option<String>,
}

/// Batching mode for grouping related files in build-refactor.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
#[clap(rename_all = "snake_case")]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    #[default]
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

#[derive(Args)]
#[command(after_long_help = "Examples:
 ralph task build refactor
 ralph task build refactor --threshold 700
 ralph task build refactor --path crates/ralph/src/cli
 ralph task build refactor --dry-run --threshold 500
 ralph task build refactor --batch never
 ralph task build refactor --tags urgent,technical-debt")]
pub struct TaskBuildRefactorArgs {
    /// LOC threshold for flagging files as "large" (default: 1000).
    /// Files exceeding ~1000 LOC are presumed mis-scoped per AGENTS.md.
    #[arg(long, default_value = "1000")]
    pub threshold: usize,

    /// Directory to scan for Rust files (default: current directory / repo root).
    #[arg(long)]
    pub path: Option<std::path::PathBuf>,

    /// Preview tasks without inserting into queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Batching behavior for related files.
    /// - auto: Group files in same directory with similar names (default).
    /// - never: Create individual task per file.
    /// - aggressive: Group all files in same module.
    #[arg(long, value_enum, default_value = "auto")]
    pub batch: BatchMode,

    /// Additional tags to add to generated tasks (comma-separated).
    #[arg(long)]
    pub tags: Option<String>,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n ralph task show RQ-0001\n ralph task show RQ-0001 --format compact"
)]
pub struct TaskShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    pub format: QueueShowFormat,
}

#[derive(Args)]
pub struct TaskReadyArgs {
    /// Optional note to append when marking ready.
    #[arg(long)]
    pub note: Option<String>,

    /// Draft task ID to promote.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskStatusArg {
    /// Task is a draft and not ready to run.
    Draft,
    /// Task is waiting to be started.
    Todo,
    /// Task is currently being worked on.
    Doing,
    /// Task is complete (terminal, archived).
    Done,
    /// Task was rejected (terminal, archived).
    Rejected,
}

impl From<TaskStatusArg> for TaskStatus {
    fn from(value: TaskStatusArg) -> Self {
        match value {
            TaskStatusArg::Draft => TaskStatus::Draft,
            TaskStatusArg::Todo => TaskStatus::Todo,
            TaskStatusArg::Doing => TaskStatus::Doing,
            TaskStatusArg::Done => TaskStatus::Done,
            TaskStatusArg::Rejected => TaskStatus::Rejected,
        }
    }
}

#[derive(Args)]
pub struct TaskStatusArgs {
    /// Optional note to append.
    #[arg(long)]
    pub note: Option<String>,

    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskDoneArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to complete.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskRejectArgs {
    /// Notes to append (repeatable).
    #[arg(long)]
    pub note: Vec<String>,

    /// Task ID to reject.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Args)]
pub struct TaskFieldArgs {
    /// Custom field key (must not contain whitespace).
    pub key: String,

    /// Custom field value.
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

#[derive(Args)]
pub struct TaskCloneArgs {
    /// Source task ID to clone.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Status for the cloned task (default: draft).
    #[arg(long, value_enum)]
    pub status: Option<TaskStatusArg>,

    /// Prefix to add to the cloned task title.
    #[arg(long)]
    pub title_prefix: Option<String>,

    /// Preview the clone without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

/// Batch operation type.
#[derive(Subcommand)]
pub enum BatchOperation {
    /// Update status for multiple tasks.
    Status(BatchStatusArgs),
    /// Set a custom field on multiple tasks.
    Field(BatchFieldArgs),
    /// Edit any field on multiple tasks.
    Edit(BatchEditArgs),
}

/// Arguments for batch status operation.
#[derive(Args)]
pub struct BatchStatusArgs {
    /// New status.
    #[arg(value_enum)]
    pub status: TaskStatusArg,

    /// Optional note to append to all affected tasks.
    #[arg(long)]
    pub note: Option<String>,

    /// Task IDs to update (mutually exclusive with --tag-filter).
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag (case-insensitive, repeatable).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

/// Arguments for batch field operation.
#[derive(Args)]
pub struct BatchFieldArgs {
    /// Custom field key.
    pub key: String,

    /// Custom field value.
    pub value: String,

    /// Task IDs to update (mutually exclusive with --tag-filter).
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag (case-insensitive, repeatable).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

/// Arguments for batch edit operation.
#[derive(Args)]
pub struct BatchEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value.
    pub value: String,

    /// Task IDs to update (mutually exclusive with --tag-filter).
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag (case-insensitive, repeatable).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,
}

/// Arguments for the batch command.
#[derive(Args)]
pub struct TaskBatchArgs {
    /// Batch operation type.
    #[command(subcommand)]
    pub operation: BatchOperation,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,

    /// Continue on individual task failures (default: atomic/all-or-nothing).
    #[arg(long)]
    pub continue_on_error: bool,
}

#[derive(Args)]
pub struct TaskEditArgs {
    /// Task field to update.
    #[arg(value_enum)]
    pub field: TaskEditFieldArg,

    /// New field value (empty string clears optional fields).
    pub value: String,

    /// Task ID(s) to update.
    #[arg(value_name = "TASK_ID...")]
    pub task_ids: Vec<String>,

    /// Filter tasks by tag for batch operation (alternative to explicit IDs).
    #[arg(long, value_name = "TAG")]
    pub tag_filter: Vec<String>,

    /// Preview changes without modifying the queue.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct TaskUpdateArgs {
    /// Fields to update (comma-separated, default: all).
    ///
    /// Valid fields: scope, evidence, plan, notes, tags, depends_on
    #[arg(long, default_value = "")]
    pub fields: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,

    /// Task ID to update (omit to update all tasks).
    #[arg(value_name = "TASK_ID")]
    pub task_id: Option<String>,

    /// Preview changes without modifying the queue.
    ///
    /// For task update, this shows the prompt that would be sent to the runner.
    /// Actual changes depend on runner analysis of repository state.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
#[clap(rename_all = "snake_case")]
pub enum TaskEditFieldArg {
    Title,
    Status,
    Priority,
    Tags,
    Scope,
    Evidence,
    Plan,
    Notes,
    Request,
    DependsOn,
    Blocks,
    RelatesTo,
    Duplicates,
    CustomFields,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
}

impl TaskEditFieldArg {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskEditFieldArg::Title => "title",
            TaskEditFieldArg::Status => "status",
            TaskEditFieldArg::Priority => "priority",
            TaskEditFieldArg::Tags => "tags",
            TaskEditFieldArg::Scope => "scope",
            TaskEditFieldArg::Evidence => "evidence",
            TaskEditFieldArg::Plan => "plan",
            TaskEditFieldArg::Notes => "notes",
            TaskEditFieldArg::Request => "request",
            TaskEditFieldArg::DependsOn => "depends_on",
            TaskEditFieldArg::Blocks => "blocks",
            TaskEditFieldArg::RelatesTo => "relates_to",
            TaskEditFieldArg::Duplicates => "duplicates",
            TaskEditFieldArg::CustomFields => "custom_fields",
            TaskEditFieldArg::CreatedAt => "created_at",
            TaskEditFieldArg::UpdatedAt => "updated_at",
            TaskEditFieldArg::CompletedAt => "completed_at",
        }
    }
}

impl From<TaskEditFieldArg> for TaskEditKey {
    fn from(value: TaskEditFieldArg) -> Self {
        match value {
            TaskEditFieldArg::Title => TaskEditKey::Title,
            TaskEditFieldArg::Status => TaskEditKey::Status,
            TaskEditFieldArg::Priority => TaskEditKey::Priority,
            TaskEditFieldArg::Tags => TaskEditKey::Tags,
            TaskEditFieldArg::Scope => TaskEditKey::Scope,
            TaskEditFieldArg::Evidence => TaskEditKey::Evidence,
            TaskEditFieldArg::Plan => TaskEditKey::Plan,
            TaskEditFieldArg::Notes => TaskEditKey::Notes,
            TaskEditFieldArg::Request => TaskEditKey::Request,
            TaskEditFieldArg::DependsOn => TaskEditKey::DependsOn,
            TaskEditFieldArg::Blocks => TaskEditKey::Blocks,
            TaskEditFieldArg::RelatesTo => TaskEditKey::RelatesTo,
            TaskEditFieldArg::Duplicates => TaskEditKey::Duplicates,
            TaskEditFieldArg::CustomFields => TaskEditKey::CustomFields,
            TaskEditFieldArg::CreatedAt => TaskEditKey::CreatedAt,
            TaskEditFieldArg::UpdatedAt => TaskEditKey::UpdatedAt,
            TaskEditFieldArg::CompletedAt => TaskEditKey::CompletedAt,
        }
    }
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task schedule RQ-0001 '2026-02-01T09:00:00Z'\n  ralph task schedule RQ-0001 'tomorrow 9am'\n  ralph task schedule RQ-0001 'in 2 hours'\n  ralph task schedule RQ-0001 'next monday'\n  ralph task schedule RQ-0001 --clear"
)]
pub struct TaskScheduleArgs {
    /// Task ID to schedule.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Timestamp or relative time expression (e.g., 'tomorrow 9am', 'in 2 hours').
    #[arg(value_name = "WHEN")]
    pub when: Option<String>,

    /// Clear the scheduled start time.
    #[arg(long, conflicts_with = "when")]
    pub clear: bool,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task relate RQ-0001 blocks RQ-0002\n  ralph task relate RQ-0001 relates_to RQ-0003\n  ralph task relate RQ-0001 duplicates RQ-0004"
)]
pub struct TaskRelateArgs {
    /// Source task ID.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Relationship type (blocks, relates_to, duplicates).
    #[arg(value_name = "RELATION")]
    pub relation: String,

    /// Target task ID.
    #[arg(value_name = "OTHER_TASK_ID")]
    pub other_task_id: String,
}

#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph task blocks RQ-0001 RQ-0002\n  ralph task blocks RQ-0001 RQ-0002 RQ-0003"
)]
pub struct TaskBlocksArgs {
    /// Task that does the blocking.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Task(s) being blocked.
    #[arg(value_name = "BLOCKED_TASK_ID...")]
    pub blocked_task_ids: Vec<String>,
}

#[derive(Args)]
#[command(after_long_help = "Examples:\n  ralph task mark-duplicate RQ-0001 RQ-0002")]
pub struct TaskMarkDuplicateArgs {
    /// Task to mark as duplicate.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Original task this duplicates.
    #[arg(value_name = "ORIGINAL_TASK_ID")]
    pub original_task_id: String,
}

// Task template subcommands

#[derive(Args)]
pub struct TaskTemplateArgs {
    #[command(subcommand)]
    pub command: TaskTemplateCommand,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum TaskTemplateCommand {
    /// List available task templates
    List,
    /// Show template details
    Show(TaskTemplateShowArgs),
    /// Build a task from a template
    Build(TaskTemplateBuildArgs),
}

#[derive(Args)]
pub struct TaskTemplateShowArgs {
    /// Template name (e.g., "bug", "feature")
    pub name: String,
}

#[derive(Args)]
pub struct TaskTemplateBuildArgs {
    /// Template name
    pub template: String,

    /// Target file/path for template variable substitution ({{target}}, {{module}}, {{file}}).
    /// Used to auto-fill template variables with context from the specified path.
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Task title/request
    pub request: Vec<String>,

    /// Additional tags to merge
    #[arg(short, long)]
    pub tags: Option<String>,

    /// Additional scope to merge
    #[arg(short, long)]
    pub scope: Option<String>,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,
}

/// Handle template subcommands
fn handle_template_command(resolved: &config::Resolved, args: TaskTemplateArgs) -> Result<()> {
    use crate::template::{list_templates, load_template};

    match args.command {
        TaskTemplateCommand::List => {
            let templates = list_templates(&resolved.repo_root);
            println!("Available task templates:");
            println!();
            for template in templates {
                let source_label = match template.source {
                    crate::template::TemplateSource::Custom(_) => "(custom)",
                    crate::template::TemplateSource::Builtin(_) => "(built-in)",
                };
                println!(
                    "  {:12} {:10} {}",
                    template.name, source_label, template.description
                );
            }
            println!();
            println!("Use 'ralph task template show <name>' to view template details.");
            println!("Use 'ralph task template build <name> \"request\"' to create from template.");
            Ok(())
        }
        TaskTemplateCommand::Show(show_args) => {
            let (task, source) = load_template(&show_args.name, &resolved.repo_root)?;

            let source_label = match source {
                crate::template::TemplateSource::Custom(path) => {
                    format!("custom ({})", path.display())
                }
                crate::template::TemplateSource::Builtin(_) => "built-in".to_string(),
            };

            println!("Template: {} ({})", show_args.name, source_label);
            println!();

            if !task.tags.is_empty() {
                println!("Tags: {}", task.tags.join(", "));
            }
            if !task.scope.is_empty() {
                println!("Scope: {}", task.scope.join(", "));
            }
            println!("Priority: {}", task.priority);
            println!("Status: {}", task.status);

            if !task.plan.is_empty() {
                println!();
                println!("Plan:");
                for (i, step) in task.plan.iter().enumerate() {
                    println!("  {}. {}", i + 1, step);
                }
            }

            if !task.evidence.is_empty() {
                println!();
                println!("Evidence: {}", task.evidence.join(", "));
            }

            Ok(())
        }
        TaskTemplateCommand::Build(build_args) => {
            let request = task_cmd::read_request_from_args_or_stdin(&build_args.request)?;
            let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
                runner: build_args.runner.clone(),
                model: build_args.model.clone(),
                effort: build_args.effort.clone(),
                repo_prompt: build_args.repo_prompt,
                runner_cli: build_args.runner_cli.clone(),
            })?;

            // Merge template tags and scope with user-provided values
            let hint_tags = build_args.tags.unwrap_or_default();
            let hint_scope = build_args.scope.unwrap_or_default();

            task_cmd::build_task(
                resolved,
                task_cmd::TaskBuildOptions {
                    request,
                    hint_tags,
                    hint_scope,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force: false,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        build_args.repo_prompt,
                        resolved,
                    ),
                    template_hint: Some(build_args.template),
                    template_target: build_args.target,
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::queue::QueueShowFormat;
    use crate::cli::task::{TaskEditFieldArg, TaskStatusArg};
    use crate::cli::Cli;

    #[test]
    fn task_update_help_mentions_rp_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let update = task
            .find_subcommand_mut("update")
            .expect("task update subcommand");
        let help = update.render_long_help().to_string();

        assert!(
            help.contains("ralph task update --repo-prompt plan RQ-0001"),
            "missing repo-prompt plan example: {help}"
        );
        assert!(
            help.contains("ralph task update --repo-prompt off --fields scope,evidence RQ-0001"),
            "missing repo-prompt off example: {help}"
        );
        assert!(
            help.contains("ralph task update --approval-mode auto-edits --runner claude RQ-0001"),
            "missing approval-mode example: {help}"
        );
    }

    #[test]
    fn task_show_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let show = task
            .find_subcommand_mut("show")
            .expect("task show subcommand");
        let help = show.render_long_help().to_string();

        assert!(
            help.contains("ralph task show RQ-0001"),
            "missing show example: {help}"
        );
        assert!(
            help.contains("--format compact"),
            "missing format example: {help}"
        );
    }

    #[test]
    fn task_details_alias_parses() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "details", "RQ-0001", "--format", "compact"])
                .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Show(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.format, QueueShowFormat::Compact);
                }
                _ => panic!("expected task show command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
            "--repo-prompt",
            "plan",
            "-e",
            "high",
            "Add tests",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Build(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Plan));
                    assert_eq!(args.effort.as_deref(), Some("high"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
            "--approval-mode",
            "yolo",
            "--sandbox",
            "disabled",
            "Add tests",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Build(args)) => {
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("yolo"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
            "--repo-prompt",
            "off",
            "-e",
            "low",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Off));
                    assert_eq!(args.effort.as_deref(), Some("low"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
            "--approval-mode",
            "auto-edits",
            "--sandbox",
            "disabled",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_parses_dry_run_flag() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "edit",
            "--dry-run",
            "title",
            "New title",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert!(args.dry_run);
                    assert_eq!(args.task_ids, vec!["RQ-0001"]);
                    assert_eq!(args.value, "New title");
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_without_dry_run_defaults_to_false() {
        let cli = Cli::try_parse_from(["ralph", "task", "edit", "title", "New title", "RQ-0001"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_dry_run_flag() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "--dry-run", "RQ-0001"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert!(args.dry_run);
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_without_dry_run_defaults_to_false() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "RQ-0001"]).expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_refactor_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "refactor"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Refactor(_)) => {}
                _ => panic!("expected task refactor command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_ref_alias_parses() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "ref", "--threshold", "800"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Refactor(args)) => {
                    assert_eq!(args.threshold, 800);
                }
                _ => panic!("expected task refactor command via alias"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_refactor_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "build-refactor", "--threshold", "700"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::BuildRefactor(args)) => {
                    assert_eq!(args.threshold, 700);
                }
                _ => panic!("expected task build-refactor command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "clone", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_duplicate_alias_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "duplicate", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                }
                _ => panic!("expected task clone command via duplicate alias"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_status_flag() {
        let cli = Cli::try_parse_from(["ralph", "task", "clone", "--status", "todo", "RQ-0001"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.status, Some(TaskStatusArg::Todo));
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_title_prefix() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "clone",
            "--title-prefix",
            "[Follow-up] ",
            "RQ-0001",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.title_prefix, Some("[Follow-up] ".to_string()));
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_dry_run_flag() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "clone", "--dry-run", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert!(args.dry_run);
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let clone = task
            .find_subcommand_mut("clone")
            .expect("task clone subcommand");
        let help = clone.render_long_help().to_string();

        assert!(
            help.contains("ralph task clone RQ-0001"),
            "missing clone example: {help}"
        );
        assert!(
            help.contains("--status"),
            "missing --status example: {help}"
        );
        assert!(
            help.contains("--title-prefix"),
            "missing --title-prefix example: {help}"
        );
        assert!(
            help.contains("ralph task duplicate"),
            "missing duplicate alias example: {help}"
        );
    }

    #[test]
    fn task_batch_status_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "batch", "status", "doing", "RQ-0001", "RQ-0002", "RQ-0003",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::BatchOperation::Status(status_args) => {
                        assert_eq!(status_args.status, TaskStatusArg::Doing);
                        assert_eq!(status_args.task_ids, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
                        assert!(!args.dry_run);
                        assert!(!args.continue_on_error);
                    }
                    _ => panic!("expected batch status operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_status_parses_tag_filter() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "batch",
            "status",
            "doing",
            "--tag-filter",
            "rust",
            "--tag-filter",
            "cli",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::BatchOperation::Status(status_args) => {
                        assert_eq!(status_args.status, TaskStatusArg::Doing);
                        assert!(status_args.task_ids.is_empty());
                        assert_eq!(status_args.tag_filter, vec!["rust", "cli"]);
                    }
                    _ => panic!("expected batch status operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_field_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "batch", "field", "severity", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::BatchOperation::Field(field_args) => {
                        assert_eq!(field_args.key, "severity");
                        assert_eq!(field_args.value, "high");
                        assert_eq!(field_args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                    }
                    _ => panic!("expected batch field operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_edit_parses_dry_run() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "batch",
            "--dry-run",
            "edit",
            "priority",
            "high",
            "RQ-0001",
            "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => {
                    assert!(args.dry_run);
                    assert!(!args.continue_on_error);
                    match args.operation {
                        crate::cli::task::BatchOperation::Edit(edit_args) => {
                            assert_eq!(edit_args.field, TaskEditFieldArg::Priority);
                            assert_eq!(edit_args.value, "high");
                            assert_eq!(edit_args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                        }
                        _ => panic!("expected batch edit operation"),
                    }
                }
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_parses_continue_on_error() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "batch",
            "--continue-on-error",
            "status",
            "doing",
            "RQ-0001",
            "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => {
                    assert!(!args.dry_run);
                    assert!(args.continue_on_error);
                    match args.operation {
                        crate::cli::task::BatchOperation::Status(status_args) => {
                            assert_eq!(status_args.status, TaskStatusArg::Doing);
                        }
                        _ => panic!("expected batch status operation"),
                    }
                }
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let batch = task
            .find_subcommand_mut("batch")
            .expect("task batch subcommand");
        let help = batch.render_long_help().to_string();

        assert!(
            help.contains("ralph task batch status doing"),
            "missing batch status example: {help}"
        );
        assert!(
            help.contains("--tag-filter"),
            "missing --tag-filter example: {help}"
        );
        assert!(
            help.contains("--dry-run"),
            "missing --dry-run example: {help}"
        );
        assert!(
            help.contains("--continue-on-error"),
            "missing --continue-on-error example: {help}"
        );
    }

    #[test]
    fn task_status_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "status", "doing", "RQ-0001", "RQ-0002", "RQ-0003",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Status(args)) => {
                    assert_eq!(args.status, TaskStatusArg::Doing);
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
                }
                _ => panic!("expected task status command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_status_parses_tag_filter() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "status",
            "doing",
            "--tag-filter",
            "rust",
            "--tag-filter",
            "cli",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Status(args)) => {
                    assert_eq!(args.status, TaskStatusArg::Doing);
                    assert!(args.task_ids.is_empty());
                    assert_eq!(args.tag_filter, vec!["rust", "cli"]);
                }
                _ => panic!("expected task status command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_field_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "field", "severity", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Field(args)) => {
                    assert_eq!(args.key, "severity");
                    assert_eq!(args.value, "high");
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                }
                _ => panic!("expected task field command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "edit", "priority", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert_eq!(args.field, TaskEditFieldArg::Priority);
                    assert_eq!(args.value, "high");
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }
}
