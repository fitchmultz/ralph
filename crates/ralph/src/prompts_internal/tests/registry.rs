//! Registry mapping and metadata validation tests.
//!
//! Responsibilities: validate prompt registry mappings, metadata, and required placeholders.
//! Not handled: prompt rendering, variable expansion, or file loading.
//! Invariants/assumptions: embedded defaults include known headers and registry metadata matches
//! prompt asset locations.

use super::*;

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
            embedded_marker: "{{MODE_GUIDANCE}}",
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
        assert!(
            template
                .embedded_default
                .contains(expectation.embedded_marker)
        );
    }
}

#[test]
fn worker_prompt_mentions_scope_is_starting_point() {
    let template = prompt_template(PromptTemplateId::Worker).embedded_default;
    assert!(template.contains("Scope is a starting point, not a restriction."));
}

#[test]
fn task_builder_prompt_mentions_scope_is_starting_point() {
    let template = prompt_template(PromptTemplateId::TaskBuilder).embedded_default;
    assert!(template.contains("Scope is a starting point, not a restriction."));
}

#[test]
fn scan_prompt_template_is_mode_guidance_only() {
    let template = prompt_template(PromptTemplateId::Scan).embedded_default;
    assert_eq!(template.trim(), "{{MODE_GUIDANCE}}");
}

#[test]
fn task_updater_prompt_mentions_scope_is_starting_point() {
    let template = prompt_template(PromptTemplateId::TaskUpdater).embedded_default;
    assert!(template.contains("Scope is a starting point, not a restriction."));
}

#[test]
fn required_placeholders_fail_when_missing() {
    let template = "no placeholders here";
    let meta = prompt_template(PromptTemplateId::Scan);
    let err = ensure_required_placeholders(template, meta.required_placeholders).unwrap_err();
    assert!(
        err.to_string()
            .contains("scan prompt template is missing the required")
    );
}

#[test]
fn required_placeholders_pass_when_present() -> Result<()> {
    let template = "{{MODE_GUIDANCE}}";
    let meta = prompt_template(PromptTemplateId::Scan);
    ensure_required_placeholders(template, meta.required_placeholders)?;
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
fn repoprompt_required_instruction_mentions_tool_inventory() {
    let instruction = REPOPROMPT_REQUIRED_INSTRUCTION;
    let required_fragments = [
        "TOOLING REQUIREMENT: RepoPrompt",
        "list_windows",
        "select_window",
        "_windowID",
        "manage_workspaces",
        "list_tabs",
        "select_tab",
        "_tabID",
        "manage_selection",
        "get_file_tree",
        "file_search",
        "read_file",
        "get_code_structure",
        "workspace_context",
        "prompt",
        "apply_edits",
        "file_actions",
        "git",
        "status/diff/log/show/blame",
        "context_builder",
        "list_models",
        "chat_send",
        "chats",
    ];
    for fragment in required_fragments {
        assert!(
            instruction.contains(fragment),
            "instruction missing fragment: {fragment}"
        );
    }
}
