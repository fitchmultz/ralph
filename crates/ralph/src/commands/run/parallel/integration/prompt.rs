//! Prompt assembly for parallel integration retries.
//!
//! Purpose:
//! - Prompt assembly for parallel integration retries.
//!
//! Responsibilities:
//! - Build the mandatory integration continuation prompt.
//! - Summarize compliance failures into a compact retry reason.
//!
//! Non-scope:
//! - Running the continuation session.
//! - Reading or writing integration markers.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::path::Path;

use super::compliance::ComplianceResult;

#[allow(clippy::too_many_arguments)]
pub fn build_agent_integration_prompt(
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
You are finalizing task `{task_id}` (`{task_title}`) for integration into `origin/{target_branch}`.

## Hard Requirement
You MUST execute integration git operations yourself in this turn. Do not stop early.
You are NOT done until all required checks are satisfied.
Ralph will reconcile queue/done bookkeeping and push after your turn returns.

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
   - Continue rebase until complete (`git add ...`, `git rebase --continue`).
4. Do not manually edit shared bookkeeping:
   - Leave `{queue_path_display}` and `{done_path_display}` alone unless they have conflict markers that must be resolved to complete the rebase.
   - Ralph will rebuild those files from the latest target branch and archive `{task_id}` after your turn.
5. Stage and commit any remaining implementation changes needed for integration.
6. {ci_block}
7. Do not push. Stop after the workspace is rebased, conflict-free, committed, and CI-clean.

## Completion Contract (Mandatory)
Before ending your response:
- No unresolved merge conflicts remain.
- Implementation changes are committed locally.
- Shared bookkeeping files are not manually rewritten.
- CI has passed when enabled.

If any check fails, keep working in this same turn until fixed.
"#
    ))
}

pub fn compose_block_reason(
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
