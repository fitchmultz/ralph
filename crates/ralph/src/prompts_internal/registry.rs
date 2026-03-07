//! Prompt template registry and metadata.
//!
//! Responsibilities: centralize prompt template metadata (paths, embedded defaults, required
//! placeholders, flags) and provide a shared loader using the standard fallback behavior.
//! Not handled: prompt rendering/variable expansion or prompt-specific placeholder replacement
//! beyond required placeholder checks.
//! Invariants/assumptions: templates live under `.ralph/prompts/`, embedded defaults are compile-time
//! `include_str!` values, and required placeholder tokens include braces (e.g., `{{TASK_ID}}`).

use super::util::{RequiredPlaceholder, load_prompt_with_fallback};
use anyhow::Result;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum PromptTemplateId {
    Worker,
    WorkerPhase1,
    WorkerPhase2,
    WorkerPhase2Handoff,
    WorkerPhase3,
    WorkerSinglePhase,
    TaskBuilder,
    TaskDecompose,
    TaskUpdater,
    ScanMaintenanceV1,
    ScanMaintenanceV2,
    ScanInnovationV1,
    ScanInnovationV2,
    ScanGeneralV2,
    MergeConflicts,
    CodeReview,
    CompletionChecklist,
    Phase2HandoffChecklist,
    IterationChecklist,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PromptTemplate {
    pub(crate) rel_path: &'static str,
    pub(crate) embedded_default: &'static str,
    pub(crate) label: &'static str,
    pub(crate) required_placeholders: &'static [RequiredPlaceholder],
    pub(crate) project_type_guidance: bool,
}

const EMPTY_REQUIRED: &[RequiredPlaceholder] = &[];

const SCAN_V1_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{PROJECT_TYPE_GUIDANCE}}",
        error_message: "Template error: scan prompt v1 template is missing the required '{{PROJECT_TYPE_GUIDANCE}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{USER_FOCUS}}",
        error_message: "Template error: scan prompt v1 template is missing the required '{{USER_FOCUS}}' placeholder.",
    },
];

const SCAN_V2_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{PROJECT_TYPE_GUIDANCE}}",
        error_message: "Template error: scan prompt v2 template is missing the required '{{PROJECT_TYPE_GUIDANCE}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{USER_FOCUS}}",
        error_message: "Template error: scan prompt v2 template is missing the required '{{USER_FOCUS}}' placeholder.",
    },
];

const TASK_BUILDER_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{USER_REQUEST}}",
        error_message: "Template error: task builder prompt template is missing the required '{{USER_REQUEST}}' placeholder. Ensure the template in .ralph/prompts/task_builder.md includes this placeholder.",
    },
    RequiredPlaceholder {
        token: "{{HINT_TAGS}}",
        error_message: "Template error: task builder prompt template is missing the required '{{HINT_TAGS}}' placeholder. Ensure the template includes this placeholder.",
    },
    RequiredPlaceholder {
        token: "{{HINT_SCOPE}}",
        error_message: "Template error: task builder prompt template is missing the required '{{HINT_SCOPE}}' placeholder. Ensure the template includes this placeholder.",
    },
];

const TASK_UPDATER_REQUIRED: &[RequiredPlaceholder] = &[RequiredPlaceholder {
    token: "{{TASK_ID}}",
    error_message: "Template error: task updater prompt template is missing required '{{TASK_ID}}' placeholder. Ensure template in .ralph/prompts/task_updater.md includes this placeholder.",
}];

const TASK_DECOMPOSE_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{SOURCE_MODE}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{SOURCE_MODE}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{SOURCE_REQUEST}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{SOURCE_REQUEST}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{SOURCE_TASK_JSON}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{SOURCE_TASK_JSON}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{ATTACH_TARGET_JSON}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{ATTACH_TARGET_JSON}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{MAX_DEPTH}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{MAX_DEPTH}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{MAX_CHILDREN}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{MAX_CHILDREN}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{MAX_NODES}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{MAX_NODES}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{CHILD_POLICY}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{CHILD_POLICY}}' placeholder.",
    },
    RequiredPlaceholder {
        token: "{{WITH_DEPENDENCIES}}",
        error_message: "Template error: task decompose prompt template is missing the required '{{WITH_DEPENDENCIES}}' placeholder.",
    },
];

const CODE_REVIEW_REQUIRED: &[RequiredPlaceholder] = &[RequiredPlaceholder {
    token: "{{TASK_ID}}",
    error_message: "Template error: code review prompt template is missing the required '{{TASK_ID}}' placeholder.",
}];

const MERGE_CONFLICT_REQUIRED: &[RequiredPlaceholder] = &[RequiredPlaceholder {
    token: "{{CONFLICT_FILES}}",
    error_message: "Template error: merge conflict prompt template is missing the required '{{CONFLICT_FILES}}' placeholder.",
}];

pub(crate) fn prompt_template(id: PromptTemplateId) -> PromptTemplate {
    match id {
        PromptTemplateId::Worker => PromptTemplate {
            rel_path: ".ralph/prompts/worker.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker.md"
            )),
            label: "worker",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::WorkerPhase1 => PromptTemplate {
            rel_path: ".ralph/prompts/worker_phase1.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker_phase1.md"
            )),
            label: "worker phase1",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::WorkerPhase2 => PromptTemplate {
            rel_path: ".ralph/prompts/worker_phase2.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker_phase2.md"
            )),
            label: "worker phase2",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::WorkerPhase2Handoff => PromptTemplate {
            rel_path: ".ralph/prompts/worker_phase2_handoff.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker_phase2_handoff.md"
            )),
            label: "worker phase2 handoff",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::WorkerPhase3 => PromptTemplate {
            rel_path: ".ralph/prompts/worker_phase3.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker_phase3.md"
            )),
            label: "worker phase3",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::WorkerSinglePhase => PromptTemplate {
            rel_path: ".ralph/prompts/worker_single_phase.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/worker_single_phase.md"
            )),
            label: "worker single phase",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::TaskBuilder => PromptTemplate {
            rel_path: ".ralph/prompts/task_builder.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/task_builder.md"
            )),
            label: "task builder",
            required_placeholders: TASK_BUILDER_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::TaskDecompose => PromptTemplate {
            rel_path: ".ralph/prompts/task_decompose.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/task_decompose.md"
            )),
            label: "task decompose",
            required_placeholders: TASK_DECOMPOSE_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::TaskUpdater => PromptTemplate {
            rel_path: ".ralph/prompts/task_updater.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/task_updater.md"
            )),
            label: "task updater",
            required_placeholders: TASK_UPDATER_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::ScanMaintenanceV1 => PromptTemplate {
            rel_path: ".ralph/prompts/scan_maintenance_v1.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan_maintenance_v1.md"
            )),
            label: "scan maintenance v1",
            required_placeholders: SCAN_V1_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::ScanMaintenanceV2 => PromptTemplate {
            rel_path: ".ralph/prompts/scan_maintenance_v2.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan_maintenance_v2.md"
            )),
            label: "scan maintenance v2",
            required_placeholders: SCAN_V2_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::ScanInnovationV1 => PromptTemplate {
            rel_path: ".ralph/prompts/scan_innovation_v1.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan_innovation_v1.md"
            )),
            label: "scan innovation v1",
            required_placeholders: SCAN_V1_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::ScanInnovationV2 => PromptTemplate {
            rel_path: ".ralph/prompts/scan_innovation_v2.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan_innovation_v2.md"
            )),
            label: "scan innovation v2",
            required_placeholders: SCAN_V2_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::ScanGeneralV2 => PromptTemplate {
            rel_path: ".ralph/prompts/scan_general_v2.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan_general_v2.md"
            )),
            label: "scan general v2",
            required_placeholders: SCAN_V2_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::MergeConflicts => PromptTemplate {
            rel_path: ".ralph/prompts/merge_conflicts.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/merge_conflicts.md"
            )),
            label: "merge conflicts",
            required_placeholders: MERGE_CONFLICT_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::CodeReview => PromptTemplate {
            rel_path: ".ralph/prompts/code_review.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/code_review.md"
            )),
            label: "code review",
            required_placeholders: CODE_REVIEW_REQUIRED,
            project_type_guidance: true,
        },
        PromptTemplateId::CompletionChecklist => PromptTemplate {
            rel_path: ".ralph/prompts/completion_checklist.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/completion_checklist.md"
            )),
            label: "completion checklist",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::Phase2HandoffChecklist => PromptTemplate {
            rel_path: ".ralph/prompts/phase2_handoff_checklist.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/phase2_handoff_checklist.md"
            )),
            label: "phase2 handoff checklist",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
        PromptTemplateId::IterationChecklist => PromptTemplate {
            rel_path: ".ralph/prompts/iteration_checklist.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/iteration_checklist.md"
            )),
            label: "iteration checklist",
            required_placeholders: EMPTY_REQUIRED,
            project_type_guidance: false,
        },
    }
}

pub(crate) fn load_prompt_template(repo_root: &Path, id: PromptTemplateId) -> Result<String> {
    let template = prompt_template(id);
    load_prompt_with_fallback(
        repo_root,
        template.rel_path,
        template.embedded_default,
        template.label,
    )
}
