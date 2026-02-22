//! Prompt construction for worker run phases.

use crate::contracts::Config;
use crate::fsutil;
use crate::prompts;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Phase1, // Planning
    Phase2, // Implementation
    Phase3, // Code review
}

#[derive(Debug, Clone)]
pub struct PromptPolicy {
    pub repoprompt_plan_required: bool,
    pub repoprompt_tool_injection: bool,
}

pub const PHASE1_TASK_REFRESH_REQUIRED_INSTRUCTION: &str = r#"## TASK REFRESH STEP (REQUIRED BEFORE PLANNING)
Before producing the final plan, update only the current task in `.ralph/queue.jsonc`:
- Refresh only: `scope`, `evidence`, `plan`, `notes`, `tags`, `depends_on`
- Set `updated_at` to current UTC RFC3339 time
- Preserve task identity/status fields (`id`, `title`, `status`, `priority`, `created_at`, `request`, `agent`)
- Do not add or remove tasks

After updating the task, re-read the updated task data and then produce the final plan."#;

pub const PHASE1_TASK_REFRESH_DISABLED_INSTRUCTION: &str = r#"## TASK REFRESH STEP
Parallel worker mode is active for this run. Do NOT edit `.ralph/queue.jsonc`.
Use current task metadata as-is and continue with planning only."#;

/// Path to the cached plan for a given task ID.
pub fn plan_cache_path(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root
        .join(".ralph/cache/plans")
        .join(format!("{}.md", task_id))
}

/// Write a plan to the cache.
pub fn write_plan_cache(repo_root: &Path, task_id: &str, plan_text: &str) -> Result<()> {
    let path = plan_cache_path(repo_root, task_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    fsutil::write_atomic(&path, plan_text.as_bytes())?;
    Ok(())
}

/// Path to the cached Phase 2 final response for a given task ID.
pub fn phase2_final_response_cache_path(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root
        .join(".ralph/cache/phase2_final")
        .join(format!("{}.md", task_id))
}

/// Write the Phase 2 final response to the cache.
pub fn write_phase2_final_response_cache(
    repo_root: &Path,
    task_id: &str,
    response_text: &str,
) -> Result<()> {
    let path = phase2_final_response_cache_path(repo_root, task_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    fsutil::write_atomic(&path, response_text.as_bytes())?;
    Ok(())
}

/// Read the Phase 2 final response from the cache. Fails if missing or empty.
pub fn read_phase2_final_response_cache(repo_root: &Path, task_id: &str) -> Result<String> {
    let path = phase2_final_response_cache_path(repo_root, task_id);
    if !path.exists() {
        bail!(
            "Phase 2 final response cache not found at {}",
            path.display()
        );
    }
    let content = std::fs::read_to_string(&path)?;
    if content.trim().is_empty() {
        bail!(
            "Phase 2 final response cache is empty at {}",
            path.display()
        );
    }
    Ok(content)
}

/// Read a plan from the cache. Fails if missing or empty.
pub fn read_plan_cache(repo_root: &Path, task_id: &str) -> Result<String> {
    let path = plan_cache_path(repo_root, task_id);
    if !path.exists() {
        bail!("Plan cache not found at {}", path.display());
    }
    let content = std::fs::read_to_string(&path)?;
    if content.trim().is_empty() {
        bail!("Plan cache is empty at {}", path.display());
    }
    Ok(content)
}

/// Build the prompt for Phase 1 (Planning).
#[allow(clippy::too_many_arguments)]
pub fn build_phase1_prompt(
    template: &str,
    base_worker_prompt: &str,
    iteration_context: &str,
    task_refresh_instruction: &str,
    task_id: &str,
    total_phases: u8,
    policy: &PromptPolicy,
    config: &Config,
) -> Result<String> {
    let plan_path = format!(".ralph/cache/plans/{}.md", task_id.trim());
    prompts::render_worker_phase1_prompt(
        template,
        base_worker_prompt,
        iteration_context,
        task_refresh_instruction,
        task_id,
        total_phases,
        &plan_path,
        policy.repoprompt_plan_required,
        policy.repoprompt_tool_injection,
        config,
    )
}

/// Build the prompt for Phase 2 (Implementation).
#[allow(clippy::too_many_arguments)]
pub fn build_phase2_prompt(
    template: &str,
    base_worker_prompt: &str,
    plan_text: &str,
    completion_checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    policy: &PromptPolicy,
    config: &Config,
) -> Result<String> {
    prompts::render_worker_phase2_prompt(
        template,
        base_worker_prompt,
        plan_text,
        completion_checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        total_phases,
        policy.repoprompt_tool_injection,
        config,
    )
}

/// Build the prompt for Phase 2 handoff (3-phase workflow).
#[allow(clippy::too_many_arguments)]
pub fn build_phase2_handoff_prompt(
    template: &str,
    base_worker_prompt: &str,
    plan_text: &str,
    handoff_checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    policy: &PromptPolicy,
    config: &Config,
) -> Result<String> {
    prompts::render_worker_phase2_handoff_prompt(
        template,
        base_worker_prompt,
        plan_text,
        handoff_checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        total_phases,
        policy.repoprompt_tool_injection,
        config,
    )
}

/// Build the prompt for Phase 3 (Code Review).
#[allow(clippy::too_many_arguments)]
pub fn build_phase3_prompt(
    template: &str,
    base_worker_prompt: &str,
    code_review_body: &str,
    phase2_final_response: &str,
    task_id: &str,
    completion_checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    total_phases: u8,
    policy: &PromptPolicy,
    config: &Config,
) -> Result<String> {
    prompts::render_worker_phase3_prompt(
        template,
        base_worker_prompt,
        code_review_body,
        phase2_final_response,
        task_id,
        completion_checklist,
        iteration_context,
        iteration_completion_block,
        phase3_completion_guidance,
        total_phases,
        policy.repoprompt_tool_injection,
        config,
    )
}

/// Build the prompt for Single Phase (Plan + Implement).
#[allow(clippy::too_many_arguments)]
pub fn build_single_phase_prompt(
    template: &str,
    base_worker_prompt: &str,
    completion_checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    policy: &PromptPolicy,
    config: &Config,
) -> Result<String> {
    prompts::render_worker_single_phase_prompt(
        template,
        base_worker_prompt,
        completion_checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        policy.repoprompt_tool_injection,
        config,
    )
}

/// Build the prompt for merge conflict resolution.
pub fn build_merge_conflict_prompt(
    template: &str,
    conflict_files: &[String],
    config: &Config,
) -> Result<String> {
    prompts::render_merge_conflict_prompt(template, conflict_files, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn phase2_final_response_cache_round_trip() -> Result<()> {
        let dir = TempDir::new()?;
        write_phase2_final_response_cache(dir.path(), "RQ-0001", "done")?;
        let read = read_phase2_final_response_cache(dir.path(), "RQ-0001")?;
        assert_eq!(read, "done");
        Ok(())
    }

    #[test]
    fn phase2_final_response_cache_missing_is_error() -> Result<()> {
        let dir = TempDir::new()?;
        let err = read_phase2_final_response_cache(dir.path(), "RQ-0001").unwrap_err();
        assert!(
            err.to_string()
                .contains("Phase 2 final response cache not found")
        );
        Ok(())
    }

    #[test]
    fn phase2_final_response_cache_empty_is_error() -> Result<()> {
        let dir = TempDir::new()?;
        let path = phase2_final_response_cache_path(dir.path(), "RQ-0001");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, "")?;
        let err = read_phase2_final_response_cache(dir.path(), "RQ-0001").unwrap_err();
        assert!(
            err.to_string()
                .contains("Phase 2 final response cache is empty")
        );
        Ok(())
    }

    #[test]
    fn build_merge_conflict_prompt_replaces_conflicts() -> Result<()> {
        let template = "Conflicts:\n{{CONFLICT_FILES}}\n";
        let config = Config::default();
        let files = vec!["src/lib.rs".to_string()];
        let prompt = build_merge_conflict_prompt(template, &files, &config)?;
        assert!(prompt.contains("- src/lib.rs"));
        assert!(!prompt.contains("{{CONFLICT_FILES}}"));
        Ok(())
    }
}
