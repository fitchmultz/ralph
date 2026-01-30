//! Task-building and task-updating command helpers (request parsing, runner invocation, and queue updates).
//!
//! Responsibilities:
//! - Build task requests/prompts and invoke runners for task creation or updates.
//! - Validate queue/done state before and after runner execution.
//! - Parse task request inputs from CLI args or stdin.
//! - Scan for large files and create refactoring tasks (build-refactor command).
//!
//! Not handled here:
//! - CLI argument definitions or command routing.
//! - Runner process implementation details or output parsing.
//! - Queue schema definitions or config persistence.
//!
//! Invariants/assumptions:
//! - Queue/done files are the source of truth for task ordering and status.
//! - Runner execution requires stream-json output for parsing.
//! - Permission/approval defaults come from config unless overridden at CLI.
//! - LOC counting excludes comments and empty lines for accurate measurement.

use crate::commands::run::PhaseType;
use crate::contracts::{
    ClaudePermissionMode, Model, ProjectType, ReasoningEffort, Runner, RunnerCliOptionsPatch,
};
use crate::{config, prompts, queue, runner, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

/// Batching mode for grouping related files in build-refactor.
#[derive(Clone, Copy, Debug)]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

impl From<crate::cli::task::BatchMode> for BatchMode {
    fn from(mode: crate::cli::task::BatchMode) -> Self {
        match mode {
            crate::cli::task::BatchMode::Auto => BatchMode::Auto,
            crate::cli::task::BatchMode::Never => BatchMode::Never,
            crate::cli::task::BatchMode::Aggressive => BatchMode::Aggressive,
        }
    }
}

/// Options for the build-refactor command.
pub struct TaskBuildRefactorOptions {
    pub threshold: usize,
    pub path: Option<PathBuf>,
    pub dry_run: bool,
    pub batch: BatchMode,
    pub extra_tags: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
}

// TaskBuildOptions controls runner-driven task creation via .ralph/prompts/task_builder.md.
pub struct TaskBuildOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    /// Optional template name to use as a base for task fields
    pub template_hint: Option<String>,
    /// Optional target path for template variable substitution
    pub template_target: Option<String>,
}

// TaskUpdateSettings controls runner-driven task updates via .ralph/prompts/task_updater.md.
pub struct TaskUpdateSettings {
    pub fields: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
struct TaskRunnerSettings {
    runner: Runner,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    runner_cli: runner::ResolvedRunnerCliOptions,
    permission_mode: Option<ClaudePermissionMode>,
}

fn resolve_task_runner_settings(
    resolved: &config::Resolved,
    runner_override: Option<Runner>,
    model_override: Option<Model>,
    reasoning_effort_override: Option<ReasoningEffort>,
    runner_cli_overrides: &RunnerCliOptionsPatch,
) -> Result<TaskRunnerSettings> {
    let settings = runner::resolve_agent_settings(
        runner_override,
        model_override,
        reasoning_effort_override,
        runner_cli_overrides,
        None,
        &resolved.config.agent,
    )?;

    Ok(TaskRunnerSettings {
        runner: settings.runner,
        model: settings.model,
        reasoning_effort: settings.reasoning_effort,
        runner_cli: settings.runner_cli,
        permission_mode: resolved.config.agent.claude_permission_mode,
    })
}

fn resolve_task_build_settings(
    resolved: &config::Resolved,
    opts: &TaskBuildOptions,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        opts.runner_override,
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
    )
}

fn resolve_task_update_settings(
    resolved: &config::Resolved,
    settings: &TaskUpdateSettings,
) -> Result<TaskRunnerSettings> {
    resolve_task_runner_settings(
        resolved,
        settings.runner_override,
        settings.model_override.clone(),
        settings.reasoning_effort_override,
        &settings.runner_cli_overrides,
    )
}

pub fn read_request_from_args_or_reader(
    args: &[String],
    stdin_is_terminal: bool,
    mut reader: impl Read,
) -> Result<String> {
    if !args.is_empty() {
        let joined = args.join(" ");
        let trimmed = joined.trim();
        if trimmed.is_empty() {
            bail!("Missing request: task requires a request description. Pass arguments or pipe input to the command.");
        }
        return Ok(trimmed.to_string());
    }

    if stdin_is_terminal {
        bail!("Missing request: task requires a request description. Pass arguments or pipe input to the command.");
    }

    let mut buf = String::new();
    reader.read_to_string(&mut buf).context("read stdin")?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        bail!("Missing request: task requires a request description (pass arguments or pipe input to the command).");
    }
    Ok(trimmed.to_string())
}

// read_request_from_args_or_stdin joins any positional args, otherwise reads stdin.
pub fn read_request_from_args_or_stdin(args: &[String]) -> Result<String> {
    let stdin = std::io::stdin();
    let stdin_is_terminal = stdin.is_terminal();
    let handle = stdin.lock();
    read_request_from_args_or_reader(args, stdin_is_terminal, handle)
}

pub fn build_task(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    build_task_impl(resolved, opts, true)
}

pub fn build_task_without_lock(resolved: &config::Resolved, opts: TaskBuildOptions) -> Result<()> {
    build_task_impl(resolved, opts, false)
}

fn build_task_impl(
    resolved: &config::Resolved,
    mut opts: TaskBuildOptions,
    acquire_lock: bool,
) -> Result<()> {
    let _queue_lock = if acquire_lock {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "task",
            opts.force,
        )?)
    } else {
        None
    };

    if opts.request.trim().is_empty() {
        bail!("Missing request: task requires a request description. Provide a non-empty request.");
    }

    // Apply template if specified
    let mut template_context = String::new();
    if let Some(template_name) = opts.template_hint.clone() {
        // Use context-aware loading if target is provided
        let load_result = if let Some(ref target) = opts.template_target {
            crate::template::load_template_with_context(
                &template_name,
                &resolved.repo_root,
                Some(target),
            )
        } else {
            crate::template::load_template(&template_name, &resolved.repo_root)
        };

        match load_result {
            Ok((template, _)) => {
                crate::template::merge_template_with_options(&template, &mut opts);
                template_context = crate::template::format_template_context(&template);
                log::info!("Using template '{}' for task creation", template_name);
            }
            Err(e) => {
                log::warn!("Failed to load template '{}': {}", template_name, e);
            }
        }
    }

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    // Compute insertion strategy from pre-run queue state
    let insert_index = queue::suggest_new_task_insert_index(&before);

    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &before,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task")?;
    let before_ids = queue::task_id_set(&before);

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt = prompts::render_task_builder_prompt(
        &template,
        &opts.request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
        &resolved.config,
    )?;

    // Append template context to prompt if available
    if !template_context.is_empty() {
        prompt.push_str("\n\n--- Template Suggestions ---\n");
        prompt.push_str(&template_context);
    }

    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_tool_injection);
    prompt = prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let settings = resolve_task_build_settings(resolved, &opts)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    // Two-pass mode disabled for task (only generates task, should not implement)

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: settings.runner,
            bins,
            model: settings.model,
            reasoning_effort: settings.reasoning_effort,
            runner_cli: settings.runner_cli,
            prompt: &prompt,
            timeout: None,
            permission_mode: settings.permission_mode,
            revert_on_error: false,
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            output_handler: None,
            output_stream: runner::OutputStream::Terminal,
            revert_prompt: None,
            phase_type: PhaseType::SinglePhase,
        },
        runutil::RunnerErrorMessages {
            log_label: "task builder",
            interrupted_msg: "Task builder interrupted: the agent run was canceled.",
            timeout_msg: "Task builder timed out: the agent run exceeded the time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Task builder terminated: the agent was stopped by a signal. Review uncommitted changes before rerunning.",
            non_zero_msg: |code| {
                format!(
                    "Task builder failed: the agent exited with a non-zero code ({code}). Review uncommitted changes before rerunning."
                )
            },
            other_msg: |err| {
                format!(
                    "Task builder failed: the agent could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let mut after = match queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))
    {
        Ok(queue) => queue,
        Err(err) => {
            return Err(err);
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task")?;

    let added = queue::added_tasks(&before_ids, &after);
    if !added.is_empty() {
        let added_ids: Vec<String> = added.iter().map(|(id, _)| id.clone()).collect();

        // Enforce smart positioning deterministically
        queue::reposition_new_tasks(&mut after, &added_ids, insert_index);

        let now = timeutil::now_utc_rfc3339_or_fallback();
        let default_request = opts.request.clone();
        queue::backfill_missing_fields(&mut after, &added_ids, &default_request, &now);
        queue::save_queue(&resolved.queue_path, &after)
            .context("save queue with backfilled fields")?;
    }
    if added.is_empty() {
        log::info!("Task builder completed. No new tasks detected.");
    } else {
        log::info!("Task builder added {} task(s):", added.len());
        for (id, title) in added.iter().take(10) {
            log::info!("- {}: {}", id, title);
        }
        if added.len() > 10 {
            log::info!("...and {} more.", added.len() - 10);
        }
    }
    Ok(())
}

pub fn update_task(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, true)
}

pub(crate) fn update_task_without_lock(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, false)
}

pub fn update_all_tasks(resolved: &config::Resolved, settings: &TaskUpdateSettings) -> Result<()> {
    let _queue_lock =
        queue::acquire_queue_lock(&resolved.repo_root, "task update", settings.force)?;

    let queue_file = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    if queue_file.tasks.is_empty() {
        bail!("No tasks in queue to update.");
    }

    let task_ids: Vec<String> = queue_file
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect();
    for task_id in task_ids {
        update_task_impl(resolved, &task_id, settings, false)?;
    }

    Ok(())
}

fn update_task_impl(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
    acquire_lock: bool,
) -> Result<()> {
    // Handle dry-run mode early (before any mutations)
    if settings.dry_run {
        let before = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

        let task_id = task_id.trim();
        let task = before
            .tasks
            .iter()
            .find(|t| t.id.trim() == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let prompt = prompts::render_task_updater_prompt(
            &template,
            task_id,
            &settings.fields,
            project_type,
            &resolved.config,
        )?;

        println!("Dry run - would update task {}:", task_id);
        println!("  Fields to update: {}", settings.fields);
        println!("  Current title: {}", task.title);
        println!("\n  Prompt preview (first 800 chars):");
        let preview_len = prompt.len().min(800);
        println!("{}", &prompt[..preview_len]);
        if prompt.len() > 800 {
            println!("\n  ... ({} more characters)", prompt.len() - 800);
        }
        println!("\n  Note: Actual changes depend on runner analysis of repository state.");
        return Ok(());
    }

    let _queue_lock = if acquire_lock {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "task update",
            settings.force,
        )?)
    } else {
        None
    };

    // Create backup before running task updater
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let backup_path = queue::backup_queue(&resolved.queue_path, &cache_dir)
        .with_context(|| "failed to create queue backup before task update")?;
    log::debug!("Created queue backup at: {}", backup_path.display());

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    let task_id = task_id.trim();
    if !before.tasks.iter().any(|t| t.id.trim() == task_id) {
        bail!("Task not found: {}", task_id);
    }

    let before_task = before
        .tasks
        .iter()
        .find(|t| t.id.trim() == task_id)
        .unwrap();
    let before_json = serde_json::to_string(before_task)?;

    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &before,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task update")?;

    let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_task_updater_prompt(
        &template,
        task_id,
        &settings.fields,
        project_type,
        &resolved.config,
    )?;

    let prompt =
        prompts::wrap_with_repoprompt_requirement(&prompt, settings.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let runner_settings = resolve_task_update_settings(resolved, settings)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: runner_settings.runner,
            bins,
            model: runner_settings.model.clone(),
            reasoning_effort: runner_settings.reasoning_effort,
            runner_cli: runner_settings.runner_cli,
            prompt: &prompt,
            timeout: None,
            permission_mode: runner_settings.permission_mode,
            revert_on_error: true,
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            output_handler: None,
            output_stream: runner::OutputStream::Terminal,
            revert_prompt: None,
            phase_type: PhaseType::SinglePhase,
        },
        runutil::RunnerErrorMessages {
            log_label: "task updater",
            interrupted_msg: "Task updater interrupted: agent run was canceled.",
            timeout_msg: "Task updater timed out: agent run exceeded time limit. Changes in the working tree were reverted; review repo state manually.",
            terminated_msg: "Task updater terminated: agent was stopped by a signal. Review uncommitted changes before rerunning.",
            non_zero_msg: |code| {
                format!(
                    "Task updater failed: agent exited with a non-zero code ({}). Changes in the working tree were reverted; review repo state before rerunning.",
                    code
                )
            },
            other_msg: |err| {
                format!(
                    "Task updater failed: agent could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    // Load queue after update, with repair for common JSON errors
    let after = match queue::load_queue_with_repair(&resolved.queue_path) {
        Ok(queue) => queue,
        Err(err) => {
            log::error!(
                "Failed to parse queue after task update. Backup available at: {}",
                backup_path.display()
            );
            log::error!(
                "To restore from backup, copy the backup file to: {}",
                resolved.queue_path.display()
            );
            return Err(err).with_context(|| {
                format!(
                    "task update for {}: queue file may be corrupted. Backup: {}",
                    task_id,
                    backup_path.display()
                )
            });
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task update")?;

    let after_task = after.tasks.iter().find(|t| t.id.trim() == task_id).unwrap();
    let after_json = serde_json::to_string(after_task)?;

    if before_json == after_json {
        log::info!("Task {} updated. No changes detected.", task_id);
    } else {
        let changed_fields = compare_task_fields(&before_json, &after_json)?;
        log::info!(
            "Task {} updated. Changed fields: {}",
            task_id,
            changed_fields.join(", ")
        );
    }

    queue::save_queue(&resolved.queue_path, &after).context("save queue after task update")?;

    Ok(())
}

pub fn compare_task_fields(before: &str, after: &str) -> Result<Vec<String>> {
    let before_value: serde_json::Value = serde_json::from_str(before)?;
    let after_value: serde_json::Value = serde_json::from_str(after)?;

    if let (Some(before_obj), Some(after_obj)) = (before_value.as_object(), after_value.as_object())
    {
        let mut changed = Vec::new();
        for (key, after_val) in after_obj {
            if let Some(before_val) = before_obj.get(key) {
                if before_val != after_val {
                    changed.push(key.clone());
                }
            } else {
                changed.push(key.clone());
            }
        }
        Ok(changed)
    } else {
        Ok(vec!["task".to_string()])
    }
}

/// Build refactoring tasks for large files exceeding the LOC threshold.
///
/// Scans the specified directory for Rust files, identifies those exceeding
/// the threshold, groups them based on batch mode, and creates tasks using
/// the task builder.
pub fn build_refactor_tasks(
    resolved: &config::Resolved,
    opts: TaskBuildRefactorOptions,
) -> Result<()> {
    // Determine scan path (default to repo root for generic usage)
    let scan_path = opts
        .path
        .clone()
        .unwrap_or_else(|| resolved.repo_root.clone());

    // Scan for large .rs files
    let large_files = scan_for_large_files(&scan_path, opts.threshold)?;

    if large_files.is_empty() {
        println!(
            "No files found exceeding {} LOC threshold in {}.",
            opts.threshold,
            scan_path.display()
        );
        return Ok(());
    }

    println!(
        "Found {} file(s) exceeding {} LOC:",
        large_files.len(),
        opts.threshold
    );
    for (path, loc) in &large_files {
        println!("  {} ({} LOC)", path.display(), loc);
    }

    // Group files based on batch mode
    let groups = group_files(&large_files, opts.batch);

    println!("\nWill create {} task(s):", groups.len());
    for (i, group) in groups.iter().enumerate() {
        match &group[..] {
            [(path, loc)] => {
                println!("  {}. {} ({} LOC)", i + 1, path.display(), loc);
            }
            multiple => {
                let total_loc: usize = multiple.iter().map(|(_, loc)| loc).sum();
                println!(
                    "  {}. {} files in {} ({} total LOC)",
                    i + 1,
                    multiple.len(),
                    multiple[0].0.parent().unwrap_or(&multiple[0].0).display(),
                    total_loc
                );
            }
        }
    }

    if opts.dry_run {
        println!("\nDry run - no tasks created.");
        return Ok(());
    }

    // Create tasks for each group
    let mut created_count = 0;
    for group in groups {
        let request = build_refactor_request(&group);
        let scope = build_scope(&group);

        let mut hint_tags = "refactor,large-file".to_string();
        if !opts.extra_tags.is_empty() {
            hint_tags.push(',');
            hint_tags.push_str(&opts.extra_tags);
        }

        build_task(
            resolved,
            TaskBuildOptions {
                request,
                hint_tags,
                hint_scope: scope,
                runner_override: opts.runner_override,
                model_override: opts.model_override.clone(),
                reasoning_effort_override: opts.reasoning_effort_override,
                runner_cli_overrides: opts.runner_cli_overrides.clone(),
                force: opts.force,
                repoprompt_tool_injection: opts.repoprompt_tool_injection,
                template_hint: Some("refactor".to_string()),
                template_target: None,
            },
        )?;
        created_count += 1;
    }

    println!("\nCreated {} refactoring task(s).", created_count);
    Ok(())
}

/// Scan directory for .rs files exceeding threshold.
/// Returns Vec of (path, loc_count) sorted by loc descending.
fn scan_for_large_files(root: &Path, threshold: usize) -> Result<Vec<(PathBuf, usize)>> {
    let mut results = Vec::new();
    scan_directory_recursive(root, root, threshold, &mut results)?;

    // Sort by LOC descending (largest first)
    results.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(results)
}

/// Recursively scan directory for Rust files.
#[allow(clippy::only_used_in_recursion)]
fn scan_directory_recursive(
    root: &Path,
    current: &Path,
    threshold: usize,
    results: &mut Vec<(PathBuf, usize)>,
) -> Result<()> {
    let entries = std::fs::read_dir(current)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs, target/, and .ralph/cache/
        if path.is_dir() {
            if name_str.starts_with('.') || name_str == "target" {
                continue;
            }
            // Skip .ralph/cache/ to avoid scanning generated/temp files
            if path
                .components()
                .any(|c| c.as_os_str() == ".ralph" || c.as_os_str() == "cache")
            {
                continue;
            }
            scan_directory_recursive(root, &path, threshold, results)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let loc = count_lines_of_code(&path)?;
            if loc > threshold {
                results.push((path.to_path_buf(), loc));
            }
        }
    }

    Ok(())
}

/// Count non-empty, non-comment lines in a Rust file.
fn count_lines_of_code(path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let mut count = 0;
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        count += 1;
    }

    Ok(count)
}

/// Group files based on batch mode strategy.
fn group_files(files: &[(PathBuf, usize)], mode: BatchMode) -> Vec<Vec<(PathBuf, usize)>> {
    match mode {
        BatchMode::Never => files.iter().map(|f| vec![f.clone()]).collect(),
        BatchMode::Aggressive => {
            // Group by parent directory
            let mut groups: std::collections::HashMap<PathBuf, Vec<(PathBuf, usize)>> =
                std::collections::HashMap::new();
            for (path, loc) in files {
                let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                groups.entry(parent).or_default().push((path.clone(), *loc));
            }
            groups.into_values().collect()
        }
        BatchMode::Auto => {
            // Group files with similar names in same directory
            // (e.g., test_*.rs, *_tests.rs)
            let mut groups: Vec<Vec<(PathBuf, usize)>> = Vec::new();
            let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();

            for (i, (path, loc)) in files.iter().enumerate() {
                if used.contains(&i) {
                    continue;
                }

                let parent = path.parent();
                let stem = path.file_stem().and_then(|s| s.to_str());

                let mut group = vec![(path.clone(), *loc)];
                used.insert(i);

                // Look for related files
                for (j, (other_path, other_loc)) in files.iter().enumerate().skip(i + 1) {
                    if used.contains(&j) {
                        continue;
                    }

                    if other_path.parent() != parent {
                        continue;
                    }

                    let other_stem = other_path.file_stem().and_then(|s| s.to_str());

                    // Check for test file relationships
                    if let (Some(s), Some(os)) = (stem, other_stem) {
                        if is_related_file(s, os) {
                            group.push((other_path.clone(), *other_loc));
                            used.insert(j);
                        }
                    }
                }

                groups.push(group);
            }

            groups
        }
    }
}

/// Check if two file stems are related (e.g., "foo" and "foo_tests").
fn is_related_file(a: &str, b: &str) -> bool {
    let test_suffixes = ["_test", "_tests", "test_"];

    for suffix in &test_suffixes {
        if a.starts_with(suffix) && b == &a[suffix.len()..] {
            return true;
        }
        if b.starts_with(suffix) && a == &b[suffix.len()..] {
            return true;
        }
        if a.ends_with(suffix) && b == &a[..a.len() - suffix.len()] {
            return true;
        }
        if b.ends_with(suffix) && a == &b[..b.len() - suffix.len()] {
            return true;
        }
    }

    false
}

/// Build the request text for a refactoring task.
fn build_refactor_request(group: &[(PathBuf, usize)]) -> String {
    match group {
        [(path, loc)] => {
            format!(
                "Refactor {} ({} LOC) to improve maintainability by splitting it into smaller, cohesive modules per AGENTS.md guidelines.",
                path.display(),
                loc
            )
        }
        files => {
            let total_loc: usize = files.iter().map(|(_, loc)| loc).sum();
            let paths: Vec<String> = files.iter().map(|(p, _)| p.display().to_string()).collect();
            format!(
                "Refactor {} related files ({} total LOC) to improve maintainability by splitting them into smaller, cohesive modules per AGENTS.md guidelines. Files: {}",
                files.len(),
                total_loc,
                paths.join(", ")
            )
        }
    }
}

/// Build the scope string for a group of files.
fn build_scope(group: &[(PathBuf, usize)]) -> String {
    group
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{
        build_refactor_request, build_scope, count_lines_of_code, is_related_file,
        read_request_from_args_or_reader, resolve_task_build_settings,
        resolve_task_update_settings, TaskBuildOptions, TaskUpdateSettings,
    };
    use crate::config;
    use crate::contracts::{
        ClaudePermissionMode, Config, RunnerApprovalMode, RunnerCliConfigRoot,
        RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
        RunnerVerbosity, UnsupportedOptionPolicy,
    };
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_with_config(config: Config) -> (config::Resolved, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();
        let queue_rel = config
            .queue
            .file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/queue.json"));
        let done_rel = config
            .queue
            .done_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/done.json"));
        let id_prefix = config
            .queue
            .id_prefix
            .clone()
            .unwrap_or_else(|| "RQ".to_string());
        let id_width = config.queue.id_width.unwrap_or(4) as usize;

        (
            config::Resolved {
                config,
                repo_root: repo_root.clone(),
                queue_path: repo_root.join(queue_rel),
                done_path: repo_root.join(done_rel),
                id_prefix,
                id_width,
                global_config_path: None,
                project_config_path: Some(repo_root.join(".ralph/config.json")),
            },
            dir,
        )
    }

    fn build_opts() -> TaskBuildOptions {
        TaskBuildOptions {
            request: "request".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        }
    }

    fn update_settings() -> TaskUpdateSettings {
        TaskUpdateSettings {
            fields: "scope".to_string(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            dry_run: false,
        }
    }

    #[test]
    fn read_request_from_args_or_reader_rejects_empty_args_on_terminal() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("");
        let err = read_request_from_args_or_reader(&args, true, reader).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("Missing request"));
        assert!(message.contains("Pass arguments"));
    }

    #[test]
    fn read_request_from_args_or_reader_reads_piped_input() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("  hello world  ");
        let value = read_request_from_args_or_reader(&args, false, reader).unwrap();
        assert_eq!(value, "hello world");
    }

    #[test]
    fn read_request_from_args_or_reader_rejects_empty_piped_input() {
        let args: Vec<String> = vec![];
        let reader = Cursor::new("   ");
        let err = read_request_from_args_or_reader(&args, false, reader).unwrap_err();
        assert!(err.to_string().contains("Missing request"));
    }

    #[test]
    fn task_build_respects_config_permission_mode_when_approval_default() {
        let mut config = Config::default();
        config.agent.claude_permission_mode = Some(ClaudePermissionMode::AcceptEdits);
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                verbosity: Some(RunnerVerbosity::Normal),
                approval_mode: Some(RunnerApprovalMode::Default),
                sandbox: Some(RunnerSandboxMode::Default),
                plan_mode: Some(RunnerPlanMode::Default),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let settings = resolve_task_build_settings(&resolved, &build_opts()).expect("settings");
        let effective = settings
            .runner_cli
            .effective_claude_permission_mode(settings.permission_mode);
        assert_eq!(effective, Some(ClaudePermissionMode::AcceptEdits));
    }

    #[test]
    fn task_update_cli_override_yolo_bypasses_permission_mode() {
        let mut config = Config::default();
        config.agent.claude_permission_mode = Some(ClaudePermissionMode::AcceptEdits);
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                verbosity: Some(RunnerVerbosity::Normal),
                approval_mode: Some(RunnerApprovalMode::Default),
                sandbox: Some(RunnerSandboxMode::Default),
                plan_mode: Some(RunnerPlanMode::Default),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
            },
            runners: BTreeMap::new(),
        });

        let mut settings = update_settings();
        settings.runner_cli_overrides = RunnerCliOptionsPatch {
            approval_mode: Some(RunnerApprovalMode::Yolo),
            ..RunnerCliOptionsPatch::default()
        };

        let (resolved, _dir) = resolved_with_config(config);
        let runner_settings = resolve_task_update_settings(&resolved, &settings).expect("settings");
        let effective = runner_settings
            .runner_cli
            .effective_claude_permission_mode(runner_settings.permission_mode);
        assert_eq!(effective, Some(ClaudePermissionMode::BypassPermissions));
    }

    #[test]
    fn task_build_fails_fast_when_safe_approval_requires_prompt() {
        let mut config = Config::default();
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                approval_mode: Some(RunnerApprovalMode::Safe),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
                ..RunnerCliOptionsPatch::default()
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let err = resolve_task_build_settings(&resolved, &build_opts()).expect_err("error");
        assert!(err.to_string().contains("approval_mode=safe"));
    }

    #[test]
    fn task_update_fails_fast_when_safe_approval_requires_prompt() {
        let mut config = Config::default();
        config.agent.runner_cli = Some(RunnerCliConfigRoot {
            defaults: RunnerCliOptionsPatch {
                output_format: Some(RunnerOutputFormat::StreamJson),
                approval_mode: Some(RunnerApprovalMode::Safe),
                unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
                ..RunnerCliOptionsPatch::default()
            },
            runners: BTreeMap::new(),
        });

        let (resolved, _dir) = resolved_with_config(config);
        let err = resolve_task_update_settings(&resolved, &update_settings()).expect_err("error");
        assert!(err.to_string().contains("approval_mode=safe"));
    }

    #[test]
    fn count_lines_of_code_skips_comments_and_empty() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "// comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "fn main() {{").unwrap();
        writeln!(f, "    println!(\"hello\");").unwrap();
        writeln!(f, "}}").unwrap();

        let loc = count_lines_of_code(&file).unwrap();
        assert_eq!(loc, 3); // fn main, println, closing brace
    }

    #[test]
    fn count_lines_of_code_handles_block_comments() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "/* block comment start").unwrap();
        writeln!(f, "   continues here */").unwrap();
        writeln!(f, "fn main() {{").unwrap();
        writeln!(f, "    /* inline */ println!(\"hello\");").unwrap();
        writeln!(f, "}}").unwrap();

        let loc = count_lines_of_code(&file).unwrap();
        assert_eq!(loc, 2); // fn main, println
    }

    #[test]
    fn is_related_file_detects_test_pairs() {
        assert!(is_related_file("foo", "foo_test"));
        assert!(is_related_file("foo_test", "foo"));
        assert!(is_related_file("test_foo", "foo"));
        assert!(is_related_file("foo", "test_foo"));
        assert!(is_related_file("foo_tests", "foo"));
        assert!(is_related_file("foo", "foo_tests"));
        assert!(!is_related_file("foo", "bar"));
        assert!(!is_related_file("foo_test", "bar"));
    }

    #[test]
    fn build_refactor_request_single_file() {
        let group = vec![(PathBuf::from("src/main.rs"), 1200)];
        let request = build_refactor_request(&group);
        assert!(request.contains("src/main.rs"));
        assert!(request.contains("1200 LOC"));
        assert!(request.contains("AGENTS.md"));
    }

    #[test]
    fn build_refactor_request_multiple_files() {
        let group = vec![
            (PathBuf::from("src/foo.rs"), 800),
            (PathBuf::from("src/foo_test.rs"), 500),
        ];
        let request = build_refactor_request(&group);
        assert!(request.contains("2 related files"));
        assert!(request.contains("1300 total LOC"));
        assert!(request.contains("src/foo.rs"));
        assert!(request.contains("src/foo_test.rs"));
    }

    #[test]
    fn build_scope_single_file() {
        let group = vec![(PathBuf::from("src/main.rs"), 1200)];
        let scope = build_scope(&group);
        assert_eq!(scope, "src/main.rs");
    }

    #[test]
    fn build_scope_multiple_files() {
        let group = vec![
            (PathBuf::from("src/foo.rs"), 800),
            (PathBuf::from("src/bar.rs"), 500),
        ];
        let scope = build_scope(&group);
        assert_eq!(scope, "src/foo.rs,src/bar.rs");
    }
}
