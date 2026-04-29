//! Purpose: worker prompt rendering and validation coverage for prompt command previews.
//!
//! Responsibilities:
//! - Verify worker prompt rendering across phase 1, phase 2, phase 3, and single-pass modes.
//! - Preserve iteration-context and invalid iteration-argument regression coverage.
//!
//! Scope:
//! - `ralph::commands::prompt::build_worker_prompt` behavior only.
//!
//! Usage:
//! - Run via the root `prompt_cmd_test` integration suite.
//!
//! Invariants/Assumptions:
//! - Assertions, prompt fragments, and error-text expectations remain unchanged from the original suite.
//! - Shared fixture setup continues to flow through `make_resolved` and `write_minimal_queue`.

use super::*;

#[test]
fn worker_phase1_includes_plan_cache_path_and_optional_rp() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: None,
            mode: WorkerMode::Phase1,
            repoprompt_plan_required: true,
            repoprompt_tool_injection: true,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("PLANNING MODE - PHASE 1 OF 3"));
    assert!(prompt.contains(".ralph/cache/plans/RQ-0001.md"));
    assert!(prompt.contains(ralph::prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    Ok(())
}

#[test]
fn worker_single_phase_includes_completion_workflow() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: None,
            mode: WorkerMode::Single,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("single-pass execution mode"));
    assert_eq!(
        prompt
            .match_indices("IMPLEMENTATION COMPLETION CHECKLIST")
            .count(),
        1
    );

    assert!(prompt.contains("Task bookkeeping"));
    assert!(prompt.contains("ralph task done"));
    assert!(prompt.contains(".ralph/queue.jsonc"));
    Ok(())
}

#[test]
fn worker_phase2_requires_plan_text() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: true,
            repoprompt_tool_injection: true,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: Some("PLAN BODY".to_string()),
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains("PLAN BODY"));
    assert!(prompt.contains("PHASE 2 HANDOFF CHECKLIST"));
    Ok(())
}

#[test]
fn worker_phase2_uses_placeholder_when_no_plan_found() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains("*No plan file found*"));
    assert!(prompt.contains("No plan file was found at"));
    assert!(prompt.contains("Please proceed with implementation based on the task requirements"));
    assert!(prompt.contains("PHASE 2 HANDOFF CHECKLIST"));
    Ok(())
}

#[test]
fn worker_phase3_includes_code_review_prompt() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase3,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 1,
            iteration_index: 1,
            plan_file: None,
            plan_text: None,
            explain: false,
        },
    )?;

    assert!(prompt.contains("CODE REVIEW MODE - PHASE 3 OF 3"));
    assert!(prompt.contains("CODING STANDARDS"));
    assert!(prompt.contains("PRE-FLIGHT OVERRIDE"));
    Ok(())
}

#[test]
fn worker_phase2_includes_iteration_context_for_followup() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 3,
            iteration_index: 2,
            plan_file: None,
            plan_text: Some("PLAN BODY".to_string()),
            explain: false,
        },
    )?;

    assert!(prompt.contains("REFINEMENT CONTEXT"));
    assert!(prompt.contains("ITERATION COMPLETION RULES"));
    Ok(())
}

#[test]
fn worker_rejects_iterations_zero() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let result = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 0,
            iteration_index: 1,
            plan_file: None,
            plan_text: Some("PLAN".to_string()),
            explain: false,
        },
    );

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("--iterations must be >= 1"),
        "unexpected error: {msg}"
    );
    Ok(())
}

#[test]
fn worker_rejects_iteration_index_zero() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let result = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 3,
            iteration_index: 0,
            plan_file: None,
            plan_text: Some("PLAN".to_string()),
            explain: false,
        },
    );

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("--iteration-index must be >= 1"),
        "unexpected error: {msg}"
    );
    Ok(())
}

#[test]
fn worker_rejects_iteration_index_exceeds_iterations() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let result = prompt_cmd::build_worker_prompt(
        &resolved,
        WorkerPromptOptions {
            task_id: Some("RQ-0001".to_string()),
            mode: WorkerMode::Phase2,
            repoprompt_plan_required: false,
            repoprompt_tool_injection: false,
            iterations: 3,
            iteration_index: 5,
            plan_file: None,
            plan_text: Some("PLAN".to_string()),
            explain: false,
        },
    );

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("--iteration-index (5) cannot exceed --iterations (3)"),
        "unexpected error: {msg}"
    );
    Ok(())
}
