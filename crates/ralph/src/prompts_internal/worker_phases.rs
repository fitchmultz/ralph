//! Worker phase prompt loading and rendering.
//!
//! Responsibilities: load phase-specific worker templates, render multi-phase content, and inject
//! RepoPrompt instructions when configured.
//! Not handled: base worker prompt rendering, checklist content generation, or queue/task
//! persistence.
//! Invariants/assumptions: phase templates include expected placeholders and rendering inputs are
//! pre-trimmed by callers.

use super::registry::{PromptTemplateId, load_prompt_template, prompt_template};
use super::util::{ensure_no_unresolved_placeholders, escape_placeholder_like_text};
use crate::contracts::Config;
use anyhow::{Result, bail};

pub(crate) fn load_worker_phase1_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::WorkerPhase1)
}

pub(crate) fn load_worker_phase2_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::WorkerPhase2)
}

pub(crate) fn load_worker_phase2_handoff_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::WorkerPhase2Handoff)
}

pub(crate) fn load_worker_phase3_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::WorkerPhase3)
}

pub(crate) fn load_worker_single_phase_prompt(repo_root: &std::path::Path) -> Result<String> {
    load_prompt_template(repo_root, PromptTemplateId::WorkerSinglePhase)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase1_prompt(
    template: &str,
    base_worker_prompt: &str,
    iteration_context: &str,
    task_refresh_instruction: &str,
    task_id: &str,
    total_phases: u8,
    plan_path: &str,
    repoprompt_plan_required: bool,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::WorkerPhase1);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: worker phase1 prompt requires a non-empty task id.");
    }

    let expanded = super::util::expand_variables(template, config)?;
    let repoprompt_block = repoprompt_block(repoprompt_tool_injection, repoprompt_plan_required);
    let safe_iteration_context = escape_placeholder_like_text(iteration_context.trim());
    let safe_base_worker_prompt = escape_placeholder_like_text(base_worker_prompt);
    let rendered = expanded
        .replace("{{ITERATION_CONTEXT}}", iteration_context.trim())
        .replace(
            "{{TASK_REFRESH_INSTRUCTION}}",
            task_refresh_instruction.trim(),
        )
        .replace("{{TASK_ID}}", id)
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{PLAN_PATH}}", plan_path)
        .replace("{{BASE_WORKER_PROMPT}}", base_worker_prompt)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    let rendered_for_validation = expanded
        .replace("{{ITERATION_CONTEXT}}", safe_iteration_context.trim())
        .replace(
            "{{TASK_REFRESH_INSTRUCTION}}",
            task_refresh_instruction.trim(),
        )
        .replace("{{TASK_ID}}", id)
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{PLAN_PATH}}", plan_path)
        .replace("{{BASE_WORKER_PROMPT}}", safe_base_worker_prompt.trim())
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(clean_repoprompt_spacing(
        rendered,
        repoprompt_plan_required || repoprompt_tool_injection,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase2_prompt(
    template: &str,
    base_worker_prompt: &str,
    plan_text: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::WorkerPhase2);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: worker phase2 prompt requires a non-empty task id.");
    }
    let expanded = super::util::expand_variables(template, config)?;
    let repoprompt_block = repoprompt_block(repoprompt_tool_injection, false);
    let safe_plan_text = escape_placeholder_like_text(plan_text.trim());
    let safe_iteration_context = escape_placeholder_like_text(iteration_context.trim());
    let safe_iteration_completion_block =
        escape_placeholder_like_text(iteration_completion_block.trim());
    let safe_base_worker_prompt = escape_placeholder_like_text(base_worker_prompt);
    let rendered = expanded
        .replace("{{PLAN_TEXT}}", plan_text.trim())
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            iteration_completion_block.trim(),
        )
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{BASE_WORKER_PROMPT}}", base_worker_prompt)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    let rendered_for_validation = expanded
        .replace("{{PLAN_TEXT}}", safe_plan_text.trim())
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", safe_iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            safe_iteration_completion_block.trim(),
        )
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{BASE_WORKER_PROMPT}}", safe_base_worker_prompt.trim())
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(clean_repoprompt_spacing(
        rendered,
        repoprompt_tool_injection,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase2_handoff_prompt(
    template: &str,
    base_worker_prompt: &str,
    plan_text: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::WorkerPhase2Handoff);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: worker phase2 handoff prompt requires a non-empty task id.");
    }
    let expanded = super::util::expand_variables(template, config)?;
    let repoprompt_block = repoprompt_block(repoprompt_tool_injection, false);
    let safe_plan_text = escape_placeholder_like_text(plan_text.trim());
    let safe_iteration_context = escape_placeholder_like_text(iteration_context.trim());
    let safe_iteration_completion_block =
        escape_placeholder_like_text(iteration_completion_block.trim());
    let safe_base_worker_prompt = escape_placeholder_like_text(base_worker_prompt);
    let rendered = expanded
        .replace("{{PLAN_TEXT}}", plan_text.trim())
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            iteration_completion_block.trim(),
        )
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{BASE_WORKER_PROMPT}}", base_worker_prompt)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    let rendered_for_validation = expanded
        .replace("{{PLAN_TEXT}}", safe_plan_text.trim())
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", safe_iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            safe_iteration_completion_block.trim(),
        )
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{BASE_WORKER_PROMPT}}", safe_base_worker_prompt.trim())
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());
    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(clean_repoprompt_spacing(
        rendered,
        repoprompt_tool_injection,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_phase3_prompt(
    template: &str,
    base_worker_prompt: &str,
    code_review_body: &str,
    phase2_final_response: &str,
    task_id: &str,
    completion_checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    phase3_completion_guidance: &str,
    total_phases: u8,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::WorkerPhase3);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: worker phase3 prompt requires a non-empty task id.");
    }
    let expanded = super::util::expand_variables(template, config)?;
    let mut review_body = code_review_body.trim().to_string();
    if base_worker_prompt.contains("## PROJECT TYPE:") {
        review_body = strip_project_type_guidance(&review_body);
    }
    let repoprompt_block = repoprompt_block(repoprompt_tool_injection, false);
    let safe_phase2_final_response = escape_placeholder_like_text(phase2_final_response.trim());
    let safe_iteration_context = escape_placeholder_like_text(iteration_context.trim());
    let safe_iteration_completion_block =
        escape_placeholder_like_text(iteration_completion_block.trim());
    let safe_phase3_completion_guidance =
        escape_placeholder_like_text(phase3_completion_guidance.trim());
    let safe_base_worker_prompt = escape_placeholder_like_text(base_worker_prompt);
    let rendered = expanded
        .replace("{{CODE_REVIEW_BODY}}", review_body.trim())
        .replace("{{COMPLETION_CHECKLIST}}", completion_checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            iteration_completion_block.trim(),
        )
        .replace(
            "{{PHASE3_COMPLETION_GUIDANCE}}",
            phase3_completion_guidance.trim(),
        )
        .replace("{{PHASE2_FINAL_RESPONSE}}", phase2_final_response.trim())
        .replace("{{BASE_WORKER_PROMPT}}", base_worker_prompt)
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    let rendered_for_validation = expanded
        .replace("{{CODE_REVIEW_BODY}}", review_body.trim())
        .replace("{{COMPLETION_CHECKLIST}}", completion_checklist.trim())
        .replace(
            "{{PHASE2_FINAL_RESPONSE}}",
            safe_phase2_final_response.trim(),
        )
        .replace("{{ITERATION_CONTEXT}}", safe_iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            safe_iteration_completion_block.trim(),
        )
        .replace(
            "{{PHASE3_COMPLETION_GUIDANCE}}",
            safe_phase3_completion_guidance.trim(),
        )
        .replace("{{BASE_WORKER_PROMPT}}", safe_base_worker_prompt.trim())
        .replace("{{TOTAL_PHASES}}", &total_phases.to_string())
        .replace("{{TASK_ID}}", id)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(clean_repoprompt_spacing(
        rendered,
        repoprompt_tool_injection,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_worker_single_phase_prompt(
    template: &str,
    base_worker_prompt: &str,
    checklist: &str,
    iteration_context: &str,
    iteration_completion_block: &str,
    task_id: &str,
    repoprompt_tool_injection: bool,
    config: &Config,
) -> Result<String> {
    let template_meta = prompt_template(PromptTemplateId::WorkerSinglePhase);
    let id = task_id.trim();
    if id.is_empty() {
        bail!("Missing task id: worker single-phase prompt requires a non-empty task id.");
    }

    let expanded = super::util::expand_variables(template, config)?;
    let repoprompt_block = repoprompt_block(repoprompt_tool_injection, false);
    let safe_iteration_context = escape_placeholder_like_text(iteration_context.trim());
    let safe_iteration_completion_block =
        escape_placeholder_like_text(iteration_completion_block.trim());
    let safe_base_worker_prompt = escape_placeholder_like_text(base_worker_prompt);
    let rendered = expanded
        .replace("{{TASK_ID}}", id)
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            iteration_completion_block.trim(),
        )
        .replace("{{BASE_WORKER_PROMPT}}", base_worker_prompt)
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    let rendered_for_validation = expanded
        .replace("{{TASK_ID}}", id)
        .replace("{{CHECKLIST}}", checklist.trim())
        .replace("{{ITERATION_CONTEXT}}", safe_iteration_context.trim())
        .replace(
            "{{ITERATION_COMPLETION_BLOCK}}",
            safe_iteration_completion_block.trim(),
        )
        .replace("{{BASE_WORKER_PROMPT}}", safe_base_worker_prompt.trim())
        .replace("{{REPOPROMPT_BLOCK}}", repoprompt_block.trim());

    ensure_no_unresolved_placeholders(&rendered_for_validation, template_meta.label)?;
    Ok(clean_repoprompt_spacing(
        rendered,
        repoprompt_tool_injection,
    ))
}

fn repoprompt_block(tool_injection: bool, plan_required: bool) -> String {
    if !tool_injection && !plan_required {
        return String::new();
    }

    let mut sections = Vec::new();
    if tool_injection {
        sections.push(super::util::REPOPROMPT_REQUIRED_INSTRUCTION.trim());
    }
    if plan_required {
        sections.push(super::util::REPOPROMPT_CONTEXT_BUILDER_PLANNING_INSTRUCTION.trim());
    }

    sections.join("\n\n")
}

fn clean_repoprompt_spacing(rendered: String, repoprompt_present: bool) -> String {
    if repoprompt_present {
        return rendered;
    }

    let mut cleaned = rendered.replace("\n\n\n", "\n\n");
    while cleaned.contains("\n\n\n") {
        cleaned = cleaned.replace("\n\n\n", "\n\n");
    }
    cleaned
}

fn strip_project_type_guidance(review_body: &str) -> String {
    if let Some(start) = review_body.find("## PROJECT TYPE:") {
        if let Some(end) = review_body[start..].find("\n## ") {
            let end_idx = start + end + 1;
            let mut stripped = String::new();
            stripped.push_str(review_body[..start].trim_end());
            if !stripped.is_empty() {
                stripped.push('\n');
                stripped.push('\n');
            }
            stripped.push_str(review_body[end_idx..].trim_start());
            return stripped;
        }
        return review_body[..start].trim_end().to_string();
    }
    review_body.to_string()
}
