//! Prompt loading, rendering, and validation tests.
//!
//! Responsibilities: validate prompt registry mappings, fallback behavior, and rendering/validation
//! paths.
//! Not handled: end-to-end CLI behavior, queue updates, or runner integration beyond templates.
//! Invariants/assumptions: embedded defaults include known headers and registry metadata matches
//! prompt asset locations.

use super::registry::{prompt_template, PromptTemplateId};
use super::{review::*, scan::*, task_builder::*, util::*, worker::*, worker_phases::*};
use crate::contracts::{Config, ProjectType};
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

fn default_config() -> Config {
    Config::default()
}

#[test]
fn registry_maps_prompt_metadata() {
    struct Expectation {
        id: PromptTemplateId,
        rel_path: &'static str,
        label: &'static str,
        embedded_marker: &'static str,
        project_guidance: bool,
    }

    let expectations = [
        Expectation {
            id: PromptTemplateId::Worker,
            rel_path: ".ralph/prompts/worker.md",
            label: "worker",
            embedded_marker: "# MISSION",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::WorkerPhase1,
            rel_path: ".ralph/prompts/worker_phase1.md",
            label: "worker phase1",
            embedded_marker: "# PLANNING MODE",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::WorkerPhase2,
            rel_path: ".ralph/prompts/worker_phase2.md",
            label: "worker phase2",
            embedded_marker: "# IMPLEMENTATION MODE",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::WorkerPhase2Handoff,
            rel_path: ".ralph/prompts/worker_phase2_handoff.md",
            label: "worker phase2 handoff",
            embedded_marker: "# IMPLEMENTATION MODE - PHASE 2",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::WorkerPhase3,
            rel_path: ".ralph/prompts/worker_phase3.md",
            label: "worker phase3",
            embedded_marker: "# CODE REVIEW MODE",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::WorkerSinglePhase,
            rel_path: ".ralph/prompts/worker_single_phase.md",
            label: "worker single phase",
            embedded_marker: "single-pass execution mode",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::TaskBuilder,
            rel_path: ".ralph/prompts/task_builder.md",
            label: "task builder",
            embedded_marker: "ralph queue next-id",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::TaskUpdater,
            rel_path: ".ralph/prompts/task_updater.md",
            label: "task updater",
            embedded_marker: "{{FIELDS_TO_UPDATE}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::Scan,
            rel_path: ".ralph/prompts/scan.md",
            label: "scan",
            embedded_marker: "ralph queue next-id",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::CodeReview,
            rel_path: ".ralph/prompts/code_review.md",
            label: "code review",
            embedded_marker: "Phase 3 reviewer",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::CompletionChecklist,
            rel_path: ".ralph/prompts/completion_checklist.md",
            label: "completion checklist",
            embedded_marker: "IMPLEMENTATION COMPLETION CHECKLIST",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::Phase2HandoffChecklist,
            rel_path: ".ralph/prompts/phase2_handoff_checklist.md",
            label: "phase2 handoff checklist",
            embedded_marker: "PHASE 2 HANDOFF CHECKLIST",
            project_guidance: false,
        },
        Expectation {
            id: PromptTemplateId::IterationChecklist,
            rel_path: ".ralph/prompts/iteration_checklist.md",
            label: "iteration checklist",
            embedded_marker: "ITERATION CHECKLIST",
            project_guidance: false,
        },
    ];

    for expectation in expectations {
        let template = prompt_template(expectation.id);
        assert_eq!(template.rel_path, expectation.rel_path);
        assert_eq!(template.label, expectation.label);
        assert_eq!(template.project_type_guidance, expectation.project_guidance);
        assert!(template
            .embedded_default
            .contains(expectation.embedded_marker));
    }
}

#[test]
fn required_placeholders_fail_when_missing() {
    let template = "no placeholders here";
    let meta = prompt_template(PromptTemplateId::Scan);
    let err = ensure_required_placeholders(template, meta.required_placeholders).unwrap_err();
    assert!(err
        .to_string()
        .contains("scan prompt template is missing the required"));
}

#[test]
fn required_placeholders_pass_when_present() -> Result<()> {
    let template = "FOCUS={{USER_FOCUS}}";
    let meta = prompt_template(PromptTemplateId::Scan);
    ensure_required_placeholders(template, meta.required_placeholders)?;
    Ok(())
}

#[test]
fn render_worker_prompt_replaces_interactive_instructions() -> Result<()> {
    let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n";
    let config = default_config();
    let rendered = render_worker_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(!rendered.contains("{{INTERACTIVE_INSTRUCTIONS}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
    let template = "FOCUS:\n{{USER_FOCUS}}\n";
    let config = default_config();
    let rendered = render_scan_prompt(template, "hello world", ProjectType::Code, &config)?;
    assert!(rendered.contains("hello world"));
    assert!(!rendered.contains("{{USER_FOCUS}}"));
    Ok(())
}

#[test]
fn render_scan_prompt_allows_placeholder_like_focus() -> Result<()> {
    let template = "FOCUS:\n{{USER_FOCUS}}\n";
    let config = default_config();
    let focus = "see {{config.agent.model}} here";
    let rendered = render_scan_prompt(template, focus, ProjectType::Code, &config)?;
    assert!(rendered.contains(focus));
    Ok(())
}

#[test]
fn render_task_builder_prompt_replaces_placeholders() -> Result<()> {
    let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let rendered = render_task_builder_prompt(
        template,
        "do thing",
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("do thing"));
    assert!(rendered.contains("code"));
    assert!(rendered.contains("repo"));
    assert!(!rendered.contains("{{USER_REQUEST}}"));
    Ok(())
}

#[test]
fn render_task_builder_prompt_allows_placeholder_like_request() -> Result<()> {
    let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let request = "use {{config.agent.model}}";
    let rendered = render_task_builder_prompt(
        template,
        request,
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains(request));
    Ok(())
}

#[test]
fn repoprompt_planning_instruction_mentions_preflight_and_parity() {
    let instruction = REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION;
    assert!(instruction.contains("RepoPrompt produces a plan, but you own its correctness"));
    assert!(instruction.contains("quick repo reality check"));
    assert!(instruction.contains("Parity rule"));
    assert!(instruction.contains("append (add) missing files"));
    assert!(instruction.contains("do NOT replace selection"));
    assert!(instruction.contains("provided chat ID"));
}

#[test]
fn load_worker_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(prompt.contains("# MISSION"));
    Ok(())
}

#[test]
fn load_worker_phase1_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase1_prompt(dir.path())?;
    assert!(prompt.contains("# PLANNING MODE"));
    Ok(())
}

#[test]
fn load_worker_phase2_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase2_prompt(dir.path())?;
    assert!(prompt.contains("# IMPLEMENTATION MODE"));
    Ok(())
}

#[test]
fn load_worker_phase3_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_phase3_prompt(dir.path())?;
    assert!(prompt.contains("# CODE REVIEW MODE"));
    Ok(())
}

#[test]
fn load_worker_single_phase_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_single_phase_prompt(dir.path())?;
    assert!(prompt.contains("single-pass execution mode"));
    Ok(())
}

#[test]
fn default_task_builder_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_task_builder_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}

#[test]
fn default_scan_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_scan_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}

#[test]
fn default_worker_prompt_excludes_completion_checklist() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_worker_prompt(dir.path())?;
    assert!(!prompt.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    assert!(!prompt.contains("END-OF-TURN CHECKLIST"));
    Ok(())
}

#[test]
fn load_completion_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_completion_checklist(dir.path())?;
    assert!(checklist.contains("IMPLEMENTATION COMPLETION CHECKLIST"));
    Ok(())
}

#[test]
fn load_iteration_checklist_falls_back_to_embedded_default_when_missing() -> Result<()> {
    let dir = TempDir::new()?;
    let checklist = load_iteration_checklist(dir.path())?;
    assert!(checklist.contains("ITERATION CHECKLIST"));
    Ok(())
}

#[test]
fn load_completion_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("completion_checklist.md"), "override")?;
    let checklist = load_completion_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}

#[test]
fn load_iteration_checklist_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("iteration_checklist.md"), "override")?;
    let checklist = load_iteration_checklist(dir.path())?;
    assert_eq!(checklist, "override");
    Ok(())
}

#[test]
fn load_worker_prompt_uses_override_when_present() -> Result<()> {
    let dir = TempDir::new()?;
    let overrides = dir.path().join(".ralph/prompts");
    fs::create_dir_all(&overrides)?;
    fs::write(overrides.join("worker.md"), "override")?;
    let prompt = load_worker_prompt(dir.path())?;
    assert_eq!(prompt, "override");
    Ok(())
}

#[test]
fn render_iteration_checklist_replaces_task_id() -> Result<()> {
    let template = "ID={{TASK_ID}}\n";
    let config = default_config();
    let rendered = render_iteration_checklist(template, "RQ-0001", &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(!rendered.contains("{{TASK_ID}}"));
    Ok(())
}

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

#[test]
fn ensure_no_unresolved_placeholders_passes_when_none_remain() -> Result<()> {
    let rendered = "Hello world";
    assert!(ensure_no_unresolved_placeholders(rendered, "test").is_ok());
    Ok(())
}

#[test]
fn ensure_no_unresolved_placeholders_fails_with_placeholder() -> Result<()> {
    let rendered = "Hello {{MISSING}} world";
    let err = ensure_no_unresolved_placeholders(rendered, "test").unwrap_err();
    assert!(err.to_string().contains("MISSING"));
    assert!(err.to_string().contains("unresolved placeholders"));
    Ok(())
}

#[test]
fn unresolved_placeholders_finds_all_placeholders() {
    let rendered = "Test {{ONE}} and {{TWO}} and {{three}}";
    let placeholders = unresolved_placeholders(rendered);
    assert_eq!(placeholders.len(), 3);
    assert!(placeholders.contains(&"ONE".to_string()));
    assert!(placeholders.contains(&"TWO".to_string()));
    assert!(placeholders.contains(&"THREE".to_string()));
}

#[test]
fn unresolved_placeholders_returns_sorted_unique() {
    let rendered = "Test {{Z}} and {{A}} and {{B}} and {{A}}";
    let placeholders = unresolved_placeholders(rendered);
    assert_eq!(placeholders, vec!["A", "B", "Z"]);
}

#[test]
fn expand_variables_expands_env_var_with_default() -> Result<()> {
    let var_name = "RALPH_TEST_DEFAULT_VAR";
    std::env::remove_var(var_name);
    let template = format!("Value: ${{{}:-default_value}}", var_name);
    let config = default_config();
    let result = expand_variables(&template, &config)?;
    assert_eq!(result, "Value: default_value");
    Ok(())
}

#[test]
fn expand_variables_expands_env_var_when_set() -> Result<()> {
    let var_name = "RALPH_TEST_SET_VAR";
    let template = format!("Value: ${{{}:-default}}", var_name);
    let config = default_config();
    std::env::set_var(var_name, "actual_value");
    let result = expand_variables(&template, &config)?;
    std::env::remove_var(var_name);
    assert_eq!(result, "Value: actual_value");
    Ok(())
}

#[test]
fn expand_variables_leaves_missing_env_var_literal() -> Result<()> {
    let template = "Value: ${MISSING_VAR}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("${MISSING_VAR}"));
    Ok(())
}

#[test]
fn expand_variables_handles_dollar_escape() -> Result<()> {
    let template = "Literal: $${ESCAPED}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert_eq!(result, "Literal: ${ESCAPED}");
    Ok(())
}

#[test]
fn expand_variables_handles_backslash_escape() -> Result<()> {
    let template = "Literal: \\${ESCAPED}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert_eq!(result, "Literal: ${ESCAPED}");
    Ok(())
}

#[test]
fn expand_variables_expands_config_runner() -> Result<()> {
    let template = "Runner: {{config.agent.runner}}";
    let mut config = default_config();
    config.agent.runner = Some(crate::contracts::Runner::Claude);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Runner: Claude"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_model() -> Result<()> {
    let template = "Model: {{config.agent.model}}";
    let mut config = default_config();
    config.agent.model = Some(crate::contracts::Model::Gpt52Codex);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("gpt-5.2-codex"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_queue_id_prefix() -> Result<()> {
    let template = "Prefix: {{config.queue.id_prefix}}";
    let mut config = default_config();
    config.queue.id_prefix = Some("TASK".to_string());
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Prefix: TASK"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_ci_gate_command() -> Result<()> {
    let template = "CI: {{config.agent.ci_gate_command}}";
    let mut config = default_config();
    config.agent.ci_gate_command = Some("make ci".to_string());
    let result = expand_variables(template, &config)?;
    assert!(result.contains("CI: make ci"));
    Ok(())
}

#[test]
fn expand_variables_expands_config_ci_gate_enabled() -> Result<()> {
    let template = "Enabled: {{config.agent.ci_gate_enabled}}";
    let mut config = default_config();
    config.agent.ci_gate_enabled = Some(false);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Enabled: false"));
    Ok(())
}

#[test]
fn expand_variables_expands_git_commit_push_enabled() -> Result<()> {
    let template = "Git commit/push: {{config.agent.git_commit_push_enabled}}";
    let mut config = default_config();
    config.agent.git_commit_push_enabled = Some(false);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("Git commit/push: false"));
    Ok(())
}

#[test]
fn expand_variables_leaves_non_config_placeholders() -> Result<()> {
    let template = "Request: {{USER_REQUEST}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("{{USER_REQUEST}}"));
    Ok(())
}

#[test]
fn expand_variables_mixed_env_and_config() -> Result<()> {
    let template = "Model: {{config.agent.model}}, Var: ${TEST:-default}";
    let mut config = default_config();
    config.agent.model = Some(crate::contracts::Model::Gpt52Codex);
    let result = expand_variables(template, &config)?;
    assert!(result.contains("gpt-5.2-codex"));
    assert!(result.contains("Var: default"));
    Ok(())
}

#[test]
fn render_code_review_prompt_replaces_placeholders() -> Result<()> {
    let template = "ID={{TASK_ID}}\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("PROJECT TYPE: CODE"));
    Ok(())
}

#[test]
fn render_code_review_prompt_allows_placeholder_like_text() -> Result<()> {
    let template = "ID={{TASK_ID}}\nSome text with {{TASK_ID}} in it\n";
    let config = default_config();
    let rendered = render_code_review_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(rendered.contains("Some text with RQ-0001 in it"));
    Ok(())
}

#[test]
fn render_code_review_prompt_fails_missing_task_id() -> Result<()> {
    let template = "{{TASK_ID}}\n";
    let config = default_config();
    let result = render_code_review_prompt(template, "", ProjectType::Code, &config);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("task id"));
    Ok(())
}

#[test]
fn expand_variables_invalid_config_path_left_literal() -> Result<()> {
    let template = "Value: {{config.invalid.path}}";
    let config = default_config();
    let result = expand_variables(template, &config)?;
    assert!(result.contains("{{config.invalid.path}}"));
    Ok(())
}
