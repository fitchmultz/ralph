//! Deterministic integration compliance checks.
//!
//! Purpose:
//! - Deterministic integration compliance checks.
//!
//! Responsibilities:
//! - Validate merge conflict, queue/done, task archival, CI, and push-sync invariants.
//! - Collapse validation output into one compliance summary.
//!
//! Non-scope:
//! - Prompt generation.
//! - Retry orchestration or persistence side effects.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Compliance checks report deterministic booleans without mutating queues.

use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::commands::run::supervision::capture_ci_gate_result;
use crate::config::Resolved;
use crate::contracts::TaskStatus;
use crate::git;
use crate::git::error::git_output;
use crate::queue;

#[derive(Debug, Clone)]
pub struct ComplianceResult {
    pub has_unresolved_conflicts: bool,
    pub queue_done_valid: bool,
    pub task_archived: bool,
    pub ci_passed: bool,
    pub conflict_files: Vec<String>,
    pub validation_error: Option<String>,
}

impl ComplianceResult {
    pub fn all_passed(&self) -> bool {
        !self.has_unresolved_conflicts
            && self.queue_done_valid
            && self.task_archived
            && self.ci_passed
    }
}

pub fn run_compliance_checks(
    repo_root: &Path,
    resolved: &Resolved,
    task_id: &str,
    ci_enabled: bool,
) -> Result<ComplianceResult> {
    let conflict_files = git::list_conflict_files(repo_root)?;
    let has_unresolved_conflicts = !conflict_files.is_empty();

    let mut errors = Vec::new();
    if has_unresolved_conflicts {
        errors.push("unresolved merge conflicts remain".to_string());
    }

    let queue_done_valid = match validate_queue_done_semantics(repo_root, resolved) {
        Ok(()) => true,
        Err(err) => {
            errors.push(format!("queue/done semantic validation failed: {}", err));
            false
        }
    };

    let task_archived = match validate_task_archived(resolved, task_id) {
        Ok(()) => true,
        Err(err) => {
            errors.push(format!("task archival validation failed: {}", err));
            false
        }
    };

    let ci_passed = if ci_enabled {
        match run_ci_check(resolved) {
            Ok(()) => true,
            Err(err) => {
                errors.push(format!("CI gate failed: {}", err));
                false
            }
        }
    } else {
        true
    };

    Ok(ComplianceResult {
        has_unresolved_conflicts,
        queue_done_valid,
        task_archived,
        ci_passed,
        conflict_files,
        validation_error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        },
    })
}

pub fn validate_queue_done_semantics(_repo_root: &Path, resolved: &Resolved) -> Result<()> {
    let queue_path = resolved.queue_path.clone();
    let done_path = resolved.done_path.clone();

    let queue = queue::load_queue(&queue_path).context("load queue for validation")?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let done = if done_path.exists() {
        Some(queue::load_queue(&done_path).context("load done for validation")?)
    } else {
        None
    };

    queue::validate_queue_set(
        &queue,
        done.as_ref(),
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("queue/done semantic validation")?;

    Ok(())
}

pub fn validate_task_archived(resolved: &Resolved, task_id: &str) -> Result<()> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        bail!("task id is empty");
    }

    let queue_path = resolved.queue_path.clone();
    let done_path = resolved.done_path.clone();

    if !queue_path.exists() {
        bail!("queue file missing at {}", queue_path.display());
    }
    if !done_path.exists() {
        bail!("done file missing at {}", done_path.display());
    }

    let queue_file = queue::load_queue(&queue_path)
        .with_context(|| format!("load queue file {}", queue_path.display()))?;
    if queue_file
        .tasks
        .iter()
        .any(|task| task.id.trim() == task_id)
    {
        bail!("task {} still present in {}", task_id, queue_path.display());
    }

    let done_file = queue::load_queue(&done_path)
        .with_context(|| format!("load done file {}", done_path.display()))?;
    let done_task = done_file
        .tasks
        .iter()
        .find(|task| task.id.trim() == task_id)
        .ok_or_else(|| anyhow::anyhow!("task {} missing from {}", task_id, done_path.display()))?;

    if done_task.status != TaskStatus::Done {
        bail!(
            "task {} exists in done but status is {:?}, expected done",
            task_id,
            done_task.status
        );
    }

    Ok(())
}

fn run_ci_check(resolved: &Resolved) -> Result<()> {
    let result = capture_ci_gate_result(resolved)?;
    if !result.success {
        let combined = format!("{}\n{}", result.stdout, result.stderr).to_lowercase();
        if combined.contains("waiting for file lock")
            || combined.contains("file lock on build directory")
        {
            bail!(
                "CI lock contention detected (stale build/test process likely holding a lock). {} | {}",
                result.stdout.trim(),
                result.stderr.trim()
            );
        }
        bail!("{} | {}", result.stdout.trim(), result.stderr.trim());
    }

    Ok(())
}

pub fn head_is_synced_to_remote(repo_root: &Path, target_branch: &str) -> Result<bool> {
    git::fetch_branch(repo_root, "origin", target_branch)
        .with_context(|| format!("fetch origin/{} for sync check", target_branch))?;

    let remote_ref = format!("origin/{}", target_branch);
    let output = git_output(
        repo_root,
        &["merge-base", "--is-ancestor", "HEAD", &remote_ref],
    )
    .with_context(|| format!("check if HEAD is ancestor of {}", remote_ref))?;

    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(1) {
        return Ok(false);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "unable to verify push sync against {}: {}",
        remote_ref,
        stderr.trim()
    );
}
