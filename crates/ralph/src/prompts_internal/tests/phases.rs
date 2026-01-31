//! Phase-specific worker prompt rendering tests.
//!
//! Responsibilities: validate phase 1, 2, 2-handoff, 3, and single-phase prompt rendering.
//! Not handled: base worker prompts, task builder, or scan prompts.
//! Invariants/assumptions: phase templates contain expected placeholders; config expansion works.

use super::*;

#[test]
fn render_worker_phase1_prompt_replaces_placeholders() -> Result<()> {
    let template =
        "ID={{TASK_ID}}\nPHASE={{TOTAL_PHASES}}\n{{ITERATION_CONTEXT}}\nPLAN={{PLAN_PATH}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase1_prompt(
        template,
        "BASE",
        "ITERATION",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        true,
        true,
        &config,
    )?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PHASE=2"));
    assert!(rendered.contains("PLAN=.ralph/cache/plans/RQ-0001.md"));
    assert!(rendered.contains("BASE"));
    assert!(rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(rendered.contains("PLANNING REQUIREMENT"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_phase1_prompt_handles_repoprompt_flag_combinations() -> Result<()> {
    let template = "{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();

    let plan_only = render_worker_phase1_prompt(
        template,
        "BASE",
        "",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        true,
        false,
        &config,
    )?;
    assert!(plan_only.contains("PLANNING REQUIREMENT"));
    assert!(!plan_only.contains("TOOLING REQUIREMENT: RepoPrompt"));

    let tool_only = render_worker_phase1_prompt(
        template,
        "BASE",
        "",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        false,
        true,
        &config,
    )?;
    assert!(tool_only.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!tool_only.contains("PLANNING REQUIREMENT"));

    let none = render_worker_phase1_prompt(
        template,
        "BASE",
        "",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        false,
        false,
        &config,
    )?;
    assert!(!none.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!none.contains("PLANNING REQUIREMENT"));
    Ok(())
}

#[test]
fn render_worker_phase1_prompt_allows_placeholder_like_base_prompt() -> Result<()> {
    let template =
        "ID={{TASK_ID}}\nPHASE={{TOTAL_PHASES}}\nPLAN={{PLAN_PATH}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let base_prompt = "BASE {{ITERATION_COMPLETION_BLOCK}}";
    let rendered = render_worker_phase1_prompt(
        template,
        base_prompt,
        "",
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        false,
        false,
        &config,
    )?;
    assert!(rendered.contains(base_prompt));
    Ok(())
}

#[test]
fn render_worker_phase1_prompt_allows_placeholder_like_iteration_context() -> Result<()> {
    let template =
        "ID={{TASK_ID}}\nPHASE={{TOTAL_PHASES}}\n{{ITERATION_CONTEXT}}\nPLAN={{PLAN_PATH}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let iteration_context = "ITERATION {{PLACEHOLDER}}";
    let rendered = render_worker_phase1_prompt(
        template,
        "BASE",
        iteration_context,
        "RQ-0001",
        2,
        ".ralph/cache/plans/RQ-0001.md",
        false,
        false,
        &config,
    )?;
    assert!(rendered.contains(iteration_context));
    Ok(())
}

#[test]
fn render_worker_phase2_prompt_skips_repoprompt_when_not_required() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase2_prompt(
        template,
        "BASE",
        "PLAN",
        "CHECKLIST",
        "ITERATION",
        "COMPLETE",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains("PHASE=2"));
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PLAN"));
    assert!(rendered.contains("CHECKLIST"));
    assert!(rendered.contains("BASE"));
    assert!(!rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_phase2_prompt_allows_placeholder_like_base_prompt() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let base_prompt = "BASE {{ITERATION_COMPLETION_BLOCK}}";
    let rendered = render_worker_phase2_prompt(
        template,
        base_prompt,
        "PLAN",
        "CHECKLIST",
        "",
        "COMPLETE",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains(base_prompt));
    Ok(())
}

#[test]
fn render_worker_phase2_handoff_prompt_allows_placeholder_like_base_prompt() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let base_prompt = "BASE {{ITERATION_COMPLETION_BLOCK}}";
    let rendered = render_worker_phase2_handoff_prompt(
        template,
        base_prompt,
        "PLAN",
        "CHECKLIST",
        "",
        "COMPLETE",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains(base_prompt));
    Ok(())
}

#[test]
fn render_worker_phase2_handoff_prompt_allows_placeholder_like_iteration_context() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let iteration_context = "ITERATION {{PLACEHOLDER}}";
    let rendered = render_worker_phase2_handoff_prompt(
        template,
        "BASE",
        "PLAN",
        "CHECKLIST",
        iteration_context,
        "COMPLETE",
        "RQ-0001",
        3,
        false,
        &config,
    )?;
    assert!(rendered.contains(iteration_context));
    Ok(())
}

#[test]
fn render_worker_phase2_prompt_allows_placeholder_like_plan_text() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let plan_text = "Use {{config.agent.git_commit_push_enabled}} to toggle behavior.";
    let rendered = render_worker_phase2_prompt(
        template,
        "BASE",
        plan_text,
        "CHECKLIST",
        "ITERATION",
        "COMPLETE",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains(plan_text));
    Ok(())
}

#[test]
fn render_worker_phase2_prompt_allows_placeholder_like_iteration_context() -> Result<()> {
    let template =
        "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{PLAN_TEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let iteration_context = "ITERATION {{PLACEHOLDER}}";
    let rendered = render_worker_phase2_prompt(
        template,
        "BASE",
        "PLAN",
        "CHECKLIST",
        iteration_context,
        "COMPLETE",
        "RQ-0001",
        2,
        false,
        &config,
    )?;
    assert!(rendered.contains(iteration_context));
    Ok(())
}

#[test]
fn render_worker_phase3_prompt_includes_review_and_base() -> Result<()> {
    let template = "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PHASE3_COMPLETION_GUIDANCE}}\n{{ITERATION_CONTEXT}}\n{{CODE_REVIEW_BODY}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{COMPLETION_CHECKLIST}}\n{{PHASE2_FINAL_RESPONSE}}\n{{BASE_WORKER_PROMPT}}\n{{REPOPROMPT_BLOCK}}\n";
    let config = default_config();
    let rendered = render_worker_phase3_prompt(
        template,
        "BASE\n\n## PROJECT TYPE: CODE\n\nBase Guidance\n",
        "REVIEW\n## PROJECT TYPE: CODE\n\nExtra\n\n## NEXT",
        "PHASE2 RESPONSE",
        "RQ-0001",
        "CHECKLIST",
        "ITERATION",
        "COMPLETE",
        "GUIDANCE",
        3,
        true,
        &config,
    )?;
    assert!(rendered.contains("PHASE=3"));
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("REVIEW"));
    assert!(rendered.contains("## NEXT"));
    assert_eq!(rendered.matches("## PROJECT TYPE: CODE").count(), 1);
    assert!(rendered.contains("CHECKLIST"));
    assert!(rendered.contains("PHASE2 RESPONSE"));
    assert!(rendered.contains("BASE"));
    assert!(rendered.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(!rendered.contains("{{"));
    Ok(())
}

#[test]
fn render_worker_phase3_prompt_allows_placeholder_like_phase2_response() -> Result<()> {
    let template = "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PHASE3_COMPLETION_GUIDANCE}}\n{{ITERATION_CONTEXT}}\n{{CODE_REVIEW_BODY}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{COMPLETION_CHECKLIST}}\n{{PHASE2_FINAL_RESPONSE}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let phase2_text = "See {{config.agent.runner}} for the runner.";
    let rendered = render_worker_phase3_prompt(
        template,
        "BASE",
        "REVIEW",
        phase2_text,
        "RQ-0001",
        "CHECKLIST",
        "ITERATION",
        "COMPLETE",
        "GUIDANCE",
        3,
        false,
        &config,
    )?;
    assert!(rendered.contains(phase2_text));
    Ok(())
}

#[test]
fn render_worker_phase3_prompt_allows_placeholder_like_base_prompt() -> Result<()> {
    let template = "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PHASE3_COMPLETION_GUIDANCE}}\n{{ITERATION_CONTEXT}}\n{{CODE_REVIEW_BODY}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{COMPLETION_CHECKLIST}}\n{{PHASE2_FINAL_RESPONSE}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let base_prompt = "BASE {{ITERATION_COMPLETION_BLOCK}}";
    let rendered = render_worker_phase3_prompt(
        template,
        base_prompt,
        "REVIEW",
        "PHASE2 RESPONSE",
        "RQ-0001",
        "CHECKLIST",
        "",
        "COMPLETE",
        "GUIDANCE",
        3,
        false,
        &config,
    )?;
    assert!(rendered.contains(base_prompt));
    Ok(())
}

#[test]
fn render_worker_phase3_prompt_allows_placeholder_like_iteration_context() -> Result<()> {
    let template = "PHASE={{TOTAL_PHASES}}\nID={{TASK_ID}}\n{{PHASE3_COMPLETION_GUIDANCE}}\n{{ITERATION_CONTEXT}}\n{{CODE_REVIEW_BODY}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{COMPLETION_CHECKLIST}}\n{{PHASE2_FINAL_RESPONSE}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let iteration_context = "ITERATION {{PLACEHOLDER}}";
    let rendered = render_worker_phase3_prompt(
        template,
        "BASE",
        "REVIEW",
        "PHASE2",
        "RQ-0001",
        "CHECKLIST",
        iteration_context,
        "COMPLETE",
        "GUIDE",
        3,
        false,
        &config,
    )?;
    assert!(rendered.contains(iteration_context));
    Ok(())
}

#[test]
fn render_worker_single_phase_prompt_requires_task_id() -> Result<()> {
    let template =
        "{{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let result = render_worker_single_phase_prompt(
        template,
        "BASE",
        "CHECKLIST",
        "ITERATION",
        "COMPLETE",
        "",
        false,
        &config,
    );
    assert!(result.is_err());
    Ok(())
}

#[test]
fn render_worker_single_phase_prompt_allows_placeholder_like_base_prompt() -> Result<()> {
    let template =
        "{{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let base_prompt = "BASE {{ITERATION_COMPLETION_BLOCK}}";
    let rendered = render_worker_single_phase_prompt(
        template,
        base_prompt,
        "CHECKLIST",
        "",
        "COMPLETE",
        "RQ-0001",
        false,
        &config,
    )?;
    assert!(rendered.contains(base_prompt));
    Ok(())
}

#[test]
fn render_worker_single_phase_prompt_allows_placeholder_like_iteration_context() -> Result<()> {
    let template =
        "{{TASK_ID}}\n{{ITERATION_CONTEXT}}\n{{ITERATION_COMPLETION_BLOCK}}\n{{CHECKLIST}}\n{{BASE_WORKER_PROMPT}}\n";
    let config = default_config();
    let iteration_context = "ITERATION {{PLACEHOLDER}}";
    let rendered = render_worker_single_phase_prompt(
        template,
        "BASE",
        "CHECKLIST",
        iteration_context,
        "COMPLETE",
        "RQ-0001",
        false,
        &config,
    )?;
    assert!(rendered.contains(iteration_context));
    Ok(())
}
