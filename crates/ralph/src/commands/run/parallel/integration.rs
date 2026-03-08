//! Worker integration loop for direct-push parallel mode.
//!
//! Responsibilities:
//! - Drive a bounded integration loop through the phase continue-session.
//! - Keep `fetch/rebase/conflict-fix/commit/push` execution agent-owned.
//! - Enforce deterministic post-turn compliance gates before success.
//! - Emit remediation handoff packets for blocked attempts.
//!
//! Not handled here:
//! - Phase execution itself (see `run_one` phase modules).
//! - Worker spawning/orchestration (see `worker.rs` and `orchestration.rs`).
//!
//! Invariants/assumptions:
//! - Called after the worker has completed its configured phases.
//! - Called only in parallel-worker mode.
//! - Single-mode (`ralph run one` without `--parallel-worker`) is unchanged.

#![allow(dead_code)]

use crate::commands::run::supervision::{
    ContinueSession, capture_ci_gate_result, resume_continue_session,
};
use crate::config::Resolved;
use crate::contracts::TaskStatus;
use crate::git;
use crate::git::error::git_output;
use crate::queue;
use crate::runutil::sleep_with_cancellation;
use crate::timeutil;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

// =============================================================================
// Integration Loop Configuration
// =============================================================================

/// Configuration for the integration loop.
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    /// Maximum number of integration attempts.
    pub max_attempts: u32,
    /// Backoff intervals between retries (in milliseconds).
    pub backoff_ms: Vec<u64>,
    /// Target branch to push to.
    pub target_branch: String,
    /// Whether CI gate is enabled.
    pub ci_enabled: bool,
    /// Rendered CI gate label for prompts and handoff context.
    pub ci_label: String,
}

impl IntegrationConfig {
    pub fn from_resolved(resolved: &Resolved, target_branch: &str) -> Self {
        let parallel = &resolved.config.parallel;
        let target_branch = target_branch.trim();
        Self {
            max_attempts: parallel.max_push_attempts.unwrap_or(50) as u32,
            backoff_ms: parallel
                .push_backoff_ms
                .clone()
                .unwrap_or_else(super::default_push_backoff_ms),
            target_branch: if target_branch.is_empty() {
                "main".to_string()
            } else {
                target_branch.to_string()
            },
            ci_enabled: resolved
                .config
                .agent
                .ci_gate
                .as_ref()
                .is_some_and(|ci_gate| ci_gate.is_enabled()),
            ci_label: crate::commands::run::supervision::ci_gate_command_label(resolved),
        }
    }

    /// Get backoff for a specific attempt index (0-indexed).
    pub fn backoff_for_attempt(&self, attempt: usize) -> Duration {
        let ms = self
            .backoff_ms
            .get(attempt)
            .copied()
            .unwrap_or_else(|| self.backoff_ms.last().copied().unwrap_or(10_000));
        Duration::from_millis(ms)
    }
}

// =============================================================================
// Integration Outcome
// =============================================================================

/// Outcome of the integration loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationOutcome {
    /// Push succeeded and compliance gates passed.
    Success,
    /// Integration could not complete within bounded retries.
    BlockedPush { reason: String },
    /// Terminal integration failure (for example no resumable session).
    Failed { reason: String },
}

/// Persisted marker written by workers when integration ends in `blocked_push`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BlockedPushMarker {
    pub task_id: String,
    pub reason: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub generated_at: String,
}

// =============================================================================
// Handoff Packet
// =============================================================================

/// Structured handoff packet for blocked remediation attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationHandoff {
    /// Task identifier.
    pub task_id: String,
    /// Task title.
    pub task_title: String,
    /// Target branch for the push.
    pub target_branch: String,
    /// Current attempt number.
    pub attempt: u32,
    /// Maximum attempts allowed.
    pub max_attempts: u32,
    /// List of files with unresolved conflicts.
    pub conflict_files: Vec<String>,
    /// Current git status output.
    pub git_status: String,
    /// Phase outputs summary (last phase response summary).
    pub phase_summary: String,
    /// Original task intent snapshot.
    pub task_intent: String,
    /// CI context when validation failed on CI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_context: Option<CiContext>,
    /// Timestamp when handoff was generated.
    pub generated_at: String,
    /// Queue/done semantic rules for conflict resolution.
    pub queue_done_rules: QueueDoneRules,
}

/// CI context for remediation handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiContext {
    pub command: String,
    pub last_output: String,
    pub exit_code: i32,
}

/// Semantic rules for queue/done conflict resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueDoneRules {
    pub rules: Vec<String>,
}

impl Default for QueueDoneRules {
    fn default() -> Self {
        Self {
            rules: vec![
                "Remove the completed task from the queue file".into(),
                "Ensure the completed task is present in the done archive file".into(),
                "Preserve entries from other workers exactly".into(),
                "Preserve task metadata (timestamps/notes)".into(),
            ],
        }
    }
}

impl RemediationHandoff {
    pub fn new(
        task_id: impl Into<String>,
        task_title: impl Into<String>,
        target_branch: impl Into<String>,
        attempt: u32,
        max_attempts: u32,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_title: task_title.into(),
            target_branch: target_branch.into(),
            attempt,
            max_attempts,
            conflict_files: Vec::new(),
            git_status: String::new(),
            phase_summary: String::new(),
            task_intent: String::new(),
            ci_context: None,
            generated_at: timeutil::now_utc_rfc3339_or_fallback(),
            queue_done_rules: QueueDoneRules::default(),
        }
    }

    pub fn with_conflicts(mut self, files: Vec<String>) -> Self {
        self.conflict_files = files;
        self
    }

    pub fn with_git_status(mut self, status: String) -> Self {
        self.git_status = status;
        self
    }

    pub fn with_phase_summary(mut self, summary: String) -> Self {
        self.phase_summary = summary;
        self
    }

    pub fn with_task_intent(mut self, intent: String) -> Self {
        self.task_intent = intent;
        self
    }

    pub fn with_ci_context(mut self, command: String, last_output: String, exit_code: i32) -> Self {
        self.ci_context = Some(CiContext {
            command,
            last_output,
            exit_code,
        });
        self
    }
}

fn blocked_push_marker_path(workspace_path: &Path) -> PathBuf {
    workspace_path.join(super::BLOCKED_PUSH_MARKER_FILE)
}

fn write_blocked_push_marker(
    workspace_path: &Path,
    task_id: &str,
    reason: &str,
    attempt: u32,
    max_attempts: u32,
) -> Result<()> {
    let marker = BlockedPushMarker {
        task_id: task_id.trim().to_string(),
        reason: reason.to_string(),
        attempt,
        max_attempts,
        generated_at: timeutil::now_utc_rfc3339_or_fallback(),
    };
    let marker_path = blocked_push_marker_path(workspace_path);
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create blocked marker directory {}", parent.display()))?;
    }
    let rendered = serde_json::to_string_pretty(&marker).context("serialize blocked marker")?;
    crate::fsutil::write_atomic(&marker_path, rendered.as_bytes())
        .with_context(|| format!("write blocked marker {}", marker_path.display()))?;
    Ok(())
}

fn clear_blocked_push_marker(workspace_path: &Path) {
    let marker_path = blocked_push_marker_path(workspace_path);
    if !marker_path.exists() {
        return;
    }
    if let Err(err) = std::fs::remove_file(&marker_path) {
        log::warn!(
            "Failed to clear blocked marker at {}: {}",
            marker_path.display(),
            err
        );
    }
}

pub(crate) fn read_blocked_push_marker(workspace_path: &Path) -> Result<Option<BlockedPushMarker>> {
    let marker_path = blocked_push_marker_path(workspace_path);
    if !marker_path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&marker_path)
        .with_context(|| format!("read blocked marker {}", marker_path.display()))?;
    let marker =
        serde_json::from_str::<BlockedPushMarker>(&raw).context("parse blocked marker json")?;
    Ok(Some(marker))
}

/// Write handoff packet to workspace cache directory.
pub fn write_handoff_packet(
    workspace_path: &Path,
    task_id: &str,
    attempt: u32,
    handoff: &RemediationHandoff,
) -> Result<PathBuf> {
    let handoff_dir = workspace_path
        .join(".ralph/cache/parallel/handoffs")
        .join(task_id);
    std::fs::create_dir_all(&handoff_dir)
        .with_context(|| format!("create handoff directory {}", handoff_dir.display()))?;

    let path = handoff_dir.join(format!("attempt_{}.json", attempt));
    let content = serde_json::to_string_pretty(handoff).context("serialize handoff packet")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("write handoff packet to {}", path.display()))?;
    Ok(path)
}

// =============================================================================
// Compliance Checks
// =============================================================================

/// Result of deterministic compliance checks.
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

/// Run deterministic compliance checks after each agent integration turn.
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
        match run_ci_check(repo_root, resolved) {
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

/// Validate queue/done files semantically from the resolved queue/done paths.
fn validate_queue_done_semantics(_repo_root: &Path, resolved: &Resolved) -> Result<()> {
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

/// Validate that the specific task is removed from queue and present as done.
fn validate_task_archived(resolved: &Resolved, task_id: &str) -> Result<()> {
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

/// Run CI gate check as deterministic validation.
fn run_ci_check(_repo_root: &Path, resolved: &Resolved) -> Result<()> {
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

/// Verify that local HEAD is integrated into `origin/<target_branch>`.
fn head_is_synced_to_remote(repo_root: &Path, target_branch: &str) -> Result<bool> {
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

// =============================================================================
// Integration Prompting
// =============================================================================

#[allow(clippy::too_many_arguments)]
fn build_agent_integration_prompt(
    task_id: &str,
    task_title: &str,
    target_branch: &str,
    queue_path: &Path,
    done_path: &Path,
    attempt: u32,
    max_attempts: u32,
    phase_summary: &str,
    status_snapshot: &str,
    ci_enabled: bool,
    ci_label: &str,
    previous_failure: Option<&str>,
) -> String {
    let queue_path_display = queue_path.display();
    let done_path_display = done_path.display();
    let failure_block = previous_failure.map_or_else(String::new, |failure| {
        format!("\n## Previous Attempt Failed\n{}\n", failure)
    });

    let ci_block = if ci_enabled {
        format!(
            "- Run CI gate and fix failures before pushing: `{}`",
            ci_label
        )
    } else {
        "- CI gate is disabled for this task".to_string()
    };

    sanitize_prompt_for_runner(&format!(
        r#"# Parallel Integration (Mandatory) - Attempt {attempt}/{max_attempts}
You are finalizing task `{task_id}` (`{task_title}`) for direct push to `origin/{target_branch}`.

## Hard Requirement
You MUST execute integration git operations yourself in this turn. Do not stop early.
You are NOT done until all required checks are satisfied.

## Context
- Phase summary: {phase_summary}
- Current git status snapshot:
```text
{status_snapshot}
```
{failure_block}
## Required Sequence
1. `git fetch origin {target_branch}`
2. Rebase on latest remote state: `git rebase origin/{target_branch}`
3. If conflicts exist:
   - Resolve every conflict marker while preserving both upstream and task intent.
   - For queue/done files, preserve other workers' entries exactly.
   - Ensure `{task_id}` is removed from queue and present as done in done.
   - Continue rebase until complete (`git add ...`, `git rebase --continue`).
4. Ensure bookkeeping is correct:
   - `{queue_path_display}` does NOT contain `{task_id}`
   - `{done_path_display}` DOES contain `{task_id}` with done status
5. Stage and commit any remaining changes needed for integration.
6. {ci_block}
7. Push directly to base branch: `git push origin HEAD:{target_branch}`
8. If push is rejected (non-fast-forward), repeat from step 1 in this same turn.

## Completion Contract (Mandatory)
Before ending your response:
- No unresolved merge conflicts remain.
- Push to `origin/{target_branch}` has succeeded.
- Bookkeeping files are semantically correct for `{task_id}`.
- CI has passed when enabled.

If any check fails, keep working in this same turn until fixed.
"#
    ))
}

fn sanitize_prompt_for_runner(prompt: &str) -> String {
    prompt
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn compose_block_reason(
    compliance: &ComplianceResult,
    pushed: bool,
    extra: Option<&str>,
) -> String {
    let mut reasons = Vec::new();

    if compliance.has_unresolved_conflicts {
        reasons.push(format!(
            "unresolved conflicts: {}",
            compliance.conflict_files.join(", ")
        ));
    }
    if !compliance.queue_done_valid {
        reasons.push("queue/done semantic validation failed".to_string());
    }
    if !compliance.task_archived {
        reasons.push("task archival validation failed".to_string());
    }
    if !compliance.ci_passed {
        reasons.push("CI validation failed".to_string());
    }
    if !pushed {
        reasons.push("HEAD is not yet integrated into target branch".to_string());
    }
    if let Some(extra) = extra {
        reasons.push(extra.to_string());
    }

    if let Some(validation_error) = &compliance.validation_error {
        reasons.push(validation_error.clone());
    }

    if reasons.is_empty() {
        "integration did not satisfy completion contract".to_string()
    } else {
        reasons.join("; ")
    }
}

// =============================================================================
// Integration Loop
// =============================================================================

/// Run the integration loop for a completed worker.
///
/// Integration actions are agent-owned via continue-session prompts.
/// Ralph only validates completion and retries when contract checks fail.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_integration_loop(
    resolved: &Resolved,
    task_id: &str,
    task_title: &str,
    config: &IntegrationConfig,
    phase_summary: &str,
    continue_session: &mut ContinueSession,
    on_resume: &mut dyn FnMut(&crate::runner::RunnerOutput, Duration) -> Result<()>,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<IntegrationOutcome> {
    let repo_root = &resolved.repo_root;
    clear_blocked_push_marker(repo_root);
    let mut previous_failure: Option<String> = None;

    for attempt_index in 0..config.max_attempts {
        let attempt = attempt_index + 1;
        log::info!(
            "Agent-owned integration attempt {}/{} for {}",
            attempt,
            config.max_attempts,
            task_id
        );

        let status_snapshot = git::status_porcelain(repo_root).unwrap_or_default();
        let prompt = build_agent_integration_prompt(
            task_id,
            task_title,
            &config.target_branch,
            &resolved.queue_path,
            &resolved.done_path,
            attempt,
            config.max_attempts,
            phase_summary,
            &status_snapshot,
            config.ci_enabled,
            &config.ci_label,
            previous_failure.as_deref(),
        );

        let (output, elapsed) =
            match resume_continue_session(resolved, continue_session, &prompt, plugins) {
                Ok(resume) => resume,
                Err(err) => {
                    let reason = format!("integration continuation failed: {:#}", err);
                    if attempt >= config.max_attempts {
                        if let Err(marker_err) = write_blocked_push_marker(
                            repo_root,
                            task_id,
                            &reason,
                            attempt,
                            config.max_attempts,
                        ) {
                            log::warn!("Failed to write blocked marker: {}", marker_err);
                        }
                        return Ok(IntegrationOutcome::BlockedPush { reason });
                    }
                    previous_failure = Some(reason);
                    wait_before_retry(config, attempt_index as usize, task_id)?;
                    continue;
                }
            };

        on_resume(&output, elapsed)?;

        let compliance = run_compliance_checks(repo_root, resolved, task_id, config.ci_enabled)?;
        let (pushed, push_check_error) =
            match head_is_synced_to_remote(repo_root, &config.target_branch) {
                Ok(value) => (value, None),
                Err(err) => (false, Some(format!("push sync validation failed: {}", err))),
            };

        if compliance.all_passed() && pushed {
            log::info!(
                "Integration succeeded for {} on attempt {}/{}",
                task_id,
                attempt,
                config.max_attempts
            );
            return Ok(IntegrationOutcome::Success);
        }

        let reason = compose_block_reason(&compliance, pushed, push_check_error.as_deref());
        let mut handoff = RemediationHandoff::new(
            task_id,
            task_title,
            &config.target_branch,
            attempt,
            config.max_attempts,
        )
        .with_conflicts(compliance.conflict_files.clone())
        .with_git_status(git::status_porcelain(repo_root).unwrap_or_default())
        .with_phase_summary(phase_summary.to_string())
        .with_task_intent(format!("Complete task {}: {}", task_id, task_title));

        if !compliance.ci_passed {
            handoff = handoff.with_ci_context(
                config.ci_label.clone(),
                compliance
                    .validation_error
                    .clone()
                    .unwrap_or_else(|| "CI gate validation failed".to_string()),
                1,
            );
        }

        if let Err(err) = write_handoff_packet(repo_root, task_id, attempt, &handoff) {
            log::warn!("Failed to persist remediation handoff packet: {}", err);
        }

        if attempt >= config.max_attempts {
            if let Err(marker_err) =
                write_blocked_push_marker(repo_root, task_id, &reason, attempt, config.max_attempts)
            {
                log::warn!("Failed to write blocked marker: {}", marker_err);
            }
            return Ok(IntegrationOutcome::BlockedPush { reason });
        }

        previous_failure = Some(reason);
        wait_before_retry(config, attempt_index as usize, task_id)?;
    }

    let reason = format!("integration exhausted {} attempts", config.max_attempts);
    if let Err(marker_err) = write_blocked_push_marker(
        repo_root,
        task_id,
        &reason,
        config.max_attempts,
        config.max_attempts,
    ) {
        log::warn!("Failed to write blocked marker: {}", marker_err);
    }
    Ok(IntegrationOutcome::BlockedPush { reason })
}

fn wait_before_retry(
    config: &IntegrationConfig,
    attempt_index: usize,
    task_id: &str,
) -> Result<()> {
    let delay = config.backoff_for_attempt(attempt_index);
    log::info!(
        "Integration retry backoff for {}: sleeping {}ms before next attempt",
        task_id,
        delay.as_millis()
    );
    sleep_with_cancellation(delay, None)
        .map_err(|_| anyhow::anyhow!("integration retry cancelled for {}", task_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            description: None,
            status,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            estimated_minutes: None,
            actual_minutes: None,
            parent_id: None,
        }
    }

    #[test]
    fn integration_config_default_backoff() {
        let config = IntegrationConfig {
            max_attempts: 5,
            backoff_ms: vec![500, 2000, 5000, 10000],
            target_branch: "main".into(),
            ci_enabled: true,
            ci_label: "make ci".into(),
        };

        assert_eq!(config.backoff_for_attempt(0), Duration::from_millis(500));
        assert_eq!(config.backoff_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(config.backoff_for_attempt(2), Duration::from_millis(5000));
        assert_eq!(config.backoff_for_attempt(3), Duration::from_millis(10000));
        assert_eq!(config.backoff_for_attempt(4), Duration::from_millis(10000));
        assert_eq!(config.backoff_for_attempt(10), Duration::from_millis(10000));
    }

    #[test]
    fn remediation_handoff_builder() {
        let handoff = RemediationHandoff::new("RQ-0001", "Test Task", "main", 2, 5)
            .with_conflicts(vec!["src/lib.rs".into(), "src/main.rs".into()])
            .with_git_status("UU src/lib.rs\nUU src/main.rs".into())
            .with_phase_summary("Implemented feature X".into())
            .with_task_intent("Complete feature X implementation".into());

        assert_eq!(handoff.task_id, "RQ-0001");
        assert_eq!(handoff.task_title, "Test Task");
        assert_eq!(handoff.target_branch, "main");
        assert_eq!(handoff.attempt, 2);
        assert_eq!(handoff.max_attempts, 5);
        assert_eq!(handoff.conflict_files.len(), 2);
        assert_eq!(handoff.phase_summary, "Implemented feature X");
        assert!(handoff.ci_context.is_none());
    }

    #[test]
    fn remediation_handoff_with_ci() {
        let handoff = RemediationHandoff::new("RQ-0001", "Test", "main", 1, 5).with_ci_context(
            "make ci".into(),
            "test failed".into(),
            1,
        );

        assert!(handoff.ci_context.is_some());
        let ci = handoff.ci_context.unwrap();
        assert_eq!(ci.command, "make ci");
        assert_eq!(ci.last_output, "test failed");
        assert_eq!(ci.exit_code, 1);
    }

    #[test]
    fn integration_prompt_contains_mandatory_contract() {
        let prompt = build_agent_integration_prompt(
            "RQ-0001",
            "Implement feature",
            "main",
            Path::new("/tmp/queue.json"),
            Path::new("/tmp/done.json"),
            1,
            5,
            "phase summary",
            " M src/lib.rs",
            true,
            "make ci",
            Some("previous failure"),
        );

        assert!(prompt.contains("MUST execute integration git operations"));
        assert!(prompt.contains("Completion Contract (Mandatory)"));
        assert!(prompt.contains("git push origin HEAD:main"));
        assert!(prompt.contains("previous failure"));
    }

    #[test]
    fn integration_prompt_uses_explicit_target_branch_for_push() {
        let prompt = build_agent_integration_prompt(
            "RQ-0001",
            "Implement feature",
            "release/2026",
            Path::new("/tmp/queue.json"),
            Path::new("/tmp/done.json"),
            1,
            5,
            "phase summary",
            " M src/lib.rs",
            true,
            "make ci",
            None,
        );

        assert!(prompt.contains("git fetch origin release/2026"));
        assert!(prompt.contains("git rebase origin/release/2026"));
        assert!(prompt.contains("git push origin HEAD:release/2026"));
    }

    #[test]
    fn integration_prompt_sanitizes_nul_bytes() {
        let prompt = build_agent_integration_prompt(
            "RQ-0001",
            "NUL test",
            "main",
            Path::new("/tmp/queue.json"),
            Path::new("/tmp/done.json"),
            1,
            5,
            "phase\0summary",
            "status\0snapshot",
            true,
            "make ci",
            Some("previous\0failure"),
        );

        assert!(!prompt.contains('\0'));
        assert!(prompt.contains("phase summary"));
        assert!(prompt.contains("status snapshot"));
        assert!(prompt.contains("previous failure"));
    }

    #[test]
    fn compliance_result_all_passed() {
        let passed = ComplianceResult {
            has_unresolved_conflicts: false,
            queue_done_valid: true,
            task_archived: true,
            ci_passed: true,
            conflict_files: vec![],
            validation_error: None,
        };
        assert!(passed.all_passed());

        let failed = ComplianceResult {
            has_unresolved_conflicts: false,
            queue_done_valid: true,
            task_archived: false,
            ci_passed: true,
            conflict_files: vec![],
            validation_error: None,
        };
        assert!(!failed.all_passed());
    }

    #[test]
    fn integration_config_uses_explicit_target_branch() -> Result<()> {
        let dir = tempfile::TempDir::new()?;
        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: dir.path().to_path_buf(),
            queue_path: dir.path().join(".ralph/queue.json"),
            done_path: dir.path().join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let cfg = IntegrationConfig::from_resolved(&resolved, "release/2026");
        assert_eq!(cfg.target_branch, "release/2026");
        Ok(())
    }

    #[test]
    fn task_archived_validation_uses_resolved_paths_not_workspace_local_files() -> Result<()> {
        let dir = TempDir::new()?;
        let coordinator = dir.path().join("coordinator");
        let worker_workspace = dir.path().join("worker-ws");
        std::fs::create_dir_all(&coordinator)?;
        std::fs::create_dir_all(worker_workspace.join(".ralph"))?;

        let coordinator_queue = coordinator.join("queue.json");
        let coordinator_done = coordinator.join("done.json");
        let workspace_queue = worker_workspace.join(".ralph/queue.json");
        let workspace_done = worker_workspace.join(".ralph/done.json");

        let mut coordinator_queue_file = QueueFile::default();
        coordinator_queue_file
            .tasks
            .push(make_task("RQ-0001", TaskStatus::Todo));
        queue::save_queue(&coordinator_queue, &coordinator_queue_file)?;
        queue::save_queue(&coordinator_done, &QueueFile::default())?;

        // Workspace-local files look archived, but should be ignored by validation.
        queue::save_queue(&workspace_queue, &QueueFile::default())?;
        let mut workspace_done_file = QueueFile::default();
        workspace_done_file
            .tasks
            .push(make_task("RQ-0001", TaskStatus::Done));
        queue::save_queue(&workspace_done, &workspace_done_file)?;

        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: worker_workspace,
            queue_path: coordinator_queue.clone(),
            done_path: coordinator_done,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = validate_task_archived(&resolved, "RQ-0001")
            .expect_err("validation should use resolved queue path");
        let msg = err.to_string();
        assert!(
            msg.contains(coordinator_queue.to_string_lossy().as_ref()),
            "error should reference resolved queue path, got: {msg}"
        );
        Ok(())
    }

    #[test]
    fn queue_done_semantics_validation_uses_resolved_paths() -> Result<()> {
        let dir = TempDir::new()?;
        let coordinator = dir.path().join("coordinator");
        let worker_workspace = dir.path().join("worker-ws");
        std::fs::create_dir_all(&coordinator)?;
        std::fs::create_dir_all(worker_workspace.join(".ralph"))?;

        let coordinator_queue = coordinator.join("queue.json");
        let coordinator_done = coordinator.join("done.json");
        let workspace_queue = worker_workspace.join(".ralph/queue.json");
        let workspace_done = worker_workspace.join(".ralph/done.json");

        // Coordinator queue is semantically invalid for RQ id rules.
        let mut invalid_queue = QueueFile::default();
        invalid_queue
            .tasks
            .push(make_task("BAD-ID", TaskStatus::Todo));
        queue::save_queue(&coordinator_queue, &invalid_queue)?;
        queue::save_queue(&coordinator_done, &QueueFile::default())?;

        // Workspace-local queue is valid, but should not be read.
        let mut valid_queue = QueueFile::default();
        valid_queue
            .tasks
            .push(make_task("RQ-0001", TaskStatus::Todo));
        queue::save_queue(&workspace_queue, &valid_queue)?;
        queue::save_queue(&workspace_done, &QueueFile::default())?;

        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: worker_workspace.clone(),
            queue_path: coordinator_queue,
            done_path: coordinator_done,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        validate_queue_done_semantics(&worker_workspace, &resolved)
            .expect_err("validation should fail from resolved queue path");
        Ok(())
    }

    #[test]
    fn blocked_marker_roundtrip() -> Result<()> {
        let temp = TempDir::new()?;
        write_blocked_push_marker(temp.path(), "RQ-0001", "blocked reason", 5, 5)?;
        let marker = read_blocked_push_marker(temp.path())?.expect("marker should exist");
        assert_eq!(marker.task_id, "RQ-0001");
        assert_eq!(marker.reason, "blocked reason");
        assert_eq!(marker.attempt, 5);
        assert_eq!(marker.max_attempts, 5);

        clear_blocked_push_marker(temp.path());
        assert!(read_blocked_push_marker(temp.path())?.is_none());
        Ok(())
    }
}
