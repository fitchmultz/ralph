use crate::contracts::Runner;
use crate::fsutil;
use crate::prompts;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Phase1, // Planning
    Phase2, // Implementation
}

#[derive(Debug, Clone)]
pub struct PromptPolicy {
    pub require_repoprompt: bool,
}

pub const RALPH_PHASE1_PLAN_BEGIN: &str = "<<RALPH_PLAN_BEGIN>>";
pub const RALPH_PHASE1_PLAN_END: &str = "<<RALPH_PLAN_END>>";

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
pub fn build_phase1_prompt(
    base_worker_prompt: &str,
    _task_id: &str,
    policy: &PromptPolicy,
) -> String {
    let mut instructions = String::new();

    // 1. Heading
    instructions.push_str("# PLANNING MODE - PHASE 1 OF 2\n\n");

    // 2. RepoPrompt requirement (if enabled)
    if policy.require_repoprompt {
        instructions.push_str(prompts::REPOPROMPT_REQUIRED_INSTRUCTION);
        instructions.push_str("\n\n");
        instructions.push_str(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION);
        instructions.push('\n');
    }

    // 3. Planning-only constraint + Marker requirement
    instructions.push_str(&format!(
        r#"
## OUTPUT REQUIREMENT: PLAN ONLY
You are in Phase 1 (Planning). You must NOT implement the code yet.
Your goal is to understand the task, gather context, and produce a detailed plan.

## STRICT PROHIBITIONS (PHASE 1 ONLY)
**DO NOT DO ANY OF THE FOLLOWING:**
- Create or modify any files (Ralph handles queue bookkeeping)
- Run tests, `make ci`, or any validation commands
- Execute `git add`, `git commit`, or `git push`
- Write, edit, or change any source code, configuration, or documentation files
- Make any implementation changes whatsoever

**NO FILE EDITS ARE ALLOWED IN PHASE 1.**

**IF YOU START IMPLEMENTING:**
- STOP immediately
- Revert any file changes you made
- Return to planning mode
- Only output a plan wrapped in the required markers

## PLAN OUTPUT REQUIREMENT
You MUST output the final plan wrapped in these exact markers:

{begin}
<your plan here>
{end}

**Your output MUST be wrapped in these plan markers.** Without these markers, Phase 1 will fail.

The plan should be detailed enough for Phase 2 implementation.
"#,
        begin = RALPH_PHASE1_PLAN_BEGIN,
        end = RALPH_PHASE1_PLAN_END
    ));

    // 5. Divider and base prompt
    format!("{}\n\n---\n\n{}", instructions.trim(), base_worker_prompt)
}

/// Build the prompt for Phase 2 (Implementation).
pub fn build_phase2_prompt(plan_text: &str, policy: &PromptPolicy) -> String {
    let mut instructions = String::new();

    // 1. Heading
    instructions.push_str("# IMPLEMENTATION MODE - PHASE 2 OF 2\n\n");

    // 2. RepoPrompt requirement (optional in phase 2, but good for consistency)
    if policy.require_repoprompt {
        instructions.push_str(prompts::REPOPROMPT_REQUIRED_INSTRUCTION);
        instructions.push_str("\n\n");
    }

    // 3. Completion workflow
    instructions.push_str(prompts::TASK_COMPLETION_WORKFLOW);
    instructions.push('\n');

    // 4. The Plan
    instructions.push_str("# APPROVED PLAN\n\n");
    instructions.push_str(plan_text);
    instructions.push_str("\n\n---\n\n");

    // 5. Instruction to execute
    instructions.push_str("Proceed with the implementation of the plan above.");

    instructions
}

/// Build the prompt for Single Phase (Plan + Implement).
pub fn build_single_phase_prompt(
    base_worker_prompt: &str,
    _task_id: &str,
    policy: &PromptPolicy,
) -> String {
    let mut instructions = String::new();

    // 1. RepoPrompt requirement (tooling requirement only; no mandated planning tool step)
    if policy.require_repoprompt {
        instructions.push_str(prompts::REPOPROMPT_REQUIRED_INSTRUCTION);
        instructions.push_str("\n\n");
    }

    // 2. Completion workflow
    instructions.push_str(prompts::TASK_COMPLETION_WORKFLOW);
    instructions.push('\n');

    // 3. Single-pass semantics: planning is optional.
    instructions.push_str(
        "You are in single-pass execution mode. You may do brief planning, but you are NOT required to produce a separate plan first. Proceed directly to implementation.\n",
    );

    // 4. Divider and base prompt
    format!("{}\n\n---\n\n{}", instructions.trim(), base_worker_prompt)
}

/// Extract the plan text from the runner's stdout.
///
/// **Strict contract:** Phase 1 must output a plan wrapped in:
/// - `<<RALPH_PLAN_BEGIN>>`
/// - `<<RALPH_PLAN_END>>`
///
/// If markers are missing, we fail the run. This prevents Phase 1 from
/// "accidentally" implementing changes and having the entire transcript cached as a plan.
pub fn extract_plan_text(_runner_kind: Runner, stdout: &str) -> Result<String> {
    let content = stdout;

    if let Some(start_idx) = content.find(RALPH_PHASE1_PLAN_BEGIN) {
        if let Some(end_idx) = content.find(RALPH_PHASE1_PLAN_END) {
            let start = start_idx + RALPH_PHASE1_PLAN_BEGIN.len();
            if start < end_idx {
                let plan = content[start..end_idx].trim();
                if plan.is_empty() {
                    bail!("Extracted plan is empty (markers present but body is empty)");
                }
                return Ok(plan.to_string());
            }
        }
    }

    bail!(
        "Phase 1 plan output missing required markers. The agent must output the plan wrapped in:\n{}\n<plan>\n{}\n",
        RALPH_PHASE1_PLAN_BEGIN,
        RALPH_PHASE1_PLAN_END
    );
}
