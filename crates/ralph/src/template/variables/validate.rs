//! Purpose: Scan template-backed task fields for supported and unknown
//! variables.
//!
//! Responsibilities:
//! - Extract `{{variable}}` placeholders from task fields.
//! - Detect whether branch context is required.
//! - Report unknown template variables as warnings.
//!
//! Scope:
//! - Validation only; no git probing or string substitution.
//!
//! Usage:
//! - Called by template loading before context detection and substitution.
//!
//! Invariants/Assumptions:
//! - Variable syntax remains `{{variable_name}}`.
//! - Unknown variables produce warnings rather than hard failures.
//! - Validation behavior and warning ordering remain unchanged.

use std::collections::HashSet;

use regex::Regex;

use crate::contracts::Task;

use super::context::{TemplateValidation, TemplateWarning};

/// The set of known/supported template variables.
const KNOWN_VARIABLES: &[&str] = &["target", "module", "file", "branch"];

/// Extract template variable occurrences from a string.
///
/// Returns a set of variable names found in the input (without braces).
pub(super) fn extract_variables(input: &str) -> HashSet<String> {
    let mut variables = HashSet::new();
    // Use lazy_static or thread_local for regex if performance is critical,
    // but for template loading (not hot path), we can compile on demand.
    // This function is called infrequently during template loading.
    let re = match Regex::new(r"\{\{(\w+)\}\}") {
        Ok(re) => re,
        Err(_) => return variables, // Should never happen with static pattern
    };

    for cap in re.captures_iter(input) {
        if let Some(matched) = cap.get(1) {
            variables.insert(matched.as_str().to_string());
        }
    }
    variables
}

/// Check if the input contains the {{branch}} variable.
pub(super) fn uses_branch_variable(input: &str) -> bool {
    input.contains("{{branch}}")
}

/// Validate a template task and collect warnings.
///
/// This scans all string fields in the task for:
/// - Unknown template variables (not in KNOWN_VARIABLES)
/// - Presence of {{branch}} variable (to determine if git detection is needed)
pub fn validate_task_template(task: &Task) -> TemplateValidation {
    let mut validation = TemplateValidation::default();
    let mut all_variables: HashSet<String> = HashSet::new();

    // Collect variables from all string fields
    let fields = [
        ("title", task.title.clone()),
        ("request", task.request.clone().unwrap_or_default()),
    ];

    for (field_name, value) in &fields {
        if uses_branch_variable(value) {
            validation.uses_branch = true;
        }
        let vars = extract_variables(value);
        for var in &vars {
            if !KNOWN_VARIABLES.contains(&var.as_str()) {
                validation.warnings.push(TemplateWarning::UnknownVariable {
                    name: var.clone(),
                    field: Some(field_name.to_string()),
                });
            }
            all_variables.insert(var.clone());
        }
    }

    // Check array fields
    let array_fields: [(&str, &[String]); 5] = [
        ("tags", &task.tags),
        ("scope", &task.scope),
        ("evidence", &task.evidence),
        ("plan", &task.plan),
        ("notes", &task.notes),
    ];

    for (field_name, values) in &array_fields {
        for value in *values {
            if uses_branch_variable(value) {
                validation.uses_branch = true;
            }
            let vars = extract_variables(value);
            for var in &vars {
                if !KNOWN_VARIABLES.contains(&var.as_str()) {
                    validation.warnings.push(TemplateWarning::UnknownVariable {
                        name: var.clone(),
                        field: Some(field_name.to_string()),
                    });
                }
                all_variables.insert(var.clone());
            }
        }
    }

    validation
}
