//! Prompt flow integration tests.
//!
//! Purpose:
//! - Prompt flow integration tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use ralph::contracts::Config;
use ralph::promptflow::{self, PromptPolicy};
use ralph::prompts;
use tempfile::TempDir;

#[test]
fn build_phase1_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let task_id = "RQ-1234";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: true,
        repoprompt_tool_injection: true,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_phase1_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_phase1_prompt(
        &template,
        base,
        "",
        promptflow::PHASE1_TASK_REFRESH_REQUIRED_INSTRUCTION,
        task_id,
        2,
        &policy,
        &config,
    )
    .unwrap();

    assert!(prompt.contains("PLANNING MODE - PHASE 1 OF 2"));
    assert!(prompt.contains("TASK REFRESH STEP"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
    assert!(prompt.contains("PLAN ONLY"));
    assert!(prompt.contains(".ralph/cache/plans/RQ-1234.md"));
    assert!(prompt.contains(base));
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
}

#[test]
fn build_phase1_prompt_omits_rp_if_disabled() {
    let base = "BASE_PROMPT";
    let task_id = "RQ-1234";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_phase1_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_phase1_prompt(
        &template,
        base,
        "",
        promptflow::PHASE1_TASK_REFRESH_REQUIRED_INSTRUCTION,
        task_id,
        2,
        &policy,
        &config,
    )
    .unwrap();

    assert!(!prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(!prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
}

#[test]
fn build_phase2_prompt_contains_required_elements() {
    let plan = "My Plan";
    let checklist = "## IMPLEMENTATION COMPLETION CHECKLIST\n- done";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: true,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_phase2_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_phase2_prompt(
        &template,
        "BASE_PROMPT",
        plan,
        checklist,
        "",
        "",
        "RQ-1234",
        2,
        &policy,
        &config,
    )
    .unwrap();

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 2"));
    assert!(prompt.contains("CURRENT TASK: RQ-1234"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("APPROVED PLAN"));
    assert!(prompt.contains(plan));
    assert!(prompt.contains("BASE_PROMPT"));
}

#[test]
fn build_single_phase_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let checklist = "## IMPLEMENTATION COMPLETION CHECKLIST\n- done";
    let task_id = "RQ-1234";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: true,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_single_phase_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_single_phase_prompt(
        &template, base, checklist, "", "", task_id, &policy, &config,
    )
    .unwrap();

    assert!(prompt.contains("REPOPROMPT TOOLING (WHEN CONNECTED)"));
    assert!(!prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("single-pass execution mode"));
    assert!(prompt.contains(base));
}

#[test]
fn plan_cache_roundtrip() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let task_id = "RQ-9999";
    let plan = "Cached Plan";

    promptflow::write_plan_cache(root, task_id, plan).unwrap();
    let loaded = promptflow::read_plan_cache(root, task_id).unwrap();

    assert_eq!(loaded, plan);
}

#[test]
fn read_plan_cache_fails_when_missing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let err = promptflow::read_plan_cache(root, "RQ-0000").unwrap_err();
    assert!(err.to_string().contains("Plan cache not found"));
}

#[test]
fn read_plan_cache_fails_when_empty() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let task_id = "RQ-0002";
    promptflow::write_plan_cache(root, task_id, "   ").unwrap();
    let err = promptflow::read_plan_cache(root, task_id).unwrap_err();
    assert!(err.to_string().contains("Plan cache is empty"));
}

#[test]
fn build_phase2_handoff_prompt_contains_required_elements() {
    let plan = "My Plan";
    let checklist = "## PHASE 2 HANDOFF CHECKLIST (3-PHASE WORKFLOW)\n- done";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: false,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_phase2_handoff_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_phase2_handoff_prompt(
        &template,
        "BASE_PROMPT",
        plan,
        checklist,
        "",
        "",
        "RQ-1234",
        3,
        &policy,
        &config,
    )
    .unwrap();

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains("CURRENT TASK: RQ-1234"));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("resolve follow-ups, inconsistencies, missing tests"));
    assert!(prompt.contains("concrete remediation steps"));
    assert!(prompt.contains("APPROVED PLAN"));
    assert!(prompt.contains(plan));
    assert!(prompt.contains("BASE_PROMPT"));
}

#[test]
fn build_phase3_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let review = "CODE REVIEW BODY";
    let phase2_final = "PHASE 2 FINAL";
    let config = Config::default();
    let policy = PromptPolicy {
        repoprompt_plan_required: false,
        repoprompt_tool_injection: true,
    };
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_worker_phase3_prompt(repo_root.path()).unwrap();

    let prompt = promptflow::build_phase3_prompt(
        &template,
        base,
        review,
        phase2_final,
        "RQ-0001",
        "CHECKLIST",
        "",
        "",
        prompts::PHASE3_COMPLETION_GUIDANCE_FINAL,
        3,
        &policy,
        &config,
    )
    .unwrap();

    assert!(prompt.contains("CODE REVIEW MODE - PHASE 3 OF 3"));
    assert!(prompt.contains("CURRENT TASK: RQ-0001"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains("PRE-FLIGHT OVERRIDE"));
    assert!(prompt.contains(review));
    assert!(prompt.contains(phase2_final));
    assert!(prompt.contains("CHECKLIST"));
    assert!(prompt.contains(base));
    assert!(prompt.contains("Leave it unchanged until terminal task bookkeeping is complete."));
    assert!(prompt.contains("PREFERRED: investigate and resolve any risks"));
}

#[test]
fn iteration_checklist_requires_closing_flagged_issues() {
    let config = Config::default();
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_iteration_checklist(repo_root.path()).unwrap();
    let rendered = prompts::render_iteration_checklist(&template, "RQ-0002", &config).unwrap();

    assert!(rendered.contains("PREFERRED: investigate and resolve suspicious leads"));
}

#[test]
fn completion_checklist_requires_closing_flagged_issues() {
    let config = Config::default();
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_completion_checklist(repo_root.path()).unwrap();
    let rendered =
        prompts::render_completion_checklist(&template, "RQ-0003", &config, false).unwrap();

    assert!(rendered.contains("PREFERRED: investigate and resolve any risks"));
    assert!(rendered.contains("Run mode for this session: `normal`"));
}

#[test]
fn phase2_handoff_checklist_discourages_deferrals() {
    let config = Config::default();
    let repo_root = TempDir::new().unwrap();
    let template = prompts::load_phase2_handoff_checklist(repo_root.path()).unwrap();
    let rendered = prompts::render_phase2_handoff_checklist(&template, &config).unwrap();

    assert!(rendered.contains("PREFERRED: resolve follow-ups"));
    assert!(rendered.contains("If you are truly blocked"));
}
