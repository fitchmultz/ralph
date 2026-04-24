//! Prompt template inventory helpers.
//!
//! Purpose:
//! - Prompt template inventory helpers.
//!
//! Responsibilities:
//! - Enumerate prompt template IDs, names, and descriptions.
//! - Resolve embedded defaults and local override presence.
//!
//! Not handled here:
//! - Export/sync state tracking.
//! - Diff generation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Template file names remain stable across exports and sync.

use crate::prompts_internal::registry::{PromptTemplateId, prompt_template};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncStatus {
    UpToDate,
    Outdated,
    UserModified,
    Unknown,
    Missing,
}

#[derive(Debug, Clone)]
pub(crate) struct TemplateInfo {
    pub name: String,
    pub description: String,
    pub has_override: bool,
}

pub(crate) fn all_template_ids() -> Vec<PromptTemplateId> {
    vec![
        PromptTemplateId::Worker,
        PromptTemplateId::WorkerPhase1,
        PromptTemplateId::WorkerPhase2,
        PromptTemplateId::WorkerPhase2Handoff,
        PromptTemplateId::WorkerPhase3,
        PromptTemplateId::WorkerSinglePhase,
        PromptTemplateId::TaskBuilder,
        PromptTemplateId::TaskDecompose,
        PromptTemplateId::TaskUpdater,
        PromptTemplateId::ScanMaintenanceV1,
        PromptTemplateId::ScanMaintenanceV2,
        PromptTemplateId::ScanInnovationV1,
        PromptTemplateId::ScanInnovationV2,
        PromptTemplateId::ScanGeneralV2,
        PromptTemplateId::MergeConflicts,
        PromptTemplateId::CodeReview,
        PromptTemplateId::CompletionChecklist,
        PromptTemplateId::Phase2HandoffChecklist,
        PromptTemplateId::IterationChecklist,
    ]
}

pub(crate) fn template_description(id: PromptTemplateId) -> &'static str {
    match id {
        PromptTemplateId::Worker => "Base worker prompt with mission, context, and operating rules",
        PromptTemplateId::WorkerPhase1 => "Phase 1 planning wrapper (creates implementation plan)",
        PromptTemplateId::WorkerPhase2 => "Phase 2 implementation wrapper (2-phase workflow)",
        PromptTemplateId::WorkerPhase2Handoff => {
            "Phase 2 handoff wrapper (3-phase workflow, includes handoff checklist)"
        }
        PromptTemplateId::WorkerPhase3 => "Phase 3 code review wrapper (reviews implementation)",
        PromptTemplateId::WorkerSinglePhase => {
            "Single-phase wrapper (plan + implement in one pass)"
        }
        PromptTemplateId::TaskBuilder => "Task creation prompt (generates tasks from requests)",
        PromptTemplateId::TaskDecompose => {
            "Task decomposition prompt (returns a recursive JSON task tree)"
        }
        PromptTemplateId::TaskUpdater => {
            "Task update prompt (refreshes task fields from repo state)"
        }
        PromptTemplateId::ScanMaintenanceV1 => {
            "Repository scan prompt for maintenance mode - v1 (rule-based)"
        }
        PromptTemplateId::ScanMaintenanceV2 => {
            "Repository scan prompt for maintenance mode - v2 (rubric-based, default)"
        }
        PromptTemplateId::ScanInnovationV1 => {
            "Repository scan prompt for innovation mode - v1 (rule-based)"
        }
        PromptTemplateId::ScanInnovationV2 => {
            "Repository scan prompt for innovation mode - v2 (rubric-based, default)"
        }
        PromptTemplateId::ScanGeneralV2 => {
            "Repository scan prompt for general mode - v2 (focus-based, mode-agnostic)"
        }
        PromptTemplateId::MergeConflicts => {
            "Merge conflict resolution prompt (used by parallel merge runner)"
        }
        PromptTemplateId::CodeReview => "Code review body content (used in Phase 3)",
        PromptTemplateId::CompletionChecklist => {
            "Implementation completion checklist (validates done criteria)"
        }
        PromptTemplateId::Phase2HandoffChecklist => {
            "Phase 2 handoff checklist (for 3-phase workflow handoff)"
        }
        PromptTemplateId::IterationChecklist => {
            "Refinement mode checklist (for follow-up iterations)"
        }
    }
}

pub(crate) fn parse_template_name(name: &str) -> Option<PromptTemplateId> {
    let normalized = name.replace('-', "_").to_lowercase();
    match normalized.as_str() {
        "worker" => Some(PromptTemplateId::Worker),
        "worker_phase1" | "worker_phase_1" => Some(PromptTemplateId::WorkerPhase1),
        "worker_phase2" | "worker_phase_2" => Some(PromptTemplateId::WorkerPhase2),
        "worker_phase2_handoff" | "worker_phase_2_handoff" => {
            Some(PromptTemplateId::WorkerPhase2Handoff)
        }
        "worker_phase3" | "worker_phase_3" => Some(PromptTemplateId::WorkerPhase3),
        "worker_single_phase" => Some(PromptTemplateId::WorkerSinglePhase),
        "task_builder" => Some(PromptTemplateId::TaskBuilder),
        "task_decompose" => Some(PromptTemplateId::TaskDecompose),
        "task_updater" => Some(PromptTemplateId::TaskUpdater),
        "scan_maintenance_v1" => Some(PromptTemplateId::ScanMaintenanceV1),
        "scan_maintenance_v2" => Some(PromptTemplateId::ScanMaintenanceV2),
        "scan_innovation_v1" => Some(PromptTemplateId::ScanInnovationV1),
        "scan_innovation_v2" => Some(PromptTemplateId::ScanInnovationV2),
        "scan_general_v2" => Some(PromptTemplateId::ScanGeneralV2),
        "merge_conflicts" | "merge_conflict" => Some(PromptTemplateId::MergeConflicts),
        "code_review" => Some(PromptTemplateId::CodeReview),
        "completion_checklist" => Some(PromptTemplateId::CompletionChecklist),
        "phase2_handoff_checklist" | "phase_2_handoff_checklist" => {
            Some(PromptTemplateId::Phase2HandoffChecklist)
        }
        "iteration_checklist" => Some(PromptTemplateId::IterationChecklist),
        _ => None,
    }
}

pub(crate) fn template_file_name(id: PromptTemplateId) -> &'static str {
    match id {
        PromptTemplateId::Worker => "worker",
        PromptTemplateId::WorkerPhase1 => "worker_phase1",
        PromptTemplateId::WorkerPhase2 => "worker_phase2",
        PromptTemplateId::WorkerPhase2Handoff => "worker_phase2_handoff",
        PromptTemplateId::WorkerPhase3 => "worker_phase3",
        PromptTemplateId::WorkerSinglePhase => "worker_single_phase",
        PromptTemplateId::TaskBuilder => "task_builder",
        PromptTemplateId::TaskDecompose => "task_decompose",
        PromptTemplateId::TaskUpdater => "task_updater",
        PromptTemplateId::ScanMaintenanceV1 => "scan_maintenance_v1",
        PromptTemplateId::ScanMaintenanceV2 => "scan_maintenance_v2",
        PromptTemplateId::ScanInnovationV1 => "scan_innovation_v1",
        PromptTemplateId::ScanInnovationV2 => "scan_innovation_v2",
        PromptTemplateId::ScanGeneralV2 => "scan_general_v2",
        PromptTemplateId::MergeConflicts => "merge_conflicts",
        PromptTemplateId::CodeReview => "code_review",
        PromptTemplateId::CompletionChecklist => "completion_checklist",
        PromptTemplateId::Phase2HandoffChecklist => "phase2_handoff_checklist",
        PromptTemplateId::IterationChecklist => "iteration_checklist",
    }
}

pub(crate) fn list_templates(repo_root: &Path) -> Vec<TemplateInfo> {
    let prompts_dir = repo_root.join(".ralph/prompts");
    all_template_ids()
        .into_iter()
        .map(|id| {
            let file_name = template_file_name(id);
            TemplateInfo {
                name: file_name.to_string(),
                description: template_description(id).to_string(),
                has_override: prompts_dir.join(format!("{}.md", file_name)).exists(),
            }
        })
        .collect()
}

pub(crate) fn get_embedded_content(id: PromptTemplateId) -> &'static str {
    prompt_template(id).embedded_default
}

pub(crate) fn get_effective_content(repo_root: &Path, id: PromptTemplateId) -> Result<String> {
    let override_path = repo_root
        .join(".ralph/prompts")
        .join(format!("{}.md", template_file_name(id)));

    if override_path.exists() {
        fs::read_to_string(&override_path)
            .with_context(|| format!("read override file {}", override_path.display()))
    } else {
        Ok(get_embedded_content(id).to_string())
    }
}
