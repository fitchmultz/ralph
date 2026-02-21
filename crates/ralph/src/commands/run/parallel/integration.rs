//! Worker integration loop for direct-push parallel mode.
//!
//! Responsibilities:
//! - Implement the bounded integration loop: fetch, rebase, resolve conflicts, CI, push.
//! - Generate handoff packets for agent-led remediation.
//! - Enforce deterministic compliance checks before push.
//!
//! Not handled here:
//! - Phase execution (see `run_one` module).
//! - Worker spawning/orchestration (see `worker.rs` and `orchestration.rs`).
//!
//! Invariants/assumptions:
//! - Called after phase execution completes successfully.
//! - Worker has committed task changes to the workspace branch.
//! - Target branch is the coordinator's base branch.
#![allow(dead_code)]

use crate::config::Resolved;
use crate::contracts::TaskStatus;
use crate::git::{self, WorkspaceSpec};
use crate::queue::{self, operations::complete_task};
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
    /// Maximum number of push attempts.
    pub max_attempts: u32,
    /// Backoff intervals between retries (in milliseconds).
    pub backoff_ms: Vec<u64>,
    /// Target branch to push to.
    pub target_branch: String,
    /// CI gate command (if enabled).
    pub ci_command: Option<String>,
    /// Whether CI gate is enabled.
    pub ci_enabled: bool,
}

impl IntegrationConfig {
    pub fn from_resolved(resolved: &Resolved) -> Self {
        let parallel = &resolved.config.parallel;
        Self {
            max_attempts: parallel.max_push_attempts.unwrap_or(5) as u32,
            backoff_ms: parallel
                .push_backoff_ms
                .clone()
                .unwrap_or_else(super::default_push_backoff_ms),
            target_branch: git::current_branch(&resolved.repo_root)
                .unwrap_or_else(|_| "main".into()),
            ci_command: resolved.config.agent.ci_gate_command.clone(),
            ci_enabled: resolved.config.agent.ci_gate_enabled.unwrap_or(true),
        }
    }

    /// Get backoff for a specific attempt (0-indexed).
    pub fn backoff_for_attempt(&self, attempt: usize) -> Duration {
        let ms = self
            .backoff_ms
            .get(attempt)
            .copied()
            .unwrap_or_else(|| self.backoff_ms.last().copied().unwrap_or(10000));
        Duration::from_millis(ms)
    }
}

// =============================================================================
// Integration Outcome
// =============================================================================

/// Outcome of the integration loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationOutcome {
    /// Push succeeded, task is complete.
    Success,
    /// Push blocked after exhausting retries.
    BlockedPush { reason: String },
    /// Terminal failure (unrecoverable error).
    Failed { reason: String },
}

// =============================================================================
// Handoff Packet
// =============================================================================

/// Structured handoff packet for agent remediation sessions.
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
    /// List of files with conflicts (if any).
    pub conflict_files: Vec<String>,
    /// Current git status output.
    pub git_status: String,
    /// Phase outputs summary (last phase response).
    pub phase_summary: String,
    /// Original task intent snapshot.
    pub task_intent: String,
    /// CI command and last output (for CI remediation).
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
                "Remove completed task from queue.json".into(),
                "Ensure completed task appears in done.json".into(),
                "Preserve other tasks from upstream unchanged".into(),
                "Preserve task metadata (timestamps, notes)".into(),
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
    pub ci_passed: bool,
    pub conflict_files: Vec<String>,
    pub validation_error: Option<String>,
}

impl ComplianceResult {
    pub fn all_passed(&self) -> bool {
        !self.has_unresolved_conflicts && self.queue_done_valid && self.ci_passed
    }
}

/// Run all deterministic compliance checks.
pub fn run_compliance_checks(
    repo_root: &Path,
    resolved: &Resolved,
    _task_id: &str,
    ci_enabled: bool,
) -> Result<ComplianceResult> {
    // Check 1: No unresolved merge conflicts
    let conflict_files = git::list_conflict_files(repo_root)?;
    let has_unresolved_conflicts = !conflict_files.is_empty();

    if has_unresolved_conflicts {
        return Ok(ComplianceResult {
            has_unresolved_conflicts: true,
            queue_done_valid: false,
            ci_passed: false,
            conflict_files,
            validation_error: Some("Unresolved merge conflicts detected".into()),
        });
    }

    // Check 2: Queue/done semantic validation
    let (queue_done_valid, validation_error) =
        match validate_queue_done_semantics(repo_root, resolved) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };

    if !queue_done_valid {
        return Ok(ComplianceResult {
            has_unresolved_conflicts: false,
            queue_done_valid: false,
            ci_passed: false,
            conflict_files: Vec::new(),
            validation_error,
        });
    }

    // Check 3: CI gate (if enabled)
    let ci_passed = if ci_enabled {
        match run_ci_check(repo_root, resolved) {
            Ok(()) => true,
            Err(e) => {
                return Ok(ComplianceResult {
                    has_unresolved_conflicts: false,
                    queue_done_valid: true,
                    ci_passed: false,
                    conflict_files: Vec::new(),
                    validation_error: Some(format!("CI gate failed: {}", e)),
                });
            }
        }
    } else {
        true
    };

    Ok(ComplianceResult {
        has_unresolved_conflicts: false,
        queue_done_valid: true,
        ci_passed,
        conflict_files: Vec::new(),
        validation_error: None,
    })
}

/// Validate queue/done files semantically.
fn validate_queue_done_semantics(repo_root: &Path, resolved: &Resolved) -> Result<()> {
    // In worker context, we validate the workspace-local queue/done
    let queue_path = repo_root.join(".ralph/queue.json");
    let done_path = repo_root.join(".ralph/done.json");

    if queue_path.exists() {
        let queue = queue::load_queue(&queue_path).context("load queue for validation")?;
        let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
        let done = if done_path.exists() {
            Some(queue::load_queue(&done_path).context("load done for validation")?)
        } else {
            None
        };

        // Basic semantic validation
        queue::validate_queue_set(
            &queue,
            done.as_ref(),
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .context("queue/done semantic validation")?;
    }

    Ok(())
}

/// Run CI gate check.
fn run_ci_check(repo_root: &Path, resolved: &Resolved) -> Result<()> {
    let ci_command = resolved
        .config
        .agent
        .ci_gate_command
        .as_deref()
        .unwrap_or("make ci");

    log::info!("Running CI gate: {}", ci_command);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(ci_command)
        .current_dir(repo_root)
        .output()
        .context("spawn CI gate command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("CI gate failed: {}", stderr);
    }

    Ok(())
}

// =============================================================================
// Integration Loop
// =============================================================================

/// Run the integration loop for a completed worker.
///
/// This function implements the bounded retry loop for direct push:
/// 1. Fetch target branch
/// 2. Rebase onto target
/// 3. Resolve conflicts via agent if needed
/// 4. Run CI gate
/// 5. Push to target
pub(crate) fn run_integration_loop(
    resolved: &Resolved,
    workspace: &WorkspaceSpec,
    task_id: &str,
    task_title: &str,
    config: &IntegrationConfig,
    phase_summary: &str,
) -> Result<IntegrationOutcome> {
    let repo_root = &workspace.path;

    for attempt in 0..config.max_attempts {
        log::info!(
            "Integration attempt {}/{} for {}",
            attempt + 1,
            config.max_attempts,
            task_id
        );

        // Step 1: Fetch target branch
        if let Err(e) = git::fetch_branch(repo_root, "origin", &config.target_branch) {
            log::warn!("Fetch failed on attempt {}: {}", attempt + 1, e);
            if attempt + 1 == config.max_attempts {
                return Ok(IntegrationOutcome::BlockedPush {
                    reason: format!("Fetch failed after {} attempts: {}", config.max_attempts, e),
                });
            }
            std::thread::sleep(config.backoff_for_attempt(attempt as usize));
            continue;
        }

        // Step 2: Check if we're behind and need to rebase
        let behind = match git::is_behind_upstream(repo_root, &config.target_branch) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed to check divergence: {}", e);
                if attempt + 1 == config.max_attempts {
                    return Ok(IntegrationOutcome::BlockedPush {
                        reason: format!("Divergence check failed: {}", e),
                    });
                }
                std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                continue;
            }
        };

        if behind {
            // Step 3: Rebase onto target
            if let Err(e) = git::rebase_onto(repo_root, &format!("origin/{}", config.target_branch))
            {
                log::warn!("Rebase failed on attempt {}: {}", attempt + 1, e);

                // Check if it's a conflict
                let conflict_files = git::list_conflict_files(repo_root).unwrap_or_default();
                if !conflict_files.is_empty() {
                    log::info!("Merge conflicts detected: {:?}", conflict_files);

                    // Generate handoff and run remediation
                    let handoff = RemediationHandoff::new(
                        task_id,
                        task_title,
                        &config.target_branch,
                        attempt + 1,
                        config.max_attempts,
                    )
                    .with_conflicts(conflict_files.clone())
                    .with_git_status(git::status_porcelain(repo_root).unwrap_or_default())
                    .with_phase_summary(phase_summary.into())
                    .with_task_intent(format!("Complete task {}: {}", task_id, task_title));

                    if let Err(e) = write_handoff_packet(repo_root, task_id, attempt + 1, &handoff)
                    {
                        log::error!("Failed to write handoff packet: {}", e);
                    }

                    // TODO: Spawn agent remediation session
                    // For now, we fail and let the coordinator retry
                    if attempt + 1 == config.max_attempts {
                        return Ok(IntegrationOutcome::BlockedPush {
                            reason: format!(
                                "Unresolved conflicts after {} attempts",
                                config.max_attempts
                            ),
                        });
                    }

                    // Abort the failed rebase and retry
                    let _ = git::abort_rebase(repo_root);
                    std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                    continue;
                }

                // Non-conflict rebase failure
                if attempt + 1 == config.max_attempts {
                    return Ok(IntegrationOutcome::BlockedPush {
                        reason: format!("Rebase failed: {}", e),
                    });
                }
                std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                continue;
            }
        }

        // Step 4: Run compliance checks
        let compliance = run_compliance_checks(repo_root, resolved, task_id, config.ci_enabled)?;

        if !compliance.all_passed() {
            if compliance.has_unresolved_conflicts {
                log::warn!("Unresolved conflicts after rebase");

                if attempt + 1 == config.max_attempts {
                    return Ok(IntegrationOutcome::BlockedPush {
                        reason: "Unresolved conflicts remain after remediation".into(),
                    });
                }

                // Generate handoff for conflict resolution
                let handoff = RemediationHandoff::new(
                    task_id,
                    task_title,
                    &config.target_branch,
                    attempt + 1,
                    config.max_attempts,
                )
                .with_conflicts(compliance.conflict_files)
                .with_git_status(git::status_porcelain(repo_root).unwrap_or_default())
                .with_phase_summary(phase_summary.into())
                .with_task_intent(format!("Complete task {}: {}", task_id, task_title));

                let _ = write_handoff_packet(repo_root, task_id, attempt + 1, &handoff);

                std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                continue;
            }

            if !compliance.ci_passed {
                let error = compliance
                    .validation_error
                    .unwrap_or_else(|| "CI gate failed".into());
                log::warn!("CI compliance check failed: {}", error);

                // Generate handoff for CI remediation
                let handoff = RemediationHandoff::new(
                    task_id,
                    task_title,
                    &config.target_branch,
                    attempt + 1,
                    config.max_attempts,
                )
                .with_git_status(git::status_porcelain(repo_root).unwrap_or_default())
                .with_phase_summary(phase_summary.into())
                .with_task_intent(format!("Complete task {}: {}", task_id, task_title))
                .with_ci_context(
                    config
                        .ci_command
                        .clone()
                        .unwrap_or_else(|| "make ci".into()),
                    error.clone(),
                    1,
                );

                let _ = write_handoff_packet(repo_root, task_id, attempt + 1, &handoff);

                if attempt + 1 == config.max_attempts {
                    return Ok(IntegrationOutcome::BlockedPush {
                        reason: format!(
                            "CI gate failed after {} attempts: {}",
                            config.max_attempts, error
                        ),
                    });
                }

                std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                continue;
            }

            // Other validation failure
            return Ok(IntegrationOutcome::Failed {
                reason: compliance
                    .validation_error
                    .unwrap_or_else(|| "Validation failed".into()),
            });
        }

        // Step 5: Push to target
        match git::push_current_branch(repo_root, "origin") {
            Ok(()) => {
                log::info!(
                    "Successfully pushed {} to {}",
                    task_id,
                    config.target_branch
                );

                // Finalize task: move from queue to done
                if let Err(e) = finalize_task_in_bookkeeping(resolved, task_id) {
                    log::error!("Failed to finalize task in bookkeeping: {}", e);
                    // Continue anyway - the push succeeded
                }

                return Ok(IntegrationOutcome::Success);
            }
            Err(e) => {
                let error_str = e.to_string();

                // Classify push failure
                if is_non_fast_forward_error(&error_str) {
                    log::warn!("Non-fast-forward push on attempt {}", attempt + 1);

                    if attempt + 1 == config.max_attempts {
                        return Ok(IntegrationOutcome::BlockedPush {
                            reason: format!(
                                "Non-fast-forward push after {} attempts",
                                config.max_attempts
                            ),
                        });
                    }

                    std::thread::sleep(config.backoff_for_attempt(attempt as usize));
                    continue;
                }

                // Non-retryable push failure
                return Ok(IntegrationOutcome::Failed {
                    reason: format!("Push failed: {}", e),
                });
            }
        }
    }

    // Exhausted all attempts
    Ok(IntegrationOutcome::BlockedPush {
        reason: format!("Integration failed after {} attempts", config.max_attempts),
    })
}

/// Check if error is a non-fast-forward rejection.
fn is_non_fast_forward_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("non-fast-forward")
        || lower.contains("fetch first")
        || lower.contains("rejected")
        || lower.contains("stale info")
        || lower.contains("failed to push")
}

/// Finalize task by moving from queue to done.
fn finalize_task_in_bookkeeping(resolved: &Resolved, task_id: &str) -> Result<()> {
    let now = timeutil::now_utc_rfc3339().unwrap_or_else(|_| "2026-01-01T00:00:00Z".into());
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        TaskStatus::Done,
        &now,
        &["[parallel] Completed via direct push".to_string()],
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
        None,
    )
    .context("finalize task in queue/done")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_config_default_backoff() {
        let config = IntegrationConfig {
            max_attempts: 5,
            backoff_ms: vec![500, 2000, 5000, 10000],
            target_branch: "main".into(),
            ci_command: Some("make ci".into()),
            ci_enabled: true,
        };

        assert_eq!(config.backoff_for_attempt(0), Duration::from_millis(500));
        assert_eq!(config.backoff_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(config.backoff_for_attempt(2), Duration::from_millis(5000));
        assert_eq!(config.backoff_for_attempt(3), Duration::from_millis(10000));
        assert_eq!(config.backoff_for_attempt(4), Duration::from_millis(10000)); // last value repeated
        assert_eq!(config.backoff_for_attempt(10), Duration::from_millis(10000)); // last value repeated
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
    fn is_non_fast_forward_error_detection() {
        assert!(is_non_fast_forward_error(
            "error: failed to push some refs. hint: Updates were rejected because the tip of your current branch is behind"
        ));
        assert!(is_non_fast_forward_error(
            "non-fast-forward updates were rejected"
        ));
        assert!(is_non_fast_forward_error("fetch first"));
        assert!(!is_non_fast_forward_error("permission denied"));
        assert!(!is_non_fast_forward_error("repository not found"));
    }

    #[test]
    fn compliance_result_all_passed() {
        let passed = ComplianceResult {
            has_unresolved_conflicts: false,
            queue_done_valid: true,
            ci_passed: true,
            conflict_files: vec![],
            validation_error: None,
        };
        assert!(passed.all_passed());

        let failed_ci = ComplianceResult {
            has_unresolved_conflicts: false,
            queue_done_valid: true,
            ci_passed: false,
            conflict_files: vec![],
            validation_error: None,
        };
        assert!(!failed_ci.all_passed());

        let has_conflicts = ComplianceResult {
            has_unresolved_conflicts: true,
            queue_done_valid: true,
            ci_passed: true,
            conflict_files: vec!["src/lib.rs".into()],
            validation_error: Some("conflicts".into()),
        };
        assert!(!has_conflicts.all_passed());
    }
}
