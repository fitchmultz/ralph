//! Merge runner for parallel PRs and AI-based conflict resolution.
//!
//! Responsibilities:
//! - Consume PRs and attempt merges based on configured policy.
//! - Resolve merge conflicts using an AI runner when enabled.
//! - Emit merge results for downstream cleanup.
//!
//! Not handled here:
//! - Worker orchestration or task selection (see `parallel/mod.rs`).
//! - PR creation (see `git/pr.rs`).
//!
//! Invariants/assumptions:
//! - PRs originate from branches named with the configured prefix.
//! - Workspaces remain available until merge completion or failure.

use crate::commands::run::PhaseType;
use crate::config;
use crate::contracts::{
    ConflictPolicy, MergeRunnerConfig, ParallelMergeMethod, RunnerCliOptionsPatch,
};
use crate::{git, promptflow, prompts, runner};
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

pub(crate) enum MergeQueueSource {
    AsCreated(mpsc::Receiver<git::PrInfo>),
    AfterAll(Vec<git::PrInfo>),
}

#[derive(Debug, Clone)]
pub(crate) struct MergeResult {
    pub task_id: String,
    pub merged: bool,
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
    match pr_queue {
        MergeQueueSource::AsCreated(rx) => {
            for pr in rx.iter() {
                if merge_stop.load(Ordering::SeqCst) {
                    break;
                }
                handle_pr(
                    resolved,
                    pr,
                    merge_method,
                    conflict_policy,
                    merge_runner.clone(),
                    retries,
                    workspace_root,
                    delete_branch,
                    &merge_result_tx,
                    &merge_stop,
                )?;
            }
        }
        MergeQueueSource::AfterAll(prs) => {
            for pr in prs {
                if merge_stop.load(Ordering::SeqCst) {
                    break;
                }
                handle_pr(
                    resolved,
                    pr,
                    merge_method,
                    conflict_policy,
                    merge_runner.clone(),
                    retries,
                    workspace_root,
                    delete_branch,
                    &merge_result_tx,
                    &merge_stop,
                )?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_pr(
    resolved: &config::Resolved,
    pr: git::PrInfo,
    merge_method: ParallelMergeMethod,
    conflict_policy: ConflictPolicy,
    merge_runner: MergeRunnerConfig,
    retries: u8,
    workspace_root: &Path,
    delete_branch: bool,
    merge_result_tx: &mpsc::Sender<MergeResult>,
    merge_stop: &AtomicBool,
) -> Result<()> {
    if merge_stop.load(Ordering::SeqCst) {
        return Ok(());
    }

    let branch_prefix = resolved
        .config
        .parallel
        .branch_prefix
        .clone()
        .unwrap_or_else(|| "ralph/".to_string());
    let task_id = task_id_from_branch(&pr.head, &branch_prefix);

    let merged = merge_pr_with_retries(
        resolved,
        &pr,
        merge_method,
        conflict_policy,
        merge_runner,
        retries,
        workspace_root,
        &task_id,
        delete_branch,
        merge_stop,
    )?;

    if merged {
        let _ = merge_result_tx.send(MergeResult {
            task_id,
            merged: true,
        });
    }

    Ok(())
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

fn run_merge_runner_prompt(
    resolved: &config::Resolved,
    merge_runner: &MergeRunnerConfig,
    workspace_path: &Path,
    prompt: &str,
) -> Result<()> {
    let settings = runner::resolve_agent_settings(
        merge_runner.runner,
        merge_runner.model.clone(),
        merge_runner.reasoning_effort,
        &RunnerCliOptionsPatch::default(),
        None,
        &resolved.config.agent,
    )?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    runner::run_prompt(
        settings.runner,
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
    match git::is_ahead_of_upstream(repo_root) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
            git::push_upstream(repo_root).context("push branch to upstream")?;
        }
        Err(git::GitError::NoUpstream) | Err(git::GitError::NoUpstreamConfigured) => {
            git::push_upstream_allow_create(repo_root)
                .context("push branch and create upstream")?;
        }
        Err(err) => return Err(err.into()),
    }
    Ok(())
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

fn task_id_from_branch(head: &str, prefix: &str) -> String {
    let trimmed = head.trim();
    if let Some(rest) = trimmed.strip_prefix(prefix) {
        rest.to_string()
    } else {
        trimmed.to_string()
    }
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
    use tempfile::TempDir;

    #[test]
    fn task_id_from_branch_strips_prefix() {
        let task_id = task_id_from_branch("ralph/RQ-0001", "ralph/");
        assert_eq!(task_id, "RQ-0001");
    }

    #[test]
    fn task_id_from_branch_falls_back_to_head() {
        let task_id = task_id_from_branch("feature/RQ-0002", "ralph/");
        assert_eq!(task_id, "feature/RQ-0002");
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
}
