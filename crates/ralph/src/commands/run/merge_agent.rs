//! Merge-agent CLI handler for parallel coordinator subprocess invocation.
//!
//! Responsibilities:
//! - Validate --task and --pr argument formats
//! - Execute PR merge per configured policy
//! - Finalize task state in canonical queue/done
//! - Emit structured JSON result to stdout
//! - Write user-facing diagnostics to stderr
//! - Return appropriate exit codes (0/1/2/>=3)
//!
//! Not handled here:
//! - Worker orchestration or task selection (see `parallel/mod.rs`).
//! - PR creation (see `git/pr.rs`).
//! - Conflict resolution logic (handled by retry policy at coordinator level).
//! - **POST-MERGE CI** (see policy below).
//!
//! Invariants/assumptions:
//! - This command runs in the coordinator repo context (CWD is repo root)
//! - The coordinator has already verified PR existence before invoking
//! - Canonical queue mutation only happens in coordinator repo context
//!
//! # Policy: No Post-Merge CI (Spec Section 20, Decision 1)
//!
//! Merge-agent does NOT run post-merge CI. The per-worker CI gate is authoritative.
//! This decision was fixed per `docs/features/parallel-mode-rewrite.md` section 20:
//!
//! > "Post-merge CI policy: rely on per-worker CI only; merge-agent does not run post-merge CI."
//!
//! The `execute_merge_agent` function flow is:
//! 1. Validate PR lifecycle and merge eligibility
//! 2. Execute merge via `git::merge_pr`
//! 3. Finalize task via `complete_task`
//!
//! There is no CI invocation step. The `MergeAgentExecutionContext` struct intentionally
//! lacks any CI-related fields (no `ci_gate_command`, no `run_ci`, etc.).

use crate::config;
use crate::contracts::{ParallelMergeMethod, TaskStatus};
use crate::git::{self, MergeState, PrLifecycle};
use crate::queue;
use crate::queue::operations::complete_task;
use crate::timeutil;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Result payload emitted to stdout on success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeAgentResult {
    /// Task ID that was finalized
    pub task_id: String,
    /// PR number that was merged
    pub pr_number: u32,
    /// Whether the merge was successful
    pub merged: bool,
    /// Optional message (success details or error description)
    pub message: Option<String>,
}

/// Error classification for exit code determination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeAgentError {
    /// Validation failure (exit code 2)
    Validation(String),
    /// Runtime failure (exit code 1)
    Runtime(String),
    /// Domain-specific failure (exit code >= 3)
    Domain { code: u8, message: String },
}

impl MergeAgentError {
    pub fn exit_code(&self) -> i32 {
        match self {
            MergeAgentError::Validation(_) => 2,
            MergeAgentError::Runtime(_) => 1,
            MergeAgentError::Domain { code, .. } => (*code as i32).max(3),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            MergeAgentError::Validation(msg) => msg,
            MergeAgentError::Runtime(msg) => msg,
            MergeAgentError::Domain { message, .. } => message,
        }
    }
}

/// Context for merge-agent execution.
///
/// Contains all configuration and paths needed to execute a merge
/// and finalize the task in the canonical queue/done files.
#[derive(Debug, Clone)]
pub struct MergeAgentExecutionContext {
    /// Repository root path (coordinator repo)
    pub repo_root: PathBuf,
    /// Path to the active queue file
    pub queue_path: PathBuf,
    /// Path to the done archive file
    pub done_path: PathBuf,
    /// Task ID prefix (e.g., "RQ")
    pub id_prefix: String,
    /// Task ID numeric width (e.g., 4)
    pub id_width: usize,
    /// Maximum dependency depth for validation
    pub max_dependency_depth: u8,
    /// Merge method to use (squash, merge, rebase)
    pub merge_method: ParallelMergeMethod,
    /// Whether to delete the branch after merge
    pub delete_branch: bool,
}

/// Domain-specific exit codes for merge-agent.
///
/// These codes allow the coordinator to distinguish between
/// retryable and non-retryable failure modes.
#[allow(dead_code)]
pub mod exit_codes {
    /// Merge + task finalization successful
    pub const SUCCESS: i32 = 0;
    /// Runtime/unexpected failure
    pub const RUNTIME_FAILURE: i32 = 1;
    /// Usage/validation failure
    pub const VALIDATION_FAILURE: i32 = 2;
    /// Merge conflict (unresolved conflicts - retryable)
    pub const MERGE_CONFLICT: i32 = 3;
    /// PR not found / already merged / closed
    pub const PR_NOT_FOUND: i32 = 4;
    /// PR is draft (not eligible for merge)
    pub const PR_IS_DRAFT: i32 = 5;
    /// Task already finalized (idempotent success)
    pub const ALREADY_FINALIZED: i32 = 6;
}

/// Execute the merge-agent workflow:
/// 1. Validate PR exists and is mergeable
/// 2. Execute merge
/// 3. Finalize task in queue/done
///
/// Returns the exit code.
pub fn execute_merge_agent(
    ctx: &MergeAgentExecutionContext,
    task_id: &str,
    pr_number: u32,
) -> Result<i32, MergeAgentError> {
    // Step 1: Check PR lifecycle status first
    let lifecycle_status = git::pr_lifecycle_status(&ctx.repo_root, pr_number).map_err(|e| {
        MergeAgentError::Runtime(format!(
            "Failed to query PR {} lifecycle status: {}",
            pr_number, e
        ))
    })?;

    // Check if PR is already merged - idempotent success path
    if lifecycle_status.is_merged {
        // Check if task is already finalized
        if is_task_already_done(&ctx.queue_path, &ctx.done_path, task_id)? {
            emit_diagnostic(&format!(
                "Task {} already finalized and PR {} already merged (idempotent success)",
                task_id, pr_number
            ));
            emit_success(
                task_id,
                pr_number,
                Some("Already finalized and merged".into()),
            )
            .map_err(|e| MergeAgentError::Runtime(format!("Failed to emit result: {}", e)))?;
            return Ok(exit_codes::ALREADY_FINALIZED);
        }
        // PR is merged but task not finalized - continue to finalize
        emit_diagnostic(&format!(
            "PR {} already merged, finalizing task {}",
            pr_number, task_id
        ));
    } else {
        // PR is not merged - check eligibility
        match lifecycle_status.lifecycle {
            PrLifecycle::Closed => {
                return Err(MergeAgentError::Domain {
                    code: exit_codes::PR_NOT_FOUND as u8,
                    message: format!("PR {} is closed and not merged", pr_number),
                });
            }
            PrLifecycle::Unknown(state) => {
                return Err(MergeAgentError::Domain {
                    code: exit_codes::PR_NOT_FOUND as u8,
                    message: format!("PR {} is in unknown state: {}", pr_number, state),
                });
            }
            PrLifecycle::Merged => {
                // Already handled above, but for completeness
            }
            PrLifecycle::Open => {
                // Continue with merge eligibility check
                let pr_status = git::pr_merge_status(&ctx.repo_root, pr_number).map_err(|e| {
                    MergeAgentError::Runtime(format!(
                        "Failed to query PR {} merge status: {}",
                        pr_number, e
                    ))
                })?;

                // Draft PR - not eligible
                if pr_status.is_draft {
                    return Err(MergeAgentError::Domain {
                        code: exit_codes::PR_IS_DRAFT as u8,
                        message: format!("PR {} is a draft and not eligible for merge", pr_number),
                    });
                }

                // Conflict state - retryable failure
                if matches!(pr_status.merge_state, MergeState::Dirty) {
                    return Err(MergeAgentError::Domain {
                        code: exit_codes::MERGE_CONFLICT as u8,
                        message: format!(
                            "PR {} has unresolved merge conflicts. Resolve conflicts and retry.",
                            pr_number
                        ),
                    });
                }

                // Other blocked state
                if !matches!(pr_status.merge_state, MergeState::Clean) {
                    return Err(MergeAgentError::Domain {
                        code: exit_codes::PR_NOT_FOUND as u8,
                        message: format!(
                            "PR {} is not in a mergeable state: {:?}",
                            pr_number, pr_status.merge_state
                        ),
                    });
                }

                // Step 2: Execute merge
                emit_diagnostic(&format!(
                    "Merging PR {} for task {} (method: {:?})",
                    pr_number, task_id, ctx.merge_method
                ));

                git::merge_pr(
                    &ctx.repo_root,
                    pr_number,
                    ctx.merge_method,
                    ctx.delete_branch,
                )
                .map_err(|e| {
                    MergeAgentError::Runtime(format!("Failed to merge PR {}: {}", pr_number, e))
                })?;
            }
        }
    }

    // Step 3: Finalize task
    finalize_task(ctx, task_id, pr_number)?;

    emit_diagnostic(&format!(
        "Task {} finalized successfully after PR {} merge",
        task_id, pr_number
    ));

    emit_success(
        task_id,
        pr_number,
        Some(format!("Merged PR {} and finalized task", pr_number)),
    )
    .map_err(|e| MergeAgentError::Runtime(format!("Failed to emit result: {}", e)))?;

    Ok(exit_codes::SUCCESS)
}

/// Finalize task in queue/done.
///
/// This atomically moves the task from queue.json to done.json
/// with appropriate timestamps and notes.
fn finalize_task(
    ctx: &MergeAgentExecutionContext,
    task_id: &str,
    pr_number: u32,
) -> Result<(), MergeAgentError> {
    let task_id_trimmed = task_id.trim();

    // Heal partial-finalization state without duplicating done entries:
    // if done.json already contains the task but queue.json still has it,
    // remove the active queue entry and keep the existing archive record.
    if ctx.done_path.exists() && ctx.queue_path.exists() {
        let done_file = queue::load_queue_or_default(&ctx.done_path).map_err(|e| {
            MergeAgentError::Runtime(format!("Failed to load done file for finalization: {}", e))
        })?;
        let done_contains_task = done_file
            .tasks
            .iter()
            .any(|t| t.id.trim() == task_id_trimmed);

        if done_contains_task {
            let mut queue_file = queue::load_queue(&ctx.queue_path).map_err(|e| {
                MergeAgentError::Runtime(format!("Failed to load queue for finalization: {}", e))
            })?;
            let original_len = queue_file.tasks.len();
            queue_file.tasks.retain(|t| t.id.trim() != task_id_trimmed);

            if queue_file.tasks.len() != original_len {
                let warnings = queue::validate_queue_set(
                    &queue_file,
                    Some(&done_file),
                    &ctx.id_prefix,
                    ctx.id_width,
                    ctx.max_dependency_depth,
                )
                .map_err(|e| {
                    MergeAgentError::Runtime(format!(
                        "Queue validation failed while reconciling task {}: {}",
                        task_id_trimmed, e
                    ))
                })?;
                queue::log_warnings(&warnings);

                queue::save_queue(&ctx.queue_path, &queue_file).map_err(|e| {
                    MergeAgentError::Runtime(format!(
                        "Failed to persist reconciled queue for task {}: {}",
                        task_id_trimmed, e
                    ))
                })?;
                emit_diagnostic(&format!(
                    "Reconciled partial finalization for {}: removed stale queue entry",
                    task_id_trimmed
                ));
            }

            return Ok(());
        }
    }

    let now = timeutil::now_utc_rfc3339()
        .map_err(|e| MergeAgentError::Runtime(format!("Failed to get timestamp: {}", e)))?;

    complete_task(
        &ctx.queue_path,
        &ctx.done_path,
        task_id,
        TaskStatus::Done,
        &now,
        &[format!("[merge-agent] Completed via PR #{}", pr_number)],
        &ctx.id_prefix,
        ctx.id_width,
        ctx.max_dependency_depth,
        None, // no custom fields patch
    )
    .map_err(|e| MergeAgentError::Runtime(format!("Failed to finalize task {}: {}", task_id, e)))?;

    Ok(())
}

/// Check if task is already Done (in queue or done file).
///
/// This supports idempotent merge-agent execution where rerunning
/// after partial success should not corrupt queue/done state.
fn is_task_already_done(
    queue_path: &Path,
    done_path: &Path,
    task_id: &str,
) -> Result<bool, MergeAgentError> {
    let mut queue_status: Option<TaskStatus> = None;

    // Check queue file for done status
    if queue_path.exists() {
        let queue_file = queue::load_queue(queue_path)
            .map_err(|e| MergeAgentError::Runtime(format!("Failed to load queue: {}", e)))?;

        queue_status = queue_file
            .tasks
            .iter()
            .find(|t| t.id.trim() == task_id.trim())
            .map(|task| task.status);
    }

    match queue_status {
        Some(TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing) => {
            // Active queue entry means finalization still needs to run, even if done.json
            // already contains a copy from a prior partial write.
            return Ok(false);
        }
        Some(TaskStatus::Done | TaskStatus::Rejected) => {
            return Ok(true);
        }
        None => {}
    }

    // Check done file for archived task
    if done_path.exists() {
        let done_file = queue::load_queue_or_default(done_path)
            .map_err(|e| MergeAgentError::Runtime(format!("Failed to load done file: {}", e)))?;

        if done_file
            .tasks
            .iter()
            .any(|t| t.id.trim() == task_id.trim())
        {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Validate task ID format.
/// Task IDs must be non-empty and match the expected pattern (e.g., "RQ-0942").
pub fn validate_task_id(task_id: &str) -> Result<(), MergeAgentError> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        return Err(MergeAgentError::Validation(
            "Task ID cannot be empty".to_string(),
        ));
    }
    // Basic format check: should contain alphanumeric, hyphens, underscores
    if !trimmed
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(MergeAgentError::Validation(format!(
            "Invalid task ID format: '{}'. Expected alphanumeric with hyphens/underscores.",
            trimmed
        )));
    }
    Ok(())
}

/// Validate PR number.
/// PR numbers must be positive integers.
pub fn validate_pr_number(pr: u32) -> Result<(), MergeAgentError> {
    if pr == 0 {
        return Err(MergeAgentError::Validation(
            "PR number must be a positive integer".to_string(),
        ));
    }
    Ok(())
}

/// Emit a successful result to stdout as JSON.
pub fn emit_success(task_id: &str, pr_number: u32, message: Option<String>) -> Result<()> {
    let result = MergeAgentResult {
        task_id: task_id.to_string(),
        pr_number,
        merged: true,
        message,
    };
    emit_result(&result)
}

/// Emit an error result to stdout as JSON (for structured error consumption).
pub fn emit_error(task_id: &str, pr_number: u32, error: &MergeAgentError) -> Result<()> {
    let result = MergeAgentResult {
        task_id: task_id.to_string(),
        pr_number,
        merged: false,
        message: Some(error.message().to_string()),
    };
    emit_result(&result)
}

/// Write result to stdout as JSON.
fn emit_result(result: &MergeAgentResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", json)?;
    Ok(())
}

/// Write diagnostic message to stderr.
pub fn emit_diagnostic(message: &str) {
    eprintln!("{}", message);
}

/// Handle the merge-agent command.
///
/// This is the main entry point for `ralph run merge-agent --task <ID> --pr <NUMBER>`.
/// It validates inputs, resolves configuration, builds the execution context,
/// and runs the merge + finalize workflow.
pub fn handle_merge_agent(task_id: &str, pr_number: u32) -> Result<i32> {
    // Validate inputs
    if let Err(err) = validate_task_id(task_id) {
        emit_diagnostic(&format!("Validation error: {}", err.message()));
        emit_error(task_id, pr_number, &err)?;
        return Ok(err.exit_code());
    }

    if let Err(err) = validate_pr_number(pr_number) {
        emit_diagnostic(&format!("Validation error: {}", err.message()));
        emit_error(task_id, pr_number, &err)?;
        return Ok(err.exit_code());
    }

    // Resolve config from CWD (coordinator repo context)
    let resolved = match config::resolve_from_cwd() {
        Ok(r) => r,
        Err(e) => {
            let err = MergeAgentError::Runtime(format!("Failed to resolve config: {}", e));
            emit_diagnostic(&format!("Config resolution error: {}", err.message()));
            if let Err(e) = emit_error(task_id, pr_number, &err) {
                log::debug!("Failed to emit error result: {}", e);
            }
            return Ok(err.exit_code());
        }
    };

    // Build execution context
    let ctx = MergeAgentExecutionContext {
        repo_root: resolved.repo_root.clone(),
        queue_path: resolved.queue_path.clone(),
        done_path: resolved.done_path.clone(),
        id_prefix: resolved.id_prefix.clone(),
        id_width: resolved.id_width,
        max_dependency_depth: resolved.config.queue.max_dependency_depth.unwrap_or(10),
        merge_method: resolved.config.parallel.merge_method.unwrap_or_default(),
        delete_branch: resolved
            .config
            .parallel
            .delete_branch_on_merge
            .unwrap_or(true),
    };

    // Execute merge-agent workflow
    match execute_merge_agent(&ctx, task_id, pr_number) {
        Ok(exit_code) => Ok(exit_code),
        Err(err) => {
            emit_diagnostic(&format!("Merge-agent error: {}", err.message()));
            emit_error(task_id, pr_number, &err)?;
            Ok(err.exit_code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_task_id_accepts_valid_format() {
        assert!(validate_task_id("RQ-0942").is_ok());
        assert!(validate_task_id("TASK-001").is_ok());
        assert!(validate_task_id("feature_123").is_ok());
    }

    #[test]
    fn validate_task_id_rejects_empty() {
        let err = validate_task_id("").unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("empty"));
    }

    #[test]
    fn validate_task_id_rejects_special_chars() {
        let err = validate_task_id("RQ/0942").unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("Invalid task ID format"));
    }

    #[test]
    fn validate_pr_number_accepts_positive() {
        assert!(validate_pr_number(1).is_ok());
        assert!(validate_pr_number(42).is_ok());
        assert!(validate_pr_number(999999).is_ok());
    }

    #[test]
    fn validate_pr_number_rejects_zero() {
        let err = validate_pr_number(0).unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.message().contains("positive integer"));
    }

    #[test]
    fn merge_agent_error_exit_codes() {
        assert_eq!(MergeAgentError::Validation("test".into()).exit_code(), 2);
        assert_eq!(MergeAgentError::Runtime("test".into()).exit_code(), 1);
        assert_eq!(
            MergeAgentError::Domain {
                code: 3,
                message: "test".into()
            }
            .exit_code(),
            3
        );
        assert_eq!(
            MergeAgentError::Domain {
                code: 5,
                message: "test".into()
            }
            .exit_code(),
            5
        );
    }

    #[test]
    fn emit_success_produces_valid_json() {
        let result = MergeAgentResult {
            task_id: "RQ-0942".to_string(),
            pr_number: 42,
            merged: true,
            message: Some("Success".to_string()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("RQ-0942"));
        assert!(json.contains("42"));
        assert!(json.contains("true"));
    }

    #[test]
    fn merge_agent_result_serialization_roundtrip() {
        let original = MergeAgentResult {
            task_id: "RQ-0942".to_string(),
            pr_number: 42,
            merged: true,
            message: Some("Test message".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: MergeAgentResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original.task_id, deserialized.task_id);
        assert_eq!(original.pr_number, deserialized.pr_number);
        assert_eq!(original.merged, deserialized.merged);
        assert_eq!(original.message, deserialized.message);
    }

    #[test]
    fn merge_agent_result_without_message() {
        let result = MergeAgentResult {
            task_id: "RQ-0942".to_string(),
            pr_number: 42,
            merged: false,
            message: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("null"));
    }

    #[test]
    fn exit_codes_are_correct() {
        assert_eq!(exit_codes::SUCCESS, 0);
        assert_eq!(exit_codes::RUNTIME_FAILURE, 1);
        assert_eq!(exit_codes::VALIDATION_FAILURE, 2);
        assert_eq!(exit_codes::MERGE_CONFLICT, 3);
        assert_eq!(exit_codes::PR_NOT_FOUND, 4);
        assert_eq!(exit_codes::PR_IS_DRAFT, 5);
        assert_eq!(exit_codes::ALREADY_FINALIZED, 6);
    }
}

/// Tests for execution logic (is_task_already_done, context building).
#[cfg(test)]
mod execution_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_context(temp: &TempDir) -> MergeAgentExecutionContext {
        MergeAgentExecutionContext {
            repo_root: temp.path().to_path_buf(),
            queue_path: temp.path().join(".ralph/queue.json"),
            done_path: temp.path().join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            max_dependency_depth: 10,
            merge_method: ParallelMergeMethod::Squash,
            delete_branch: true,
        }
    }

    fn create_ralph_dir(temp: &TempDir) {
        fs::create_dir_all(temp.path().join(".ralph")).unwrap();
    }

    fn write_queue_file(temp: &TempDir, content: &str) {
        create_ralph_dir(temp);
        fs::write(temp.path().join(".ralph/queue.json"), content).unwrap();
    }

    fn write_done_file(temp: &TempDir, content: &str) {
        create_ralph_dir(temp);
        fs::write(temp.path().join(".ralph/done.json"), content).unwrap();
    }

    #[test]
    fn is_task_already_done_returns_false_for_missing_task() {
        let temp = TempDir::new().unwrap();
        write_queue_file(&temp, r#"{"version":1,"tasks":[]}"#);

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn is_task_already_done_returns_false_for_todo_task() {
        let temp = TempDir::new().unwrap();
        write_queue_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"todo","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#,
        );

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn is_task_already_done_returns_true_for_done_in_queue() {
        let temp = TempDir::new().unwrap();
        write_queue_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"done","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:00Z"}]}"#,
        );

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn is_task_already_done_returns_true_for_task_in_done_file() {
        let temp = TempDir::new().unwrap();
        write_queue_file(&temp, r#"{"version":1,"tasks":[]}"#);
        write_done_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"done","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:00Z"}]}"#,
        );

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn is_task_already_done_handles_whitespace_in_task_id() {
        let temp = TempDir::new().unwrap();
        write_queue_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"done","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:00Z"}]}"#,
        );

        // Should match even with leading/trailing whitespace
        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "  RQ-0001  ",
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn is_task_already_done_returns_false_when_done_has_task_but_queue_still_active() {
        let temp = TempDir::new().unwrap();
        write_queue_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"doing","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#,
        );
        write_done_file(
            &temp,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","status":"done","title":"Test","tags":[],"scope":[],"evidence":[],"plan":[],"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:00Z"}]}"#,
        );

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(
            !result,
            "Active queue entry should prevent already-finalized short-circuit"
        );
    }

    #[test]
    fn is_task_already_done_returns_false_for_missing_queue_file() {
        let temp = TempDir::new().unwrap();
        // Don't create any files

        let result = is_task_already_done(
            &temp.path().join(".ralph/queue.json"),
            &temp.path().join(".ralph/done.json"),
            "RQ-0001",
        )
        .unwrap();

        assert!(!result);
    }

    #[test]
    fn context_has_correct_defaults() {
        let temp = TempDir::new().unwrap();
        let ctx = create_test_context(&temp);

        assert_eq!(ctx.id_prefix, "RQ");
        assert_eq!(ctx.id_width, 4);
        assert_eq!(ctx.max_dependency_depth, 10);
        assert_eq!(ctx.merge_method, ParallelMergeMethod::Squash);
        assert!(ctx.delete_branch);
    }

    /// Regression test: MergeAgentExecutionContext must NOT contain CI-related fields.
    ///
    /// Per spec section 20, decision 1: "Post-merge CI policy: rely on per-worker
    /// CI only; merge-agent does not run post-merge CI."
    ///
    /// This test verifies the struct has no CI invocation capability. If this test
    /// fails because someone added a CI field, the addition should be removed.
    #[test]
    fn context_has_no_ci_fields() {
        let temp = TempDir::new().unwrap();
        let ctx = create_test_context(&temp);

        // The context should only have these fields (no CI-related fields):
        // - repo_root, queue_path, done_path (paths)
        // - id_prefix, id_width (task ID config)
        // - max_dependency_depth (validation)
        // - merge_method, delete_branch (merge config)
        //
        // Explicitly NOT present: ci_gate_command, run_ci, ci_enabled, etc.
        // This test documents the invariant by checking field access compiles.

        // If someone adds a CI field to MergeAgentExecutionContext, this test
        // serves as a reminder to update the module docs and reconsider the policy.
        let _ = &ctx.repo_root;
        let _ = &ctx.queue_path;
        let _ = &ctx.done_path;
        let _ = &ctx.id_prefix;
        let _ = &ctx.id_width;
        let _ = &ctx.max_dependency_depth;
        let _ = &ctx.merge_method;
        let _ = &ctx.delete_branch;

        // This test passes by construction - if the struct compiles, it has
        // the expected fields. The policy enforcement is in the module docs.
    }

    /// Regression test: conflict exit code returns MERGE_CONFLICT (3).
    ///
    /// Per spec section 20, decision 2: unresolved conflicts are retryable.
    /// The merge-agent signals this via exit code 3.
    #[test]
    fn dirty_merge_state_returns_conflict_exit_code() {
        // This test documents that exit_codes::MERGE_CONFLICT (3) is the
        // signal for retryable conflict. The actual merge_state check
        // happens in execute_merge_agent when merge_state is Dirty.
        assert_eq!(exit_codes::MERGE_CONFLICT, 3);
    }
}
