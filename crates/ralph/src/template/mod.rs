//! Task template system for Ralph.
//!
//! Templates provide pre-filled task fields for common patterns.
//! Built-in templates are embedded; custom templates can be added to `.ralph/templates/`.
//!
//! Responsibilities:
//! - Define built-in templates for common task types (bug, feature, refactor, test, docs).
//! - Load templates from `.ralph/templates/` directory (custom overrides).
//! - Merge template fields with user-provided options.
//!
//! Not handled here:
//! - Template application to task creation (see `crate::commands::task`).
//! - CLI argument parsing (see `crate::cli::task`).
//!
//! Invariants/assumptions:
//! - Template names are case-sensitive and must be valid filenames.
//! - Custom templates override built-in templates with the same name.
//! - Template JSON must parse to a valid Task struct (partial tasks allowed).

pub mod builtin;
pub mod loader;
pub mod merge;

pub use loader::{list_templates, load_template, TemplateInfo, TemplateSource};
pub use merge::{format_template_context, merge_template_with_options};
