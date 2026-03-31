//! Purpose: Internal prompt-template facade and re-export hub.
//!
//! Responsibilities:
//! - Declare focused prompt companion modules.
//! - Re-export stable prompt-loading and rendering entrypoints for crate-local callers.
//! - Keep prompt registry and composition internals behind the facade.
//!
//! Scope:
//! - Prompt template loading, rendering, and validation plumbing only.
//! - Does not handle CLI parsing or queue/task IO.
//!
//! Usage:
//! - Used through `crate::prompts_internal::*` from prompt wrappers and config code.
//! - Keeps prompt implementation details localized under sibling modules.
//!
//! Invariants/Assumptions:
//! - Re-exported entrypoints remain the canonical internal surface for moved helpers.
//! - `.ralph/prompts` overrides may be absent.

mod instructions;
pub(crate) mod iteration;
pub(crate) mod management;
pub(crate) mod merge_conflicts;
mod registry;
pub(crate) mod review;
pub(crate) mod scan;
pub(crate) mod task_builder;
pub(crate) mod task_decompose;
pub(crate) mod task_updater;
pub(crate) mod util;
pub(crate) mod worker;
pub(crate) mod worker_phases;

pub(crate) use instructions::{
    instruction_file_warnings, validate_instruction_file_paths, wrap_with_instruction_files,
};

#[cfg(test)]
mod tests;
use merge_conflicts::load_merge_conflict_prompt;
use review::{
    load_code_review_prompt, load_completion_checklist, load_iteration_checklist,
    load_phase2_handoff_checklist,
};
use scan::load_scan_prompt;
use task_builder::load_task_builder_prompt;
use task_decompose::load_task_decompose_prompt;
use task_updater::load_task_updater_prompt;
use worker::load_worker_prompt;
use worker_phases::{
    load_worker_phase1_prompt, load_worker_phase2_handoff_prompt, load_worker_phase2_prompt,
    load_worker_phase3_prompt, load_worker_single_phase_prompt,
};

use crate::cli::scan::ScanMode;
use crate::contracts::ScanPromptVersion;
use std::path::Path;

use anyhow::Result;

pub(crate) fn prompts_reference_readme(repo_root: &Path) -> Result<bool> {
    let worker = load_worker_prompt(repo_root)?;
    let worker_phase1 = load_worker_phase1_prompt(repo_root)?;
    let worker_phase2 = load_worker_phase2_prompt(repo_root)?;
    let worker_phase2_handoff = load_worker_phase2_handoff_prompt(repo_root)?;
    let worker_phase3 = load_worker_phase3_prompt(repo_root)?;
    let worker_single_phase = load_worker_single_phase_prompt(repo_root)?;
    let task_builder = load_task_builder_prompt(repo_root)?;
    let task_decompose = load_task_decompose_prompt(repo_root)?;
    let task_updater = load_task_updater_prompt(repo_root)?;
    let merge_conflicts = load_merge_conflict_prompt(repo_root)?;
    let scan = load_scan_prompt(repo_root, ScanPromptVersion::V2, ScanMode::General)?;
    let completion_checklist = load_completion_checklist(repo_root)?;
    let code_review = load_code_review_prompt(repo_root)?;
    let phase2_handoff = load_phase2_handoff_checklist(repo_root)?;
    let iteration_checklist = load_iteration_checklist(repo_root)?;

    Ok(worker.contains(".ralph/README.md")
        || worker_phase1.contains(".ralph/README.md")
        || worker_phase2.contains(".ralph/README.md")
        || worker_phase2_handoff.contains(".ralph/README.md")
        || worker_phase3.contains(".ralph/README.md")
        || worker_single_phase.contains(".ralph/README.md")
        || task_builder.contains(".ralph/README.md")
        || task_decompose.contains(".ralph/README.md")
        || task_updater.contains(".ralph/README.md")
        || merge_conflicts.contains(".ralph/README.md")
        || scan.contains(".ralph/README.md")
        || completion_checklist.contains(".ralph/README.md")
        || code_review.contains(".ralph/README.md")
        || phase2_handoff.contains(".ralph/README.md")
        || iteration_checklist.contains(".ralph/README.md"))
}
