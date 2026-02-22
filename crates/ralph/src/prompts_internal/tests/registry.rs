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
            embedded_marker: "{{TASK_ID}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::ScanMaintenanceV1,
            rel_path: ".ralph/prompts/scan_maintenance_v1.md",
            label: "scan maintenance v1",
            embedded_marker: "{{USER_FOCUS}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::ScanMaintenanceV2,
            rel_path: ".ralph/prompts/scan_maintenance_v2.md",
            label: "scan maintenance v2",
            embedded_marker: "{{USER_FOCUS}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::ScanInnovationV1,
            rel_path: ".ralph/prompts/scan_innovation_v1.md",
            label: "scan innovation v1",
            embedded_marker: "{{USER_FOCUS}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::ScanInnovationV2,
            rel_path: ".ralph/prompts/scan_innovation_v2.md",
            label: "scan innovation v2",
            embedded_marker: "{{USER_FOCUS}}",
            project_guidance: true,
        },
        Expectation {
            id: PromptTemplateId::MergeConflicts,
            rel_path: ".ralph/prompts/merge_conflicts.md",
            label: "merge conflicts",
            embedded_marker: "Merge Conflict Resolution",
            project_guidance: false,
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
                .contains(expectation.embedded_marker),
            "Prompt '{}' (id={:?}) is missing expected marker: {:?}\n\nActual content preview (first 500 chars):\n{}",
            expectation.label,
            expectation.id,
            expectation.embedded_marker,
            &template.embedded_default[..template.embedded_default.len().min(500)]
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
fn scan_maintenance_v1_prompt_contains_required_placeholders() {
    let template = prompt_template(PromptTemplateId::ScanMaintenanceV1).embedded_default;
    assert!(template.contains("{{USER_FOCUS}}"));
    assert!(template.contains("{{PROJECT_TYPE_GUIDANCE}}"));
}

#[test]
fn scan_maintenance_v2_prompt_contains_required_placeholders() {
    let template = prompt_template(PromptTemplateId::ScanMaintenanceV2).embedded_default;
    assert!(template.contains("{{USER_FOCUS}}"));
    assert!(template.contains("{{PROJECT_TYPE_GUIDANCE}}"));
}

#[test]
fn scan_innovation_v1_prompt_contains_required_placeholders() {
    let template = prompt_template(PromptTemplateId::ScanInnovationV1).embedded_default;
    assert!(template.contains("{{USER_FOCUS}}"));
    assert!(template.contains("{{PROJECT_TYPE_GUIDANCE}}"));
}

#[test]
fn scan_innovation_v2_prompt_contains_required_placeholders() {
    let template = prompt_template(PromptTemplateId::ScanInnovationV2).embedded_default;
    assert!(template.contains("{{USER_FOCUS}}"));
    assert!(template.contains("{{PROJECT_TYPE_GUIDANCE}}"));
}

#[test]
fn merge_conflict_prompt_contains_required_placeholders() {
    let template = prompt_template(PromptTemplateId::MergeConflicts).embedded_default;
    assert!(template.contains("{{CONFLICT_FILES}}"));
}

#[test]
fn task_updater_prompt_mentions_scope_is_starting_point() {
    let template = prompt_template(PromptTemplateId::TaskUpdater).embedded_default;
    assert!(template.contains("Scope is a starting point, not a restriction."));
}

fn contains_legacy_json_path(template: &str, legacy_path: &str) -> bool {
    let bytes = template.as_bytes();
    for (start, _) in template.match_indices(legacy_path) {
        let next_idx = start + legacy_path.len();
        if bytes.get(next_idx).copied() == Some(b'c') {
            continue;
        }
        return true;
    }
    false
}

#[test]
fn queue_related_prompts_use_config_paths_and_avoid_legacy_json_literals() {
    let queue_related_templates = [
        PromptTemplateId::Worker,
        PromptTemplateId::WorkerPhase1,
        PromptTemplateId::TaskBuilder,
        PromptTemplateId::TaskUpdater,
        PromptTemplateId::ScanMaintenanceV1,
        PromptTemplateId::ScanMaintenanceV2,
        PromptTemplateId::ScanInnovationV1,
        PromptTemplateId::ScanInnovationV2,
        PromptTemplateId::ScanGeneralV2,
        PromptTemplateId::MergeConflicts,
        PromptTemplateId::CompletionChecklist,
    ];

    for template_id in queue_related_templates {
        let template = prompt_template(template_id).embedded_default;
        assert!(
            !contains_legacy_json_path(template, ".ralph/queue.json"),
            "template {:?} still references legacy .ralph/queue.json",
            template_id
        );
        assert!(
            !contains_legacy_json_path(template, ".ralph/done.json"),
            "template {:?} still references legacy .ralph/done.json",
            template_id
        );
    }
}

#[test]
fn required_placeholders_fail_when_missing() {
    let template = "no placeholders here";
    let meta = prompt_template(PromptTemplateId::ScanMaintenanceV1);
    let err = ensure_required_placeholders(template, meta.required_placeholders).unwrap_err();
    assert!(
        err.to_string()
            .contains("scan prompt v1 template is missing the required")
    );
}

#[test]
fn required_placeholders_pass_when_present() -> Result<()> {
    let template = "{{USER_FOCUS}} {{PROJECT_TYPE_GUIDANCE}}";
    let meta = prompt_template(PromptTemplateId::ScanMaintenanceV1);
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
