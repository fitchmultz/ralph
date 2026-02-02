//! Task template system for Ralph.
//!
//! Templates provide pre-filled task fields for common patterns.
//! Built-in templates are embedded; custom templates can be added to `.ralph/templates/`.
//!
//! Responsibilities:
//! - Define built-in templates for common task types (bug, feature, refactor, test, docs).
//! - Load templates from `.ralph/templates/` directory (custom overrides).
//! - Merge template fields with user-provided options.
//! - Validate templates and report warnings for unknown variables.
//!
//! Not handled here:
//! - Template application to task creation (see `crate::commands::task`).
//! - CLI argument parsing (see `crate::cli::task`).
//!
//! Invariants/assumptions:
//! - Template names are case-sensitive and must be valid filenames.
//! - Custom templates override built-in templates with the same name.
//! - Template JSON must parse to a valid Task struct (partial tasks allowed).
//! - Unknown variables produce warnings; strict mode fails on unknown variables.

pub mod builtin;
pub mod loader;
pub mod merge;
pub mod variables;

pub use loader::{
    LoadedTemplate, TemplateInfo, TemplateSource, list_templates, load_template,
    load_template_with_context, load_template_with_context_legacy,
};
pub use merge::{format_template_context, merge_template_with_options};
pub use variables::{
    TemplateContext, TemplateValidation, TemplateWarning, detect_context,
    detect_context_with_warnings, substitute_variables, substitute_variables_in_task,
    validate_task_template,
};
