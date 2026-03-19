//! Purpose: Define shared data types for template-variable validation and
//! substitution.
//!
//! Responsibilities:
//! - Represent substitution context derived from targets and git state.
//! - Represent validation warnings and aggregate validation results.
//!
//! Scope:
//! - Data modeling only; no template scanning, substitution, or git probing.
//!
//! Usage:
//! - Used by the `validate`, `detect`, and `substitute` companions and their
//!   callers through re-exports from `template::variables`.
//!
//! Invariants/Assumptions:
//! - Warning display text is part of the user-facing contract.
//! - Validation helpers preserve existing warning semantics and ordering.

/// Context for template variable substitution.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    /// The target file/path provided by user.
    pub target: Option<String>,
    /// Module name derived from target (e.g., "src/cli/task.rs" -> "cli::task").
    pub module: Option<String>,
    /// Filename only (e.g., "src/cli/task.rs" -> "task.rs").
    pub file: Option<String>,
    /// Current git branch name.
    pub branch: Option<String>,
}

/// Warning types for template validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateWarning {
    /// Unknown template variable found (variable name, optional field context).
    UnknownVariable { name: String, field: Option<String> },
    /// Git branch detection failed (error message).
    GitBranchDetectionFailed { error: String },
}

impl std::fmt::Display for TemplateWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateWarning::UnknownVariable { name, field: None } => {
                write!(f, "Unknown template variable: {{{{{}}}}}", name)
            }
            TemplateWarning::UnknownVariable {
                name,
                field: Some(field),
            } => {
                write!(
                    f,
                    "Unknown template variable in {}: {{{{{}}}}}",
                    field, name
                )
            }
            TemplateWarning::GitBranchDetectionFailed { error } => {
                write!(f, "Git branch detection failed: {}", error)
            }
        }
    }
}

/// Result of template validation.
#[derive(Debug, Clone, Default)]
pub struct TemplateValidation {
    /// Warnings collected during validation.
    pub warnings: Vec<TemplateWarning>,
    /// Whether the template uses {{branch}} variable.
    pub uses_branch: bool,
}

impl TemplateValidation {
    /// Check if there are any unknown variable warnings.
    pub fn has_unknown_variables(&self) -> bool {
        self.warnings
            .iter()
            .any(|w| matches!(w, TemplateWarning::UnknownVariable { .. }))
    }

    /// Get list of unknown variable names (deduplicated).
    pub fn unknown_variable_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .warnings
            .iter()
            .filter_map(|w| match w {
                TemplateWarning::UnknownVariable { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
}
