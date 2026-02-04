//! Prompt template loading, rendering, and validation utilities.
//!
//! Responsibilities:
//! - Expose a minimal public prompt API for integration tests and external callers.
//! - Keep prompt composition and registry details internal to the crate.
//!
//! Not handled:
//! - CLI argument parsing or queue mutation.
//! - Direct access to prompt registry internals (see `prompts_internal`).
//!
//! Invariants/assumptions:
//! - Public exports here are intentional and minimal.

use crate::cli::scan::ScanMode;
use crate::contracts::{Config, ProjectType, ScanPromptVersion};
use crate::prompts_internal;
use anyhow::Result;
use std::path::Path;

pub const REPOPROMPT_REQUIRED_INSTRUCTION: &str =
    prompts_internal::util::REPOPROMPT_REQUIRED_INSTRUCTION;
pub const REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION: &str =
    prompts_internal::util::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION;
pub const PHASE3_COMPLETION_GUIDANCE_FINAL: &str =
    prompts_internal::iteration::PHASE3_COMPLETION_GUIDANCE_FINAL;

pub(crate) const ITERATION_CONTEXT_REFINEMENT: &str =
    prompts_internal::iteration::ITERATION_CONTEXT_REFINEMENT;
pub(crate) const ITERATION_COMPLETION_BLOCK: &str =
    prompts_internal::iteration::ITERATION_COMPLETION_BLOCK;
pub(crate) const PHASE3_COMPLETION_GUIDANCE_NONFINAL: &str =
    prompts_internal::iteration::PHASE3_COMPLETION_GUIDANCE_NONFINAL;

pub(crate) fn prompts_reference_readme(repo_root: &Path) -> Result<bool> {
    prompts_internal::prompts_reference_readme(repo_root)
}

pub(crate) fn load_scan_prompt(
    repo_root: &Path,
    version: ScanPromptVersion,
    mode: ScanMode,
) -> Result<String> {
    prompts_internal::scan::load_scan_prompt(repo_root, version, mode)
}

pub(crate) fn render_scan_prompt(
    template: &str,
    focus: &str,
    mode: ScanMode,
    version: ScanPromptVersion,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    prompts_internal::scan::render_scan_prompt(template, focus, mode, version, project_type, config)
}

pub(crate) fn load_task_builder_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::task_builder::load_task_builder_prompt(repo_root)
}

pub(crate) fn render_task_builder_prompt(
    template: &str,
    user_request: &str,
    hint_tags: &str,
    hint_scope: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    prompts_internal::task_builder::render_task_builder_prompt(
        template,
        user_request,
        hint_tags,
        hint_scope,
        project_type,
        config,
    )
}

pub(crate) fn load_task_updater_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::task_updater::load_task_updater_prompt(repo_root)
}

pub(crate) fn render_task_updater_prompt(
    template: &str,
    task_id: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    prompts_internal::task_updater::render_task_updater_prompt(
        template,
        task_id,
        project_type,
        config,
    )
}

pub(crate) fn load_merge_conflict_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::merge_conflicts::load_merge_conflict_prompt(repo_root)
}

pub(crate) fn render_merge_conflict_prompt(
    template: &str,
    conflict_files: &[String],
    config: &Config,
) -> Result<String> {
    prompts_internal::merge_conflicts::render_merge_conflict_prompt(
        template,
        conflict_files,
        config,
    )
}

pub(crate) fn wrap_with_repoprompt_requirement(prompt: &str, required: bool) -> String {
    prompts_internal::util::wrap_with_repoprompt_requirement(prompt, required)
}

pub(crate) fn wrap_with_instruction_files(
    repo_root: &Path,
    prompt: &str,
    config: &Config,
) -> Result<String> {
    prompts_internal::util::wrap_with_instruction_files(repo_root, prompt, config)
}

pub(crate) fn instruction_file_warnings(repo_root: &Path, config: &Config) -> Vec<String> {
    prompts_internal::util::instruction_file_warnings(repo_root, config)
}

pub(crate) fn load_worker_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker::load_worker_prompt(repo_root)
}

pub(crate) fn render_worker_prompt(
    template: &str,
    task_id: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker::render_worker_prompt(template, task_id, project_type, config)
}

pub fn load_worker_phase1_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker_phases::load_worker_phase1_prompt(repo_root)
}

pub fn load_worker_phase2_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker_phases::load_worker_phase2_prompt(repo_root)
}

pub fn load_worker_phase2_handoff_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker_phases::load_worker_phase2_handoff_prompt(repo_root)
}

pub fn load_worker_phase3_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker_phases::load_worker_phase3_prompt(repo_root)
}

pub fn load_worker_single_phase_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::worker_phases::load_worker_single_phase_prompt(repo_root)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase1_prompt(
    template: &str,
    base_prompt: &str,
    iteration_context: &str,
    task_id: &str,
    total_phases: u8,
    plan_path: &str,
    repoprompt_plan_required: bool,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker_phases::render_worker_phase1_prompt(
        template,
        base_prompt,
        iteration_context,
        task_id,
        total_phases,
        plan_path,
        repoprompt_plan_required,
        repoprompt_tool_injection,
        config,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase2_prompt(
    template: &str,
    base_prompt: &str,
    plan: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker_phases::render_worker_phase2_prompt(
        template,
        base_prompt,
        plan,
        checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        total_phases,
        repoprompt_tool_injection,
        config,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase2_handoff_prompt(
    template: &str,
    base_prompt: &str,
    plan: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker_phases::render_worker_phase2_handoff_prompt(
        template,
        base_prompt,
        plan,
        checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        total_phases,
        repoprompt_tool_injection,
        config,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase3_prompt(
    template: &str,
    base_prompt: &str,
    review_body: &str,
    phase2_final: &str,
    task_id: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker_phases::render_worker_phase3_prompt(
        template,
        base_prompt,
        review_body,
        phase2_final,
        task_id,
        checklist,
        iteration_context,
        iteration_completion_block,
        phase3_completion_guidance,
        total_phases,
        repoprompt_tool_injection,
        config,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_single_phase_prompt(
    template: &str,
    base_prompt: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    prompts_internal::worker_phases::render_worker_single_phase_prompt(
        template,
        base_prompt,
        checklist,
        iteration_context,
        iteration_completion_block,
        task_id,
        repoprompt_tool_injection,
        config,
    )
}

pub fn load_completion_checklist(repo_root: &Path) -> Result<String> {
    prompts_internal::review::load_completion_checklist(repo_root)
}

pub fn load_phase2_handoff_checklist(repo_root: &Path) -> Result<String> {
    prompts_internal::review::load_phase2_handoff_checklist(repo_root)
}

pub fn load_iteration_checklist(repo_root: &Path) -> Result<String> {
    prompts_internal::review::load_iteration_checklist(repo_root)
}

pub fn render_completion_checklist(
    template: &str,
    task_id: &str,
    config: &Config,
) -> Result<String> {
    prompts_internal::review::render_completion_checklist(template, task_id, config)
}

pub fn render_phase2_handoff_checklist(template: &str, config: &Config) -> Result<String> {
    prompts_internal::review::render_phase2_handoff_checklist(template, config)
}

pub fn render_iteration_checklist(
    template: &str,
    task_id: &str,
    config: &Config,
) -> Result<String> {
    prompts_internal::review::render_iteration_checklist(template, task_id, config)
}

pub(crate) fn load_code_review_prompt(repo_root: &Path) -> Result<String> {
    prompts_internal::review::load_code_review_prompt(repo_root)
}

pub(crate) fn render_code_review_prompt(
    template: &str,
    task_id: &str,
    project_type: ProjectType,
    config: &Config,
) -> Result<String> {
    prompts_internal::review::render_code_review_prompt(template, task_id, project_type, config)
}
