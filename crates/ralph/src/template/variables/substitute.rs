//! Purpose: Apply resolved template-variable context to strings and tasks.
//!
//! Responsibilities:
//! - Substitute supported variables in individual strings.
//! - Apply substitution across all template-backed task fields.
//!
//! Scope:
//! - Substitution only; no validation or context detection.
//!
//! Usage:
//! - Called after validation and context detection during template loading.
//!
//! Invariants/Assumptions:
//! - Unknown variables remain unchanged.
//! - Missing context values leave their placeholders unchanged.
//! - Field coverage remains aligned with the previous monolithic implementation.

use crate::contracts::Task;

use super::context::TemplateContext;

/// Substitute variables in a template string.
///
/// Supported variables:
/// - {{target}} - The target file/path provided by user
/// - {{module}} - Module name derived from target
/// - {{file}} - Filename only
/// - {{branch}} - Current git branch name
pub fn substitute_variables(input: &str, context: &TemplateContext) -> String {
    let mut result = input.to_string();

    if let Some(target) = &context.target {
        result = result.replace("{{target}}", target);
    }

    if let Some(module) = &context.module {
        result = result.replace("{{module}}", module);
    }

    if let Some(file) = &context.file {
        result = result.replace("{{file}}", file);
    }

    if let Some(branch) = &context.branch {
        result = result.replace("{{branch}}", branch);
    }

    result
}

/// Substitute variables in all string fields of a Task.
pub fn substitute_variables_in_task(task: &mut Task, context: &TemplateContext) {
    task.title = substitute_variables(&task.title, context);

    for tag in &mut task.tags {
        *tag = substitute_variables(tag, context);
    }

    for scope in &mut task.scope {
        *scope = substitute_variables(scope, context);
    }

    for evidence in &mut task.evidence {
        *evidence = substitute_variables(evidence, context);
    }

    for plan in &mut task.plan {
        *plan = substitute_variables(plan, context);
    }

    for note in &mut task.notes {
        *note = substitute_variables(note, context);
    }

    if let Some(request) = &mut task.request {
        *request = substitute_variables(request, context);
    }
}
