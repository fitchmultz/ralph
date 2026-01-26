//! Prompt template registry and metadata.
//!
//! Responsibilities: centralize prompt template metadata (paths, embedded defaults, required
//! placeholders, flags) and provide a shared loader using the standard fallback behavior.
//! Not handled: prompt rendering/variable expansion or prompt-specific placeholder replacement
//! beyond required placeholder checks.
//! Invariants/assumptions: templates live under `.ralph/prompts/`, embedded defaults are compile-time
//! `include_str!` values, and required placeholder tokens include braces (e.g., `{{TASK_ID}}`).

use super::util::{load_prompt_with_fallback, RequiredPlaceholder};
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
    TaskUpdater,
    Scan,
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

const SCAN_REQUIRED: &[RequiredPlaceholder] = &[RequiredPlaceholder {
    token: "{{USER_FOCUS}}",
    error_message: "Template error: scan prompt template is missing the required '{{USER_FOCUS}}' placeholder. Ensure the template in .ralph/prompts/scan.md includes this placeholder.",
}];

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

const TASK_UPDATER_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{TASK_ID}}",
        error_message: "Template error: task updater prompt template is missing required '{{TASK_ID}}' placeholder. Ensure template in .ralph/prompts/task_updater.md includes this placeholder.",
    },
    RequiredPlaceholder {
        token: "{{FIELDS_TO_UPDATE}}",
        error_message: "Template error: task updater prompt template is missing required '{{FIELDS_TO_UPDATE}}' placeholder. Ensure template includes this placeholder.",
    },
];

const CODE_REVIEW_REQUIRED: &[RequiredPlaceholder] = &[RequiredPlaceholder {
    token: "{{TASK_ID}}",
    error_message:
        "Template error: code review prompt template is missing the required '{{TASK_ID}}' placeholder.",
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
        PromptTemplateId::Scan => PromptTemplate {
            rel_path: ".ralph/prompts/scan.md",
            embedded_default: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/prompts/scan.md"
            )),
            label: "scan",
            required_placeholders: SCAN_REQUIRED,
            project_type_guidance: true,
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
