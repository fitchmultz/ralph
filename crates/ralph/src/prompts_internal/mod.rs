//! Modular prompt templates and rendering.
//!
//! Responsibilities: provide a cohesive internal home for prompt template loading/rendering and
//! re-export category-specific helpers for backward compatibility via `crate::prompts`.
//! Not handled: CLI argument parsing, task queue IO, or prompt overrides outside `.ralph/prompts`.
//! Invariants/assumptions: templates are validated for placeholders and `.ralph/prompts` overrides
//! may be absent.

pub mod iteration;
mod registry;
pub mod review;
pub mod scan;
pub mod task_builder;
pub mod task_updater;
pub mod util;
pub mod worker;
pub mod worker_phases;

#[cfg(test)]
mod tests;

pub use iteration::*;
pub use review::*;
pub use scan::*;
pub use task_builder::*;
pub use task_updater::*;
pub use util::*;
pub use worker::*;
pub use worker_phases::*;

use std::path::Path;

use anyhow::Result;

pub fn prompts_reference_readme(repo_root: &Path) -> Result<bool> {
    let worker = load_worker_prompt(repo_root)?;
    let worker_phase1 = load_worker_phase1_prompt(repo_root)?;
    let worker_phase2 = load_worker_phase2_prompt(repo_root)?;
    let worker_phase2_handoff = load_worker_phase2_handoff_prompt(repo_root)?;
    let worker_phase3 = load_worker_phase3_prompt(repo_root)?;
    let worker_single_phase = load_worker_single_phase_prompt(repo_root)?;
    let task_builder = load_task_builder_prompt(repo_root)?;
    let task_updater = load_task_updater_prompt(repo_root)?;
    let scan = load_scan_prompt(repo_root)?;
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
        || task_updater.contains(".ralph/README.md")
        || scan.contains(".ralph/README.md")
        || completion_checklist.contains(".ralph/README.md")
        || code_review.contains(".ralph/README.md")
        || phase2_handoff.contains(".ralph/README.md")
        || iteration_checklist.contains(".ralph/README.md"))
}
