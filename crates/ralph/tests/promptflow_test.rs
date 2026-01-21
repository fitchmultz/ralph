use ralph::contracts::Runner;
use ralph::promptflow::{self, PromptPolicy, RALPH_PHASE1_PLAN_BEGIN, RALPH_PHASE1_PLAN_END};
use ralph::prompts;
use tempfile::TempDir;

#[test]
fn build_phase1_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let task_id = "RQ-1234";
    let policy = PromptPolicy {
        require_repoprompt: true,
    };

    let prompt = promptflow::build_phase1_prompt(base, task_id, &policy);

    assert!(prompt.contains("PLANNING MODE - PHASE 1 OF 2"));
    assert!(prompt.contains("NO FILE EDITS ARE ALLOWED IN PHASE 1"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
    assert!(prompt.contains("PLAN ONLY"));
    assert!(prompt.contains(RALPH_PHASE1_PLAN_BEGIN));
    assert!(prompt.contains(RALPH_PHASE1_PLAN_END));
    assert!(prompt.contains(base));
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
}

#[test]
fn build_phase1_prompt_omits_rp_if_disabled() {
    let base = "BASE_PROMPT";
    let task_id = "RQ-1234";
    let policy = PromptPolicy {
        require_repoprompt: false,
    };

    let prompt = promptflow::build_phase1_prompt(base, task_id, &policy);

    assert!(!prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(!prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
}

#[test]
fn build_phase2_prompt_contains_required_elements() {
    let plan = "My Plan";
    let checklist = "## IMPLEMENTATION COMPLETION CHECKLIST\n- done";
    let policy = PromptPolicy {
        require_repoprompt: true,
    };

    let prompt = promptflow::build_phase2_prompt(plan, checklist, &policy);

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 2"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("APPROVED PLAN"));
    assert!(prompt.contains(plan));
}

#[test]
fn build_single_phase_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let checklist = "## IMPLEMENTATION COMPLETION CHECKLIST\n- done";
    let task_id = "RQ-1234";
    let policy = PromptPolicy {
        require_repoprompt: true,
    };

    let prompt = promptflow::build_single_phase_prompt(base, checklist, task_id, &policy);

    assert!(prompt.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!prompt.contains(prompts::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("single-pass execution mode"));
    assert!(prompt.contains(base));
}

#[test]
fn extract_plan_text_extracts_between_markers() {
    let output = format!(
        "Some text\n{}\nTHE PLAN\n{}
More text",
        RALPH_PHASE1_PLAN_BEGIN, RALPH_PHASE1_PLAN_END
    );

    let plan = promptflow::extract_plan_text(Runner::Claude, &output).unwrap();
    assert_eq!(plan, "THE PLAN");
}

#[test]
fn extract_plan_text_requires_markers() {
    let output = "  THE PLAN  ";
    assert!(promptflow::extract_plan_text(Runner::Claude, output).is_err());
}

#[test]
fn extract_plan_text_fails_empty() {
    let output = "   ";
    assert!(promptflow::extract_plan_text(Runner::Claude, output).is_err());
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
fn build_phase2_handoff_prompt_contains_required_elements() {
    let plan = "My Plan";
    let checklist = "## PHASE 2 HANDOFF CHECKLIST (3-PHASE WORKFLOW)\n- done";
    let policy = PromptPolicy {
        require_repoprompt: false,
    };

    let prompt = promptflow::build_phase2_handoff_prompt(plan, checklist, &policy);

    assert!(prompt.contains("IMPLEMENTATION MODE - PHASE 2 OF 3"));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains("APPROVED PLAN"));
    assert!(prompt.contains(plan));
}

#[test]
fn build_phase3_prompt_contains_required_elements() {
    let base = "BASE_PROMPT";
    let review = "CODE REVIEW BODY";
    let checklist = "## IMPLEMENTATION COMPLETION CHECKLIST\n- done";
    let policy = PromptPolicy {
        require_repoprompt: true,
    };

    let prompt = promptflow::build_phase3_prompt(base, review, checklist, &policy, "RQ-0001");

    assert!(prompt.contains("CODE REVIEW MODE - PHASE 3 OF 3"));
    assert!(prompt.contains(prompts::REPOPROMPT_REQUIRED_INSTRUCTION));
    assert!(prompt.contains("PRE-FLIGHT OVERRIDE"));
    assert!(prompt.contains(review));
    assert!(prompt.contains(checklist));
    assert!(prompt.contains(base));
}
