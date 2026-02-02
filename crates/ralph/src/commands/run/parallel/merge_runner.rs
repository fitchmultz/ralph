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
//! - Worktrees remain available until merge completion or failure.

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
    if !workspace_path.exists() {
        bail!(
            "Merge conflict resolution failed: workspace not found at {}",
            workspace_path.display()
        );
    }

    git_run(&workspace_path, &["checkout", &pr.head])?;
    git_run(&workspace_path, &["fetch", "origin"])?;
    let base_ref = format!("origin/{}", pr.base);
    git_run(&workspace_path, &["merge", &base_ref])?;

    let conflicts = conflict_files(&workspace_path)?;
    if conflicts.is_empty() {
        return Ok(());
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
    worktree_path: &Path,
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
        worktree_path,
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
    use super::task_id_from_branch;

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
}
