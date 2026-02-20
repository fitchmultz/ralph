//! Conflict resolution for merge runner.
//!
//! Responsibilities:
//! - Resolve merge conflicts using an AI runner when enabled.
//! - Validate queue/done files in workspace before committing resolved conflicts.
//!
//! Not handled here:
//! - High-level merge orchestration (see `mod.rs`).
//! - Git command execution (see `git_ops.rs`).

use crate::commands::run::parallel::merge_runner::git_ops::{git_run, git_status, push_branch};
use crate::commands::run::parallel::path_map::map_resolved_path_into_workspace;
use crate::contracts::{MergeRunnerConfig, QueueFile, Runner};
use crate::{git, promptflow, prompts, queue};
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Resolve merge conflicts in a workspace using AI runner.
pub(crate) fn resolve_conflicts(
    resolved: &crate::config::Resolved,
    pr: &git::PrInfo,
    workspace_root: &Path,
    task_id: &str,
    merge_runner: &MergeRunnerConfig,
) -> Result<()> {
    let workspace_path = workspace_root.join(task_id);

    // Ensure workspace exists (clone on demand if missing)
    git::ensure_workspace_exists(&resolved.repo_root, &workspace_path, &pr.head)
        .with_context(|| format!("ensure workspace exists at {}", workspace_path.display()))?;

    // Run the checkout and merge preparation
    let merge_outcome = prepare_and_merge(&workspace_path, &pr.head, &pr.base)?;

    // Check for actual conflict files
    let conflicts = conflict_files(&workspace_path)?;

    if conflicts.is_empty() {
        // No conflicts detected - verify this matches the merge outcome
        match merge_outcome {
            git::error::GitMergeOutcome::Clean => return Ok(()),
            git::error::GitMergeOutcome::Conflicts { stderr } => {
                // Merge reported conflicts but no unmerged files found - this is suspicious
                bail!(
                    "Merge reported conflicts but no unmerged files detected. stderr: {}",
                    stderr
                );
            }
        }
    }

    let template = prompts::load_merge_conflict_prompt(&workspace_path)?;
    let prompt = promptflow::build_merge_conflict_prompt(&template, &conflicts, &resolved.config)?;
    let prompt = prompts::wrap_with_instruction_files(&workspace_path, &prompt, &resolved.config)?;

    run_merge_runner_prompt_for_conflicts(resolved, merge_runner, &workspace_path, &prompt)?;

    let remaining = conflict_files(&workspace_path)?;
    if !remaining.is_empty() {
        bail!(
            "Merge conflicts remain after AI resolution: {}",
            remaining.join(", ")
        );
    }

    // Validate queue/done files in workspace before committing
    validate_queue_done_in_workspace(resolved, &workspace_path)
        .context("validate queue/done JSON in merge workspace before commit")?;

    git_run(&workspace_path, &["add", "-A"])?;
    let status = git_status(&workspace_path)?;
    if status.trim().is_empty() {
        bail!("No changes staged after conflict resolution.");
    }
    let message = format!("Resolve merge conflicts for {}", task_id);
    git_run(&workspace_path, &["commit", "-m", &message])?;
    push_branch(&workspace_path)?;
    Ok(())
}

/// Validate queue and done files in the workspace clone before committing.
///
/// Maps the resolved queue/done paths into the workspace, then validates them
/// using JSON repair + semantic validation. Missing done file is allowed.
/// On validation failure, returns an error that prevents commit/push.
pub(crate) fn validate_queue_done_in_workspace(
    resolved: &crate::config::Resolved,
    workspace_repo_root: &Path,
) -> Result<()> {
    let workspace_queue_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_repo_root,
        &resolved.queue_path,
        "queue",
    )?;

    let workspace_done_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        workspace_repo_root,
        &resolved.done_path,
        "done",
    )?;

    // Queue must exist - we can't validate what we can't read
    if !workspace_queue_path.exists() {
        bail!(
            "Queue file not found in workspace: {}",
            workspace_queue_path.display()
        );
    }

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Load done file if it exists
    let done_file: Option<QueueFile> = if workspace_done_path.exists() {
        Some(
            queue::load_queue_with_repair(&workspace_done_path).with_context(|| {
                format!("load done file from {}", workspace_done_path.display())
            })?,
        )
    } else {
        None
    };

    // Validate queue (with optional done file)
    let (_queue, warnings) = queue::load_queue_with_repair_and_validate(
        &workspace_queue_path,
        done_file.as_ref(),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .with_context(|| format!("validate queue file at {}", workspace_queue_path.display()))?;

    // Log any non-blocking warnings
    queue::log_warnings(&warnings);

    Ok(())
}

/// Get list of conflicted files from git status.
pub(crate) fn conflict_files(repo_root: &Path) -> Result<Vec<String>> {
    let output =
        super::git_ops::git_output(repo_root, &["diff", "--name-only", "--diff-filter=U"])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

/// Prepare workspace for merge by fetching, checking out head branch, and merging base.
/// Returns the merge outcome (clean or conflicts) so callers can decide how to proceed.
pub(crate) fn prepare_and_merge(
    workspace_path: &Path,
    head_branch: &str,
    base_branch: &str,
) -> Result<git::error::GitMergeOutcome> {
    git_run(workspace_path, &["fetch", "origin", "--prune"])?;
    git_run(
        workspace_path,
        &[
            "checkout",
            "-B",
            head_branch,
            &format!("origin/{}", head_branch),
        ],
    )?;

    let merge_target = format!("origin/{}", base_branch);
    let outcome = git::error::git_merge_allow_conflicts(workspace_path, &merge_target)?;
    Ok(outcome)
}

/// Run the merge runner prompt for conflict resolution.
///
/// This invokes the configured AI runner with the conflict resolution prompt.
/// Unlike the deleted completion.rs version, this does NOT handle completion signals
/// since the merge-agent architecture handles task finalization directly.
fn run_merge_runner_prompt_for_conflicts(
    resolved: &crate::config::Resolved,
    merge_runner: &MergeRunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    let runner_type = merge_runner
        .runner
        .clone()
        .unwrap_or_else(|| resolved.config.agent.runner.clone().unwrap_or_default());
    let model = merge_runner
        .model
        .clone()
        .or(resolved.config.agent.model.clone());

    // Create a temporary prompt file
    let prompt_file = tempfile::Builder::new()
        .prefix("ralph_merge_conflict_")
        .suffix(".md")
        .tempfile_in(workspace_path)
        .context("create temp prompt file for merge conflict")?;
    std::fs::write(&prompt_file, prompt).context("write merge conflict prompt file")?;

    // Get the binary path for the runner
    let binary = match runner_type {
        Runner::Codex => resolved
            .config
            .agent
            .codex_bin
            .as_deref()
            .unwrap_or("codex"),
        Runner::Opencode => resolved
            .config
            .agent
            .opencode_bin
            .as_deref()
            .unwrap_or("opencode"),
        Runner::Gemini => resolved
            .config
            .agent
            .gemini_bin
            .as_deref()
            .unwrap_or("gemini"),
        Runner::Claude => resolved
            .config
            .agent
            .claude_bin
            .as_deref()
            .unwrap_or("claude"),
        Runner::Cursor => resolved
            .config
            .agent
            .cursor_bin
            .as_deref()
            .unwrap_or("agent"),
        Runner::Kimi => resolved.config.agent.kimi_bin.as_deref().unwrap_or("kimi"),
        Runner::Pi => resolved.config.agent.pi_bin.as_deref().unwrap_or("pi"),
        Runner::Plugin(name) => {
            bail!(
                "Plugin runner '{}' not supported for merge conflict resolution",
                name
            );
        }
    };

    // Build a simple command to run the runner with the prompt
    let model_str = model.as_ref().map(|m| m.as_str()).unwrap_or("auto");
    let prompt_path = prompt_file.path().display().to_string();

    let mut cmd = Command::new(binary);
    cmd.current_dir(workspace_path);
    cmd.arg("run");
    cmd.arg("--model").arg(model_str);
    cmd.arg("--output-format").arg("stream-json");
    cmd.arg("--full-auto");
    cmd.arg(&prompt_path);

    // Execute the runner
    let status = cmd.status().context("execute merge conflict runner")?;
    if !status.success() {
        bail!(
            "Merge conflict runner failed with exit code {:?}",
            status.code()
        );
    }

    Ok(())
}
