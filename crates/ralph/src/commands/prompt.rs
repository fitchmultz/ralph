//! Prompt inspection/preview commands.
//!
//! This module exists to make prompt compilation observable and auditable.
//! It renders the exact final prompt that would be sent to a runner for:
//! - worker (single-phase / phase1 / phase2)
//! - scan
//! - task builder
//!
//! The logic intentionally re-uses existing prompt rendering + wrappers so that
//! previews stay accurate as runtime behavior evolves.
//!
//! Also provides prompt management commands (list, show, export, sync, diff) for
//! viewing and managing embedded prompt templates.

use crate::config;
use crate::contracts::ProjectType;
use crate::promptflow::{self, PromptPolicy};
use crate::prompts_internal::management as prompt_mgmt;
use crate::{prompts, queue};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// List all available prompt templates.
pub fn list_prompts(repo_root: &Path) -> Result<()> {
    let templates = prompt_mgmt::list_templates(repo_root);

    println!("Available prompt templates ({} total):\n", templates.len());

    // Find max name length for alignment
    let max_name_len = templates.iter().map(|t| t.name.len()).max().unwrap_or(0);

    for t in templates {
        let status = if t.has_override { " [override]" } else { "" };
        println!(
            "  {:width$}  {}{}",
            t.name,
            t.description,
            status,
            width = max_name_len
        );
    }

    println!("\nOverride paths: .ralph/prompts/<name>.md");
    println!("Use 'ralph prompt show <name> --raw' to view raw embedded content");

    Ok(())
}

/// Show a specific prompt template.
pub fn show_prompt(repo_root: &Path, name: &str, raw: bool) -> Result<()> {
    let id = prompt_mgmt::parse_template_name(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", name))?;

    let content = if raw {
        prompt_mgmt::get_embedded_content(id).to_string()
    } else {
        prompt_mgmt::get_effective_content(repo_root, id)?
    };

    print!("{}", content);
    Ok(())
}

/// Export prompt(s) to .ralph/prompts/.
pub fn export_prompts(repo_root: &Path, name: Option<&str>, force: bool) -> Result<()> {
    let ralph_version = env!("CARGO_PKG_VERSION");

    if let Some(n) = name {
        // Export single template
        let id = prompt_mgmt::parse_template_name(n)
            .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", n))?;

        let file_name = prompt_mgmt::template_file_name(id);
        let written = prompt_mgmt::export_template(repo_root, id, force, ralph_version)?;

        if written {
            println!("Exported {} to .ralph/prompts/{}.md", file_name, file_name);
        } else {
            println!(
                "Skipped {}: file already exists (use --force to overwrite)",
                file_name
            );
        }
    } else {
        // Export all templates
        let templates = prompt_mgmt::all_template_ids();
        let mut exported = 0;
        let mut skipped = 0;

        for id in templates {
            let file_name = prompt_mgmt::template_file_name(id);
            match prompt_mgmt::export_template(repo_root, id, force, ralph_version) {
                Ok(written) => {
                    if written {
                        exported += 1;
                        println!("Exported {}", file_name);
                    } else {
                        skipped += 1;
                        println!("Skipped {}: already exists", file_name);
                    }
                }
                Err(e) => {
                    eprintln!("Error exporting {}: {}", file_name, e);
                }
            }
        }

        println!("\nExported {} templates, skipped {}", exported, skipped);
        if skipped > 0 && !force {
            println!("Use --force to overwrite existing files");
        }
    }

    Ok(())
}

/// Sync prompts with embedded defaults.
pub fn sync_prompts(repo_root: &Path, dry_run: bool, force: bool) -> Result<()> {
    let ralph_version = env!("CARGO_PKG_VERSION");
    let templates = prompt_mgmt::all_template_ids();

    let mut up_to_date = Vec::new();
    let mut outdated = Vec::new();
    let mut user_modified = Vec::new();
    let mut missing = Vec::new();

    // Categorize all templates
    for id in &templates {
        let file_name = prompt_mgmt::template_file_name(*id);
        let status = prompt_mgmt::check_sync_status(repo_root, *id)?;

        match status {
            prompt_mgmt::SyncStatus::UpToDate => up_to_date.push(file_name),
            prompt_mgmt::SyncStatus::Outdated => outdated.push((file_name, *id)),
            prompt_mgmt::SyncStatus::UserModified => user_modified.push((file_name, *id)),
            prompt_mgmt::SyncStatus::Unknown => user_modified.push((file_name, *id)),
            prompt_mgmt::SyncStatus::Missing => missing.push((file_name, *id)),
        }
    }

    if dry_run {
        println!("Dry run - no changes will be made:\n");

        if !outdated.is_empty() {
            println!("Would update ({}):", outdated.len());
            for (name, _) in &outdated {
                println!("  {}", name);
            }
        }

        if !missing.is_empty() {
            println!("Would create ({}):", missing.len());
            for (name, _) in &missing {
                println!("  {}", name);
            }
        }

        if !user_modified.is_empty() {
            println!("Would skip (user modified) ({}):", user_modified.len());
            for (name, _) in &user_modified {
                println!("  {}", name);
            }
        }

        if !up_to_date.is_empty() {
            println!("Up to date ({}):", up_to_date.len());
            for name in &up_to_date {
                println!("  {}", name);
            }
        }

        return Ok(());
    }

    // Perform sync
    let mut updated = 0;
    let mut skipped = 0;
    let mut created = 0;

    // Update outdated
    for (name, id) in outdated {
        match prompt_mgmt::export_template(repo_root, id, true, ralph_version) {
            Ok(_) => {
                println!("Updated {} (outdated)", name);
                updated += 1;
            }
            Err(e) => {
                eprintln!("Error updating {}: {}", name, e);
                skipped += 1;
            }
        }
    }

    // Create missing
    for (name, id) in missing {
        match prompt_mgmt::export_template(repo_root, id, false, ralph_version) {
            Ok(_) => {
                println!("Created {}", name);
                created += 1;
            }
            Err(e) => {
                eprintln!("Error creating {}: {}", name, e);
                skipped += 1;
            }
        }
    }

    // Handle user modified
    for (name, id) in user_modified {
        if force {
            match prompt_mgmt::export_template(repo_root, id, true, ralph_version) {
                Ok(_) => {
                    println!("Overwrote {} (user modified, --force)", name);
                    updated += 1;
                }
                Err(e) => {
                    eprintln!("Error overwriting {}: {}", name, e);
                    skipped += 1;
                }
            }
        } else {
            println!("Skipped {} (user modified, use --force to overwrite)", name);
            skipped += 1;
        }
    }

    println!(
        "\nSync complete: {} updated, {} created, {} skipped",
        updated, created, skipped
    );

    Ok(())
}

/// Show diff between user override and embedded default.
pub fn diff_prompt(repo_root: &Path, name: &str) -> Result<()> {
    let id = prompt_mgmt::parse_template_name(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown template name: '{}'", name))?;

    match prompt_mgmt::generate_diff(repo_root, id)? {
        Some(diff) => {
            print!("{}", diff);
        }
        None => {
            println!("No local override for '{}' - using embedded default", name);
        }
    }

    Ok(())
}

const WORKER_OVERRIDE_PATH: &str = ".ralph/prompts/worker.md";
const SCAN_OVERRIDE_PATH: &str = ".ralph/prompts/scan.md";
const TASK_BUILDER_OVERRIDE_PATH: &str = ".ralph/prompts/task_builder.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    /// Show the prompt for phase 1 (planning).
    Phase1,
    /// Show the prompt for phase 2 (implementation). Requires plan text.
    Phase2,
    /// Show the prompt for phase 3 (code review).
    Phase3,
    /// Show the combined single-phase prompt (plan+implement).
    Single,
}

#[derive(Debug, Clone)]
pub struct WorkerPromptOptions {
    /// If None, we will attempt to pick the first todo task from the queue.
    pub task_id: Option<String>,
    pub mode: WorkerMode,
    /// RepoPrompt planning requirement already resolved (flags + config).
    pub repoprompt_plan_required: bool,
    /// RepoPrompt tooling reminder injection already resolved (flags + config).
    pub repoprompt_tool_injection: bool,
    /// Total iteration count to simulate when rendering prompts.
    pub iterations: u8,
    /// 1-based iteration index to simulate when rendering prompts.
    pub iteration_index: u8,

    /// Optional explicit plan file for Phase 2.
    /// If omitted in Phase 2, we try the cached plan at `.ralph/cache/plans/{{TASK_ID}}.md`.
    pub plan_file: Option<PathBuf>,
    /// Optional inline plan override (takes precedence over plan_file/cache).
    pub plan_text: Option<String>,

    /// Print a small header explaining what was selected.
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct ScanPromptOptions {
    pub focus: String,
    pub repoprompt_tool_injection: bool,
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct TaskBuilderPromptOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub repoprompt_tool_injection: bool,
    pub explain: bool,
}

fn worker_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(WORKER_OVERRIDE_PATH).exists() {
        WORKER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

fn scan_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(SCAN_OVERRIDE_PATH).exists() {
        SCAN_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

fn task_builder_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(TASK_BUILDER_OVERRIDE_PATH).exists() {
        TASK_BUILDER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

/// Resolve a task id for worker prompt preview:
/// - If provided explicitly, use it.
/// - Else load queue and pick first todo.
/// - Else error with a clear message.
fn resolve_worker_task_id(resolved: &config::Resolved, task_id: Option<String>) -> Result<String> {
    if let Some(id) = task_id {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            bail!("--task-id was provided but is empty");
        }
        return Ok(trimmed.to_string());
    }

    // Best-effort: mirror runtime selection.
    // Runtime prefers resuming a `doing` task, otherwise the first runnable `todo`.
    if resolved.queue_path.exists() {
        let queue_file = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read {}", resolved.queue_path.display()))?;

        let done_file = if resolved.done_path.exists() {
            Some(
                queue::load_queue(&resolved.done_path)
                    .with_context(|| format!("read {}", resolved.done_path.display()))?,
            )
        } else {
            None
        };

        let options = queue::operations::RunnableSelectionOptions::new(false, true);
        if let Some(idx) =
            queue::operations::select_runnable_task_index(&queue_file, done_file.as_ref(), options)
        {
            if let Some(task) = queue_file.tasks.get(idx) {
                return Ok(task.id.trim().to_string());
            }
        }
    }

    bail!(
        "No doing/todo tasks found to infer a worker task id. Provide --task-id (e.g., RQ-0001) to preview the worker prompt."
    );
}

/// Load plan text for Phase 2 prompt preview.
///
/// NOTE: This function is ONLY used by the `ralph prompt` command for preview/inspection.
/// Actual runtime execution (`ralph run`) extracts the plan directly from Phase 1 output
/// and will error if no plan exists. This function uses a placeholder when missing
/// to allow previewing Phase 2 prompts even when no cached plan exists.
fn load_plan_text_for_phase2(
    repo_root: &Path,
    task_id: &str,
    plan_text: Option<String>,
    plan_file: Option<PathBuf>,
) -> Result<String> {
    if let Some(text) = plan_text {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            bail!("--plan-text was provided but is empty");
        }
        return Ok(trimmed.to_string());
    }

    if let Some(path) = plan_file {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read plan file {}", path.display()))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("Plan file is empty: {}", path.display());
        }
        return Ok(trimmed.to_string());
    }

    // For preview command only: if cache is missing, use placeholder instead of erroring.
    // Runtime execution will still error appropriately since it extracts plan from Phase 1 output.
    match promptflow::read_plan_cache(repo_root, task_id) {
        Ok(plan) => Ok(plan),
        Err(_) => {
            let cache_path = promptflow::plan_cache_path(repo_root, task_id);
            Ok(format!(
                "*No plan file found*\n\nNo plan file was found at {}. Please proceed with implementation based on the task requirements.",
                cache_path.display()
            ))
        }
    }
}

fn load_phase2_final_response_for_phase3(repo_root: &Path, task_id: &str) -> String {
    match promptflow::read_phase2_final_response_cache(repo_root, task_id) {
        Ok(text) => text,
        Err(err) => {
            log::warn!(
                "Phase 2 final response cache unavailable for {}: {}",
                task_id,
                err
            );
            "(Phase 2 final response unavailable; cache missing.)".to_string()
        }
    }
}

pub fn build_worker_prompt(
    resolved: &config::Resolved,
    opts: WorkerPromptOptions,
) -> Result<String> {
    let task_id = resolve_worker_task_id(resolved, opts.task_id)?;
    if opts.iterations == 0 {
        bail!("--iterations must be >= 1");
    }
    if opts.iteration_index == 0 {
        bail!("--iteration-index must be >= 1");
    }
    if opts.iteration_index > opts.iterations {
        bail!(
            "--iteration-index ({}) cannot exceed --iterations ({})",
            opts.iteration_index,
            opts.iterations
        );
    }

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let base_prompt =
        prompts::render_worker_prompt(&template, &task_id, project_type, &resolved.config)?;
    let base_prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &base_prompt, &resolved.config)?;

    let policy = PromptPolicy {
        repoprompt_plan_required: opts.repoprompt_plan_required,
        repoprompt_tool_injection: opts.repoprompt_tool_injection,
    };
    let is_followup = opts.iteration_index > 1;
    let is_final_iteration = opts.iteration_index == opts.iterations;
    let iteration_context = if is_followup {
        prompts::ITERATION_CONTEXT_REFINEMENT
    } else {
        ""
    };
    let iteration_completion_block = if is_final_iteration {
        ""
    } else {
        prompts::ITERATION_COMPLETION_BLOCK
    };
    let phase3_completion_guidance = if is_final_iteration {
        prompts::PHASE3_COMPLETION_GUIDANCE_FINAL
    } else {
        prompts::PHASE3_COMPLETION_GUIDANCE_NONFINAL
    };

    let configured_phases = resolved.config.agent.phases.unwrap_or(2);
    let total_phases = match opts.mode {
        WorkerMode::Phase3 => 3,
        WorkerMode::Single => 1,
        _ => configured_phases.clamp(2, 3),
    };

    let load_completion_checklist = || -> Result<String> {
        let template = prompts::load_completion_checklist(&resolved.repo_root)?;
        prompts::render_completion_checklist(&template, &task_id, &resolved.config)
    };

    let prompt = match opts.mode {
        WorkerMode::Phase1 => {
            let phase1_template = prompts::load_worker_phase1_prompt(&resolved.repo_root)?;
            promptflow::build_phase1_prompt(
                &phase1_template,
                &base_prompt,
                iteration_context,
                &task_id,
                total_phases,
                &policy,
                &resolved.config,
            )?
        }
        WorkerMode::Phase2 => {
            let plan_text = load_plan_text_for_phase2(
                &resolved.repo_root,
                &task_id,
                opts.plan_text,
                opts.plan_file,
            )?;
            if total_phases == 3 {
                let handoff_template = prompts::load_phase2_handoff_checklist(&resolved.repo_root)?;
                let handoff_checklist =
                    prompts::render_phase2_handoff_checklist(&handoff_template, &resolved.config)?;
                let phase2_template =
                    prompts::load_worker_phase2_handoff_prompt(&resolved.repo_root)?;
                promptflow::build_phase2_handoff_prompt(
                    &phase2_template,
                    &base_prompt,
                    &plan_text,
                    &handoff_checklist,
                    iteration_context,
                    iteration_completion_block,
                    &task_id,
                    total_phases,
                    &policy,
                    &resolved.config,
                )?
            } else {
                let completion_checklist = load_completion_checklist()?;
                let phase2_template = prompts::load_worker_phase2_prompt(&resolved.repo_root)?;
                promptflow::build_phase2_prompt(
                    &phase2_template,
                    &base_prompt,
                    &plan_text,
                    &completion_checklist,
                    iteration_context,
                    iteration_completion_block,
                    &task_id,
                    total_phases,
                    &policy,
                    &resolved.config,
                )?
            }
        }
        WorkerMode::Phase3 => {
            let review_template = prompts::load_code_review_prompt(&resolved.repo_root)?;
            let review_body = prompts::render_code_review_prompt(
                &review_template,
                &task_id,
                project_type,
                &resolved.config,
            )?;
            let completion_checklist = load_completion_checklist()?;
            let phase3_template = prompts::load_worker_phase3_prompt(&resolved.repo_root)?;
            let phase2_final_response =
                load_phase2_final_response_for_phase3(&resolved.repo_root, &task_id);
            promptflow::build_phase3_prompt(
                &phase3_template,
                &base_prompt,
                &review_body,
                &phase2_final_response,
                &task_id,
                &completion_checklist,
                iteration_context,
                iteration_completion_block,
                phase3_completion_guidance,
                total_phases,
                &policy,
                &resolved.config,
            )?
        }
        WorkerMode::Single => {
            let completion_checklist = load_completion_checklist()?;
            let single_template = prompts::load_worker_single_phase_prompt(&resolved.repo_root)?;
            promptflow::build_single_phase_prompt(
                &single_template,
                &base_prompt,
                &completion_checklist,
                iteration_context,
                iteration_completion_block,
                &task_id,
                &policy,
                &resolved.config,
            )?
        }
    };

    if !opts.explain {
        return Ok(prompt);
    }

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (worker)\n\n");
    header.push_str(&format!("- task_id: {}\n", task_id));
    header.push_str(&format!(
        "- mode: {}\n",
        match opts.mode {
            WorkerMode::Phase1 => "phase1",
            WorkerMode::Phase2 => "phase2",
            WorkerMode::Phase3 => "phase3",
            WorkerMode::Single => "single",
        }
    ));
    header.push_str(&format!(
        "- repoprompt_plan_required: {}\n",
        opts.repoprompt_plan_required
    ));
    header.push_str(&format!(
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- iteration: {}/{}\n",
        opts.iteration_index, opts.iterations
    ));
    header.push_str(&format!(
        "- worker template source: {}\n",
        worker_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}

pub fn build_scan_prompt(resolved: &config::Resolved, opts: ScanPromptOptions) -> Result<String> {
    let template = prompts::load_scan_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let rendered =
        prompts::render_scan_prompt(&template, &opts.focus, project_type, &resolved.config)?;
    let prompt =
        prompts::wrap_with_repoprompt_requirement(&rendered, opts.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    if !opts.explain {
        return Ok(prompt);
    }

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (scan)\n\n");
    header.push_str(&format!(
        "- focus: {}\n",
        if opts.focus.trim().is_empty() {
            "(none)"
        } else {
            opts.focus.trim()
        }
    ));
    header.push_str(&format!(
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- scan template source: {}\n",
        scan_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}

pub fn build_task_builder_prompt(
    resolved: &config::Resolved,
    opts: TaskBuilderPromptOptions,
) -> Result<String> {
    let request = opts.request.trim();
    if request.is_empty() {
        bail!("Missing request: task builder prompt preview requires a non-empty request.");
    }

    let template = prompts::load_task_builder_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let rendered = prompts::render_task_builder_prompt(
        &template,
        request,
        &opts.hint_tags,
        &opts.hint_scope,
        project_type,
        &resolved.config,
    )?;
    let prompt =
        prompts::wrap_with_repoprompt_requirement(&rendered, opts.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    if !opts.explain {
        return Ok(prompt);
    }

    let mut header = String::new();
    header.push_str("# RALPH PROMPT PREVIEW (task builder)\n\n");
    header.push_str(&format!("- request: {}\n", request));
    header.push_str(&format!(
        "- hint_tags: {}\n",
        if opts.hint_tags.trim().is_empty() {
            "(empty)"
        } else {
            opts.hint_tags.trim()
        }
    ));
    header.push_str(&format!(
        "- hint_scope: {}\n",
        if opts.hint_scope.trim().is_empty() {
            "(empty)"
        } else {
            opts.hint_scope.trim()
        }
    ));
    header.push_str(&format!(
        "- repoprompt_tool_injection: {}\n",
        opts.repoprompt_tool_injection
    ));
    header.push_str(&format!(
        "- task builder template source: {}\n",
        task_builder_template_source(&resolved.repo_root)
    ));
    header.push_str("\n---\n\n");

    Ok(format!("{header}{prompt}"))
}

#[cfg(test)]
mod tests {
    use super::resolve_worker_task_id;
    use crate::config::Resolved;
    use crate::contracts::{Config, QueueFile, Task, TaskPriority, TaskStatus};
    use crate::queue;
    use tempfile::TempDir;

    fn make_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {id}"),
            status,
            priority: TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["test".to_string()],
            plan: vec!["plan".to_string()],
            notes: vec![],
            request: Some("request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        }
    }

    fn make_resolved(temp: &TempDir) -> Resolved {
        let repo_root = temp.path().to_path_buf();
        let queue_path = repo_root.join("queue.json");
        let done_path = repo_root.join("done.json");
        Resolved {
            config: Config::default(),
            repo_root,
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn resolve_worker_task_id_trims_explicit_task_id() {
        let temp = TempDir::new().expect("tempdir");
        let resolved = make_resolved(&temp);
        let id = resolve_worker_task_id(&resolved, Some("  RQ-0009  ".to_string()))
            .expect("should trim");
        assert_eq!(id, "RQ-0009");
    }

    #[test]
    fn resolve_worker_task_id_prefers_doing() {
        let temp = TempDir::new().expect("tempdir");
        let resolved = make_resolved(&temp);
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", TaskStatus::Todo),
                make_task("RQ-0002", TaskStatus::Doing),
            ],
        };
        queue::save_queue(&resolved.queue_path, &queue).expect("save queue");

        let id = resolve_worker_task_id(&resolved, None).expect("should resolve doing");
        assert_eq!(id, "RQ-0002");
    }

    #[test]
    fn resolve_worker_task_id_returns_runnable_todo() {
        let temp = TempDir::new().expect("tempdir");
        let resolved = make_resolved(&temp);

        let mut todo = make_task("RQ-0003", TaskStatus::Todo);
        todo.depends_on = vec!["RQ-0002".to_string()];

        let queue = QueueFile {
            version: 1,
            tasks: vec![todo],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![make_task("RQ-0002", TaskStatus::Done)],
        };
        queue::save_queue(&resolved.queue_path, &queue).expect("save queue");
        queue::save_queue(&resolved.done_path, &done).expect("save done");

        let id = resolve_worker_task_id(&resolved, None).expect("should resolve todo");
        assert_eq!(id, "RQ-0003");
    }
}
