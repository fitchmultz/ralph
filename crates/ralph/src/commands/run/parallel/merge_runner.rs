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
use std::sync::mpsc;
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
    worktree_root: &Path,
    delete_branch: bool,
    merge_result_tx: mpsc::Sender<MergeResult>,
) -> Result<()> {
    match pr_queue {
        MergeQueueSource::AsCreated(rx) => {
            for pr in rx.iter() {
                handle_pr(
                    resolved,
                    pr,
                    merge_method,
                    conflict_policy,
                    merge_runner.clone(),
                    retries,
                    worktree_root,
                    delete_branch,
                    &merge_result_tx,
                )?;
            }
        }
        MergeQueueSource::AfterAll(prs) => {
            for pr in prs {
                handle_pr(
                    resolved,
                    pr,
                    merge_method,
                    conflict_policy,
                    merge_runner.clone(),
                    retries,
                    worktree_root,
                    delete_branch,
                    &merge_result_tx,
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
    worktree_root: &Path,
    delete_branch: bool,
    merge_result_tx: &mpsc::Sender<MergeResult>,
) -> Result<()> {
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
        worktree_root,
        &task_id,
        delete_branch,
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
    worktree_root: &Path,
    task_id: &str,
    delete_branch: bool,
) -> Result<bool> {
    let mut attempts = 0u8;
    loop {
        attempts += 1;
        let state = git::pr_merge_state(&resolved.repo_root, pr.number)?;
        match state {
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
                    resolve_conflicts(resolved, pr, worktree_root, task_id, &merge_runner)?;
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
    worktree_root: &Path,
    task_id: &str,
    merge_runner: &MergeRunnerConfig,
) -> Result<()> {
    let worktree_path = worktree_root.join(task_id);
    if !worktree_path.exists() {
        bail!(
            "Merge conflict resolution failed: worktree not found at {}",
            worktree_path.display()
        );
    }

    git_run(&worktree_path, &["checkout", &pr.head])?;
    git_run(&worktree_path, &["fetch", "origin"])?;
    let base_ref = format!("origin/{}", pr.base);
    git_run(&worktree_path, &["merge", &base_ref])?;

    let conflicts = conflict_files(&worktree_path)?;
    if conflicts.is_empty() {
        return Ok(());
    }

    let template = prompts::load_merge_conflict_prompt(&worktree_path)?;
    let prompt = promptflow::build_merge_conflict_prompt(&template, &conflicts, &resolved.config)?;
    let prompt = prompts::wrap_with_instruction_files(&worktree_path, &prompt, &resolved.config)?;

    run_merge_runner_prompt(resolved, merge_runner, &worktree_path, &prompt)?;

    let remaining = conflict_files(&worktree_path)?;
    if !remaining.is_empty() {
        bail!(
            "Merge conflicts remain after AI resolution: {}",
            remaining.join(", ")
        );
    }

    git_run(&worktree_path, &["add", "-A"])?;
    let status = git_status(&worktree_path)?;
    if status.trim().is_empty() {
        bail!("No changes staged after conflict resolution.");
    }
    let message = format!("Resolve merge conflicts for {}", task_id);
    git_run(&worktree_path, &["commit", "-m", &message])?;
    push_branch(&worktree_path)?;
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
