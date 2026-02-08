//! Merge runner for parallel PRs and AI-based conflict resolution.
//!
//! Responsibilities:
//! - Consume PR work items and attempt merges based on configured policy.
//! - Validate PR head branch names match the expected naming convention.
//! - Resolve merge conflicts using an AI runner when enabled.
//! - Apply completion signals on the base branch after merge.
//! - Emit merge results for downstream cleanup.
//!
//! Not handled here:
//! - Worker orchestration or task selection (see `parallel/mod.rs`).
//! - PR creation (see `git/pr.rs`).
//! - Blocker persistence (handled by supervisor in `parallel/mod.rs`).
//!
//! Invariants/assumptions:
//! - PRs originate from branches named with the configured prefix.
//! - Workspaces remain available until merge completion or failure.
//! - Each work item carries a trusted task_id (from queue/state, not derived from PR head).

use crate::commands::run::PhaseType;
use crate::commands::run::parallel::path_map::map_resolved_path_into_workspace;
use crate::config;
use crate::contracts::{
    ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, QueueFile, RunnerCliOptionsPatch,
};
use crate::{completions, git, outpututil, productivity, promptflow, prompts, queue, runner};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

/// RAII guard that ensures cleanup of the .base-sync workspace on any exit path.
///
/// This guard performs best-effort cleanup of the ephemeral .base-sync directory
/// when dropped. It validates the path before deletion to prevent accidental
/// removal of unexpected directories.
struct BaseSyncWorkspaceCleanupGuard {
    workspace_root: PathBuf,
    base_sync_path: PathBuf,
}

impl BaseSyncWorkspaceCleanupGuard {
    fn new(workspace_root: &Path, base_sync_path: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            base_sync_path: base_sync_path.to_path_buf(),
        }
    }
}

impl Drop for BaseSyncWorkspaceCleanupGuard {
    fn drop(&mut self) {
        // Defense-in-depth: only delete a directory literally named ".base-sync"
        // that lives under the configured workspace root.
        let is_base_sync = self
            .base_sync_path
            .file_name()
            .is_some_and(|n| n == std::ffi::OsStr::new(".base-sync"));
        if !is_base_sync || !self.base_sync_path.starts_with(&self.workspace_root) {
            log::warn!(
                "Refusing to remove unexpected base-sync path: {} (workspace_root={})",
                self.base_sync_path.display(),
                self.workspace_root.display()
            );
            return;
        }

        match std::fs::remove_dir_all(&self.base_sync_path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => log::warn!(
                "Failed to remove base-sync workspace {}: {}",
                self.base_sync_path.display(),
                err
            ),
        }
    }
}

/// Work item for the merge runner containing trusted task_id, PR info, and workspace metadata.
#[derive(Debug, Clone)]
pub(crate) struct MergeWorkItem {
    pub task_id: String,
    pub pr: git::PrInfo,
    /// Optional path to the worker workspace for completion signal lookup.
    pub workspace_path: Option<PathBuf>,
}

pub(crate) enum MergeQueueSource {
    AsCreated(mpsc::Receiver<MergeWorkItem>),
    AfterAll(Vec<MergeWorkItem>),
}

#[derive(Debug, Clone)]
pub(crate) struct MergeResult {
    pub task_id: String,
    pub merged: bool,
    /// Human-readable reason this PR should be skipped by the supervisor.
    /// When Some(_), merged must be false and queue/done/productivity bytes must be None.
    pub merge_blocker: Option<String>,
    pub queue_bytes: Option<Vec<u8>>,
    pub done_bytes: Option<Vec<u8>>,
    pub productivity_bytes: Option<Vec<u8>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_merge_runner(
    resolved: &config::Resolved,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    pr_queue: MergeQueueSource,
    workspace_root: &Path,
    delete_branch: bool,
    merge_result_tx: mpsc::Sender<MergeResult>,
    merge_stop: Arc<AtomicBool>,
) -> Result<()> {
    let handle_one = |work_item: MergeWorkItem| -> Result<()> {
        if merge_stop.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(result) = handle_work_item(
            resolved,
            work_item,
            merge_method,
            conflict_policy,
            merge_runner.clone(),
            retries,
            workspace_root,
            delete_branch,
            &merge_stop,
        )? {
            let _ = merge_result_tx.send(result);
        }

        Ok(())
    };

    match pr_queue {
        MergeQueueSource::AsCreated(rx) => {
            for work_item in rx.iter() {
                handle_one(work_item)?;
            }
        }
        MergeQueueSource::AfterAll(work_items) => {
            for work_item in work_items {
                handle_one(work_item)?;
            }
        }
    }

    Ok(())
}

/// Validates that the PR head matches the expected branch naming convention.
///
/// Expected format: `{branch_prefix}{task_id}`
/// Returns `Ok(())` if valid, or an error message if invalid.
fn validate_pr_head(branch_prefix: &str, task_id: &str, head: &str) -> Result<(), String> {
    let expected = format!("{}{}", branch_prefix, task_id);
    let trimmed_head = head.trim();

    if trimmed_head != expected {
        return Err(format!(
            "head mismatch: expected '{}', got '{}'",
            expected, trimmed_head
        ));
    }

    // Additional safety: reject path separators and parent directory references
    if task_id.contains('/') || task_id.contains("..") {
        return Err(format!(
            "invalid task_id '{}': contains path separators",
            task_id
        ));
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_work_item(
    resolved: &config::Resolved,
    work_item: MergeWorkItem,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    workspace_root: &Path,
    delete_branch: bool,
    merge_stop: &AtomicBool,
) -> Result<Option<MergeResult>> {
    if merge_stop.load(Ordering::SeqCst) {
        return Ok(None);
    }

    let branch_prefix = resolved
        .config
        .parallel
        .branch_prefix
        .clone()
        .unwrap_or_else(|| "ralph/".to_string());

    // Validate the PR head matches expected naming convention
    if let Err(reason) = validate_pr_head(&branch_prefix, &work_item.task_id, &work_item.pr.head) {
        log::warn!(
            "Skipping PR {} due to head mismatch for task {}: {}. \
             This usually means the branch_prefix config changed or the PR head was renamed.",
            work_item.pr.number,
            work_item.task_id,
            reason
        );
        return Ok(Some(MergeResult {
            task_id: work_item.task_id,
            merged: false,
            merge_blocker: Some(reason),
            queue_bytes: None,
            done_bytes: None,
            productivity_bytes: None,
        }));
    }

    let merged = merge_pr_with_retries(
        resolved,
        &work_item.pr,
        merge_method,
        conflict_policy,
        merge_runner,
        retries,
        workspace_root,
        &work_item.task_id,
        delete_branch,
        merge_stop,
    )?;

    if merged {
        let sync = apply_completion_and_collect_bytes(
            resolved,
            workspace_root,
            work_item.workspace_path.as_deref(),
            &work_item.pr.base,
            &work_item.task_id,
        )?;
        Ok(Some(MergeResult {
            task_id: work_item.task_id,
            merged: true,
            merge_blocker: None,
            queue_bytes: Some(sync.queue_bytes),
            done_bytes: sync.done_bytes,
            productivity_bytes: sync.productivity_bytes,
        }))
    } else {
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_pr_with_retries(
    resolved: &config::Resolved,
    pr: &git::PrInfo,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    workspace_root: &Path,
    task_id: &str,
    delete_branch: bool,
    merge_stop: &AtomicBool,
) -> Result<bool> {
    let mut attempts = 0u8;
    loop {
        if merge_stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        attempts += 1;
        let status = git::pr_merge_status(&resolved.repo_root, pr.number)?;
        if status.is_draft {
            log::info!("Skipping draft PR {} (not eligible for merge).", pr.number);
            return Ok(false);
        }
        match status.merge_state {
            git::MergeState::Clean => {
                if let Err(err) =
                    git::merge_pr(&resolved.repo_root, pr.number, merge_method, delete_branch)
                {
                    log::warn!("Merge failed for PR {}: {:#}", pr.number, err);
                    return Ok(false);
                }
                return Ok(true);
            }
            git::MergeState::Dirty => match conflict_policy {
                ConflictPolicy::AutoResolve => {
                    resolve_conflicts(resolved, pr, workspace_root, task_id, &merge_runner)?;
                }
                ConflictPolicy::RetryLater => {
                    if attempts >= retries {
                        return Ok(false);
                    }
                    sleep_backoff(attempts);
                    continue;
                }
                ConflictPolicy::Reject => return Ok(false),
            },
            git::MergeState::Other(status) => {
                log::info!(
                    "PR {} merge state is {}; retrying ({}/{})",
                    pr.number,
                    status,
                    attempts,
                    retries
                );
                if attempts >= retries {
                    return Ok(false);
                }
                sleep_backoff(attempts);
            }
        }

        if attempts >= retries {
            return Ok(false);
        }
    }
}

fn resolve_conflicts(
    resolved: &config::Resolved,
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

    run_merge_runner_prompt(resolved, merge_runner, &workspace_path, &prompt)?;

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
fn validate_queue_done_in_workspace(
    resolved: &config::Resolved,
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

#[derive(Debug)]
struct QueueSyncBytes {
    queue_bytes: Vec<u8>,
    done_bytes: Option<Vec<u8>>,
    productivity_bytes: Option<Vec<u8>>,
}

fn apply_completion_and_collect_bytes(
    resolved: &config::Resolved,
    workspace_root: &Path,
    workspace_path: Option<&Path>,
    base_branch: &str,
    task_id: &str,
) -> Result<QueueSyncBytes> {
    let base_sync_path = workspace_root.join(".base-sync");
    let _base_sync_cleanup = BaseSyncWorkspaceCleanupGuard::new(workspace_root, &base_sync_path);

    git::ensure_workspace_exists(&resolved.repo_root, &base_sync_path, base_branch)
        .with_context(|| format!("ensure base-sync workspace at {}", base_sync_path.display()))?;

    let workspace_queue_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        &base_sync_path,
        &resolved.queue_path,
        "queue",
    )?;
    let workspace_done_path = map_resolved_path_into_workspace(
        &resolved.repo_root,
        &base_sync_path,
        &resolved.done_path,
        "done",
    )?;

    let mut workspace_resolved = resolved.clone();
    workspace_resolved.repo_root = base_sync_path.clone();
    workspace_resolved.queue_path = workspace_queue_path.clone();
    workspace_resolved.done_path = workspace_done_path.clone();
    if workspace_resolved.project_config_path.is_some() {
        workspace_resolved.project_config_path = Some(base_sync_path.join(".ralph/config.json"));
    }

    ensure_completion_signal_in_workspace(
        &base_sync_path,
        workspace_root,
        workspace_path,
        task_id,
    )?;
    let applied =
        crate::commands::run::apply_phase3_completion_signal(&workspace_resolved, task_id)?;
    if applied.is_none() {
        bail!(
            "apply_phase3_completion_signal returned None for {} despite ensure succeeding; this is an unexpected state.",
            task_id
        );
    }

    let task_title =
        task_title_from_queue_done_paths(&workspace_queue_path, &workspace_done_path, task_id)?
            .unwrap_or_else(|| "Parallel completion".to_string());

    let cache_dir = workspace_resolved.repo_root.join(".ralph").join("cache");
    if let Err(err) = productivity::record_task_completion_by_id(task_id, &task_title, &cache_dir) {
        log::debug!(
            "Failed to record productivity for {} in base-sync workspace: {}",
            task_id,
            err
        );
    }

    let status = git::status_porcelain(&base_sync_path)?;
    if !status.trim().is_empty() {
        let message = outpututil::format_task_commit_message(task_id, &task_title);
        match git::commit_all(&base_sync_path, &message) {
            Ok(()) => {
                push_branch(&base_sync_path)?;
            }
            Err(git::GitError::NoChangesToCommit) => {}
            Err(err) => return Err(err.into()),
        }
    }

    let queue_bytes = std::fs::read(&workspace_queue_path)
        .with_context(|| format!("read queue bytes from {}", workspace_queue_path.display()))?;
    let done_bytes =
        if workspace_done_path.exists() {
            Some(std::fs::read(&workspace_done_path).with_context(|| {
                format!("read done bytes from {}", workspace_done_path.display())
            })?)
        } else {
            None
        };
    let productivity_path = base_sync_path
        .join(".ralph")
        .join("cache")
        .join("productivity.json");
    let productivity_bytes = if productivity_path.exists() {
        Some(std::fs::read(&productivity_path).with_context(|| {
            format!(
                "read productivity bytes from {}",
                productivity_path.display()
            )
        })?)
    } else {
        None
    };

    Ok(QueueSyncBytes {
        queue_bytes,
        done_bytes,
        productivity_bytes,
    })
}

/// Ensures a completion signal exists for the task, copying it into base-sync when found.
///
/// Behavior:
/// 1. If signal exists in base_sync_root, return Ok(())
/// 2. Else check the explicit workspace_path (if provided) and the default
///    `{workspace_root}/{task_id}` path; copy the signal into base-sync if found.
/// 3. Else auto-finalize with a minimal completion signal.
fn ensure_completion_signal_in_workspace(
    base_sync_root: &Path,
    workspace_root: &Path,
    workspace_path: Option<&Path>,
    task_id: &str,
) -> Result<()> {
    if completions::read_completion_signal(base_sync_root, task_id)?.is_some() {
        return Ok(());
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = workspace_path {
        candidates.push(path.to_path_buf());
    }
    let default_path = workspace_root.join(task_id.trim());
    if workspace_path
        .map(|path| path != default_path)
        .unwrap_or(true)
    {
        candidates.push(default_path);
    }

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if let Some(signal) = completions::read_completion_signal(&candidate, task_id)? {
            completions::write_completion_signal(base_sync_root, &signal)?;
            log::info!(
                "Copied completion signal for {} from workspace {} into base-sync.",
                task_id,
                candidate.display()
            );
            return Ok(());
        }
    }

    let fallback = completions::CompletionSignal {
        task_id: task_id.to_string(),
        status: crate::contracts::TaskStatus::Done,
        notes: vec![format!(
            "[ralph] Auto-finalized after merge: completion signal missing for {}.",
            task_id
        )],
        runner_used: None,
        model_used: None,
    };
    completions::write_completion_signal(base_sync_root, &fallback)?;
    log::warn!(
        "Completion signal missing for {}; auto-finalizing as done.",
        task_id
    );
    Ok(())
}

fn task_title_from_queue_done_paths(
    queue_path: &Path,
    done_path: &Path,
    task_id: &str,
) -> Result<Option<String>> {
    let queue_file = queue::load_queue(queue_path)?;
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    let done_file = queue::load_queue_or_default(done_path)?;
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    Ok(None)
}

fn run_merge_runner_prompt(
    resolved: &config::Resolved,
    merge_runner: &MergeRunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    let settings = runner::resolve_agent_settings(
        merge_runner.runner.clone(),
        merge_runner.model.clone(),
        merge_runner.reasoning_effort,
        &RunnerCliOptionsPatch::default(),
        None,
        &resolved.config.agent,
    )?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    runner::run_prompt(
        settings.runner.clone(),
        workspace_path,
        bins,
        settings.model.clone(),
        settings.reasoning_effort,
        settings.runner_cli,
        prompt,
        None,
        resolved.config.agent.claude_permission_mode,
        None,
        runner::OutputStream::Terminal,
        PhaseType::Implementation,
        None,
        None,
    )
    .map_err(|err| anyhow::anyhow!("Merge runner failed: {:#}", err))?;

    Ok(())
}

fn conflict_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = git_output(repo_root, &["diff", "--name-only", "--diff-filter=U"])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

/// Prepare workspace for merge by fetching, checking out head branch, and merging base.
/// Returns the merge outcome (clean or conflicts) so callers can decide how to proceed.
fn prepare_and_merge(
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

fn git_status(repo_root: &Path) -> Result<String> {
    git_output(repo_root, &["status", "--porcelain"])
}

fn push_branch(repo_root: &Path) -> Result<()> {
    git::push_upstream_with_rebase(repo_root)
        .context("push branch to upstream (auto-rebase on rejection)")
}

fn git_run(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = git::error::git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(())
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = git::error::git_base_command(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn sleep_backoff(attempt: u8) {
    let ms = 500_u64.saturating_mul(attempt as u64);
    thread::sleep(Duration::from_millis(ms));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn validate_pr_head_accepts_exact_match() {
        assert!(validate_pr_head("ralph/", "RQ-0001", "ralph/RQ-0001").is_ok());
        assert!(validate_pr_head("ralph/", "RQ-0001", " ralph/RQ-0001 ").is_ok());
    }

    #[test]
    fn validate_pr_head_rejects_prefix_mismatch() {
        let err = validate_pr_head("ralph/", "RQ-0001", "feature/RQ-0001")
            .expect_err("expected mismatch error");
        assert!(err.contains("expected 'ralph/RQ-0001'"));
    }

    #[test]
    fn validate_pr_head_rejects_task_id_with_path_separators() {
        let err = validate_pr_head("ralph/", "RQ/0001", "ralph/RQ/0001")
            .expect_err("expected path separator error");
        assert!(err.contains("invalid task_id"));
    }

    #[test]
    fn validate_pr_head_rejects_task_id_with_parent_reference() {
        let err = validate_pr_head("ralph/", "RQ-..", "ralph/RQ-..")
            .expect_err("expected parent reference error");
        assert!(err.contains("invalid task_id"));
    }

    /// Setup a test scenario with a bare remote and two branches that can be merged.
    /// Returns (remote_dir, author_dir, workspace_dir) so the remote path stays alive.
    fn setup_merge_test() -> (TempDir, TempDir, TempDir) {
        // Create directories: remote (bare), author repo, workspace repo
        let remote_dir = TempDir::new().unwrap();
        let author_dir = TempDir::new().unwrap();
        let workspace_dir = TempDir::new().unwrap();

        // Initialize bare remote
        git_test::init_bare_repo(remote_dir.path()).unwrap();

        // Setup author repo with remote
        git_test::init_repo(author_dir.path()).unwrap();
        git_test::add_remote(author_dir.path(), "origin", remote_dir.path()).unwrap();

        // Create initial commit and push to main
        fs::write(author_dir.path().join("README.md"), "# Initial").unwrap();
        git_test::commit_all(author_dir.path(), "Initial commit").unwrap();
        git_test::git_run(author_dir.path(), &["push", "-u", "origin", "HEAD:main"]).unwrap();

        (remote_dir, author_dir, workspace_dir)
    }

    /// Clone from the bare remote into the workspace directory.
    fn clone_from_remote(remote_dir: &Path, workspace_dir: &Path) -> Result<()> {
        git_test::clone_repo(remote_dir, workspace_dir)
    }

    #[test]
    fn merge_runner_clean_merge_succeeds() {
        let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

        // Create a feature branch from main
        git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
        fs::write(author_dir.path().join("feature.txt"), "feature content").unwrap();
        git_test::commit_all(author_dir.path(), "Add feature").unwrap();
        git_test::push_branch(author_dir.path(), "feature").unwrap();

        // Go back to main and add a non-conflicting commit
        git_test::git_run(author_dir.path(), &["checkout", "main"]).unwrap();
        fs::write(author_dir.path().join("main.txt"), "main content").unwrap();
        git_test::commit_all(author_dir.path(), "Add main content").unwrap();
        git_test::push_branch(author_dir.path(), "main").unwrap();

        // Clone workspace from the bare remote
        clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
        git_test::configure_user(workspace_dir.path()).unwrap();

        // Run prepare_and_merge
        let outcome = prepare_and_merge(workspace_dir.path(), "feature", "main").unwrap();

        // Should be clean merge
        assert!(
            matches!(outcome, git::error::GitMergeOutcome::Clean),
            "Expected clean merge outcome"
        );

        // No conflicts should be detected
        let conflicts = conflict_files(workspace_dir.path()).unwrap();
        assert!(
            conflicts.is_empty(),
            "Expected no conflicts, got: {:?}",
            conflicts
        );
    }

    #[test]
    fn merge_runner_conflicted_merge_continues() {
        let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

        // Create a feature branch and modify a file
        git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
        fs::write(author_dir.path().join("shared.txt"), "feature version").unwrap();
        git_test::commit_all(author_dir.path(), "Feature change").unwrap();
        git_test::push_branch(author_dir.path(), "feature").unwrap();

        // Go back to main and modify the same file differently
        git_test::git_run(author_dir.path(), &["checkout", "main"]).unwrap();
        fs::write(author_dir.path().join("shared.txt"), "main version").unwrap();
        git_test::commit_all(author_dir.path(), "Main change").unwrap();
        git_test::push_branch(author_dir.path(), "main").unwrap();

        // Clone workspace from the bare remote
        clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
        git_test::configure_user(workspace_dir.path()).unwrap();

        // Run prepare_and_merge
        let outcome = prepare_and_merge(workspace_dir.path(), "feature", "main").unwrap();

        // Should report conflicts
        assert!(
            matches!(outcome, git::error::GitMergeOutcome::Conflicts { .. }),
            "Expected conflict outcome"
        );

        // Conflicts should be detected
        let conflicts = conflict_files(workspace_dir.path()).unwrap();
        assert_eq!(
            conflicts,
            vec!["shared.txt"],
            "Expected conflict in shared.txt"
        );
    }

    #[test]
    fn merge_runner_nonexistent_target_fails() {
        let (remote_dir, author_dir, workspace_dir) = setup_merge_test();

        // Create and push a feature branch
        git_test::git_run(author_dir.path(), &["checkout", "-b", "feature"]).unwrap();
        fs::write(author_dir.path().join("feature.txt"), "feature").unwrap();
        git_test::commit_all(author_dir.path(), "Feature commit").unwrap();
        git_test::push_branch(author_dir.path(), "feature").unwrap();

        // Clone workspace from the bare remote
        clone_from_remote(remote_dir.path(), workspace_dir.path()).unwrap();
        git_test::configure_user(workspace_dir.path()).unwrap();

        // Try to merge a non-existent branch (origin/nonexistent)
        // This will produce exit code 1 with "not something we can merge" in stderr,
        // which our helper treats as Conflicts. We then verify that conflict_files
        // returns empty, which triggers the error path in resolve_conflicts.
        let outcome = prepare_and_merge(workspace_dir.path(), "feature", "nonexistent").unwrap();

        // The merge returns Conflicts outcome for exit code 1
        assert!(
            matches!(outcome, git::error::GitMergeOutcome::Conflicts { .. }),
            "Expected conflict outcome for non-existent branch"
        );

        // But there are no actual conflict files
        let conflicts = conflict_files(workspace_dir.path()).unwrap();
        assert!(
            conflicts.is_empty(),
            "Non-existent branch should not produce conflict files"
        );

        // The resolve_conflicts function would detect this mismatch and error.
        // This verifies we don't mask real merge failures.
    }

    // Tests for workspace queue/done validation (RQ-0561)

    use crate::contracts::{Config, Task, TaskStatus};
    use std::collections::HashMap;

    fn build_test_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["file.rs".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: if status == TaskStatus::Done {
                Some("2026-01-18T00:00:00Z".to_string())
            } else {
                None
            },
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    fn build_test_resolved(
        original_repo_root: &Path,
        queue_path: PathBuf,
        done_path: PathBuf,
    ) -> config::Resolved {
        config::Resolved {
            config: Config::default(),
            repo_root: original_repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    fn save_queue_file(path: &Path, queue: &QueueFile) {
        let json = serde_json::to_string_pretty(queue).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, json).unwrap();
    }

    #[test]
    fn validate_queue_done_missing_done_file_allowed() {
        let original_repo = TempDir::new().unwrap();
        let workspace_repo = TempDir::new().unwrap();

        let ralph_dir = original_repo.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        // Create valid queue in workspace
        let workspace_ralph = workspace_repo.path().join(".ralph");
        fs::create_dir_all(&workspace_ralph).unwrap();
        let workspace_queue = workspace_ralph.join("queue.json");

        let valid_queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
        };
        save_queue_file(&workspace_queue, &valid_queue);
        // Note: no done.json in workspace

        let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

        let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
        assert!(
            result.is_ok(),
            "Missing done file should be allowed: {:?}",
            result
        );
    }

    #[test]
    fn validate_queue_done_invalid_json_in_queue_rejected() {
        let original_repo = TempDir::new().unwrap();
        let workspace_repo = TempDir::new().unwrap();

        let ralph_dir = original_repo.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        // Create invalid JSON in workspace queue
        let workspace_ralph = workspace_repo.path().join(".ralph");
        fs::create_dir_all(&workspace_ralph).unwrap();
        let workspace_queue = workspace_ralph.join("queue.json");
        fs::write(&workspace_queue, "not valid json").unwrap();

        let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

        let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
        assert!(result.is_err(), "Invalid JSON in queue should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("validate queue file") || err_msg.contains("parse"),
            "Error should indicate validation failure: {}",
            err_msg
        );
    }

    #[test]
    fn validate_queue_done_invalid_json_in_done_rejected() {
        let original_repo = TempDir::new().unwrap();
        let workspace_repo = TempDir::new().unwrap();

        let ralph_dir = original_repo.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let workspace_ralph = workspace_repo.path().join(".ralph");
        fs::create_dir_all(&workspace_ralph).unwrap();

        // Valid queue
        let workspace_queue = workspace_ralph.join("queue.json");
        let valid_queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
        };
        save_queue_file(&workspace_queue, &valid_queue);

        // Invalid done file
        let workspace_done = workspace_ralph.join("done.json");
        fs::write(&workspace_done, "not valid json").unwrap();

        let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

        let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
        assert!(result.is_err(), "Invalid JSON in done should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("load done file") || err_msg.contains("parse"),
            "Error should indicate done file failure: {}",
            err_msg
        );
    }

    #[test]
    fn validate_queue_done_duplicate_ids_rejected() {
        let original_repo = TempDir::new().unwrap();
        let workspace_repo = TempDir::new().unwrap();

        let ralph_dir = original_repo.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let workspace_ralph = workspace_repo.path().join(".ralph");
        fs::create_dir_all(&workspace_ralph).unwrap();

        // Queue with RQ-0001
        let workspace_queue = workspace_ralph.join("queue.json");
        let queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
        };
        save_queue_file(&workspace_queue, &queue);

        // Done file also with RQ-0001 (duplicate!)
        let workspace_done = workspace_ralph.join("done.json");
        let done = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Done)],
        };
        save_queue_file(&workspace_done, &done);

        let resolved = build_test_resolved(original_repo.path(), queue_path, done_path);

        let result = validate_queue_done_in_workspace(&resolved, workspace_repo.path());
        assert!(result.is_err(), "Duplicate IDs should be rejected");
        let err = result.unwrap_err();
        // Check the full error chain since the message is in a context layer
        let err_chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
        let full_error = err_chain.join(" | ");
        assert!(
            full_error.contains("Duplicate task ID detected across queue and done"),
            "Error chain should mention duplicate ID: {}",
            full_error
        );
    }

    #[test]
    fn apply_completion_and_collect_bytes_updates_queue_and_clears_signal() -> Result<()> {
        let (_remote_dir, author_dir, workspace_dir) = setup_merge_test();

        // Ensure we are on main branch locally
        git_test::git_run(author_dir.path(), &["checkout", "-B", "main"])?;

        // Create queue/done files in the author repo
        let ralph_dir = author_dir.path().join(".ralph");
        fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![],
        };
        save_queue_file(&queue_path, &queue);
        save_queue_file(&done_path, &done);

        git_test::commit_all(author_dir.path(), "add queue files")?;
        git_test::push_branch(author_dir.path(), "main")?;

        // Create completion signal in the workspace (not tracked by git).
        let signal = completions::CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Done,
            notes: vec!["Completed".to_string()],
            runner_used: None,
            model_used: None,
        };
        completions::write_completion_signal(workspace_dir.path(), &signal)?;

        let resolved = build_test_resolved(author_dir.path(), queue_path, done_path);

        let sync = apply_completion_and_collect_bytes(
            &resolved,
            workspace_dir.path(),
            Some(workspace_dir.path()),
            "main",
            "RQ-0001",
        )?;

        let updated_queue: QueueFile = serde_json::from_slice(&sync.queue_bytes)?;
        assert!(
            updated_queue.tasks.is_empty(),
            "queue should be empty after completion"
        );
        let done_bytes = sync.done_bytes.expect("done bytes should be present");
        let updated_done: QueueFile = serde_json::from_slice(&done_bytes)?;
        assert!(
            updated_done.tasks.iter().any(|t| t.id == "RQ-0001"),
            "done should include completed task"
        );

        // Verify .base-sync directory is cleaned up after return
        let base_sync_path = workspace_dir.path().join(".base-sync");
        assert!(
            !base_sync_path.exists(),
            ".base-sync should be cleaned up after apply_completion_and_collect_bytes returns"
        );

        // Verify the completion signal was not persisted on the base branch (origin/main).
        git_test::git_run(author_dir.path(), &["fetch", "origin", "main"])?;
        git_test::git_run(author_dir.path(), &["checkout", "main"])?;
        git_test::git_run(author_dir.path(), &["reset", "--hard", "origin/main"])?;
        let signal_path = completions::completion_signal_path(author_dir.path(), "RQ-0001")?;
        assert!(
            !signal_path.exists(),
            "completion signal should not exist on base branch after apply"
        );

        Ok(())
    }

    #[test]
    fn apply_completion_and_collect_bytes_autofinalizes_when_signal_missing() -> Result<()> {
        let (_remote_dir, author_dir, workspace_dir) = setup_merge_test();

        // Ensure we are on main branch locally
        git_test::git_run(author_dir.path(), &["checkout", "-B", "main"])?;

        // Create queue/done files in the author repo
        let ralph_dir = author_dir.path().join(".ralph");
        fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Todo)],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![],
        };
        save_queue_file(&queue_path, &queue);
        save_queue_file(&done_path, &done);
        git_test::commit_all(author_dir.path(), "add queue files")?;
        git_test::push_branch(author_dir.path(), "main")?;

        // NOTE: Intentionally NOT creating any completion signal
        // This tests the auto-finalize behavior when signal is missing from both base and workspace.

        let resolved =
            build_test_resolved(author_dir.path(), queue_path.clone(), done_path.clone());

        let result = apply_completion_and_collect_bytes(
            &resolved,
            workspace_dir.path(),
            Some(workspace_dir.path()),
            "main",
            "RQ-0001",
        )?;

        let updated_queue: QueueFile = serde_json::from_slice(&result.queue_bytes)?;
        assert!(
            updated_queue.tasks.is_empty(),
            "queue should be empty after auto-finalize"
        );
        let done_bytes = result.done_bytes.expect("done bytes should be present");
        let updated_done: QueueFile = serde_json::from_slice(&done_bytes)?;
        let done_task = updated_done
            .tasks
            .iter()
            .find(|t| t.id == "RQ-0001")
            .expect("done should include completed task");
        assert!(
            done_task
                .notes
                .iter()
                .any(|note| note.contains("Auto-finalized")),
            "done task should include auto-finalize note"
        );

        // Verify .base-sync directory is cleaned up even when the call errors
        let base_sync_path = workspace_dir.path().join(".base-sync");
        assert!(
            !base_sync_path.exists(),
            ".base-sync should be cleaned up even when apply_completion_and_collect_bytes errors"
        );

        // Verify base branch queue/done were updated by the auto-finalize flow.
        git_test::git_run(author_dir.path(), &["fetch", "origin", "main"])?;
        git_test::git_run(author_dir.path(), &["checkout", "main"])?;
        git_test::git_run(author_dir.path(), &["reset", "--hard", "origin/main"])?;
        let original_queue: QueueFile = serde_json::from_slice(&fs::read(&queue_path)?)?;
        assert!(
            !original_queue.tasks.iter().any(|t| t.id == "RQ-0001"),
            "original queue should not contain the task after auto-finalize"
        );

        let original_done: QueueFile = serde_json::from_slice(&fs::read(&done_path)?)?;
        assert!(
            original_done.tasks.iter().any(|t| t.id == "RQ-0001"),
            "original done should contain the task after auto-finalize"
        );

        Ok(())
    }

    // Test for merge blocker on head mismatch (RQ-0592)
    #[test]
    fn handle_work_item_returns_blocker_on_head_mismatch() {
        // Create a minimal resolved config with default branch prefix
        let temp = TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        let ralph_dir = repo_root.join(".ralph");
        fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        // Write minimal queue file
        let queue = QueueFile {
            version: 1,
            tasks: vec![build_test_task("RQ-0001", TaskStatus::Doing)],
        };
        save_queue_file(&queue_path, &queue);

        let resolved = build_test_resolved(&repo_root, queue_path, done_path);

        // Create a work item with mismatched head (wrong prefix)
        let work_item = MergeWorkItem {
            task_id: "RQ-0001".to_string(),
            pr: git::PrInfo {
                number: 1,
                url: "https://example.com/pr/1".to_string(),
                head: "wrong-prefix/RQ-0001".to_string(), // Mismatched!
                base: "main".to_string(),
            },
            workspace_path: None,
        };

        // Stop signal (not stopped)
        let merge_stop = AtomicBool::new(false);

        // Call handle_work_item
        let result = handle_work_item(
            &resolved,
            work_item,
            ParallelMergeMethod::Squash,
            ConflictPolicy::Reject,
            MergeRunnerConfig::default(),
            3,
            &temp.path().join("workspaces"),
            false,
            &merge_stop,
        );

        // Should return Ok(Some(MergeResult { merged: false, merge_blocker: Some(...), ... }))
        let result = result.expect("handle_work_item should not error");
        assert!(result.is_some(), "should return a MergeResult for blocker");

        let merge_result = result.unwrap();
        assert!(!merge_result.merged, "merged should be false");
        assert!(
            merge_result.merge_blocker.is_some(),
            "merge_blocker should be set"
        );
        assert!(
            merge_result
                .merge_blocker
                .as_ref()
                .unwrap()
                .contains("head mismatch"),
            "blocker should mention head mismatch: {}",
            merge_result.merge_blocker.as_ref().unwrap()
        );
        assert!(merge_result.queue_bytes.is_none());
        assert!(merge_result.done_bytes.is_none());
        assert!(merge_result.productivity_bytes.is_none());
    }
}
