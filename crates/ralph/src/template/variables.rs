//! Purpose: Provide the public template-variable API surface for validation,
//! context detection, and substitution.
//!
//! Responsibilities:
//! - Declare the `template::variables` child modules.
//! - Re-export the stable public API used by template loading and callers.
//!
//! Scope:
//! - Thin facade only; implementation lives in sibling files under
//!   `template/variables/`.
//!
//! Usage:
//! - Import public types and helpers through `crate::template` or
//!   `crate::template::variables`.
//!
//! Invariants/Assumptions:
//! - The public API surface remains stable across the split.
//! - Validation, detection, and substitution behavior lives in focused
//!   companions and must remain unchanged.

mod context;
mod detect;
mod substitute;
mod validate;

#[cfg(test)]
mod tests;

pub use context::{TemplateContext, TemplateValidation, TemplateWarning};
pub use detect::{detect_context, detect_context_with_warnings};
pub use substitute::{substitute_variables, substitute_variables_in_task};
pub use validate::validate_task_template;
