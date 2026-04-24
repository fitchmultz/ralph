//! Task edit helpers shared by CLI and GUI clients.
//!
//! Purpose:
//! - Task edit helpers shared by CLI and GUI clients.
//!
//! Responsibilities:
//! - Apply edits to a single task and update related timestamps.
//! - Parse and validate edit input (status, priority, custom fields, RFC3339 values).
//! - Preview changes before applying them.
//!
//! Non-scope:
//! - Persisting queue files or locating tasks outside the provided queue.
//! - Cross-task dependency resolution beyond status policy checks.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Callers provide a loaded `QueueFile` and a valid RFC3339 `now` value.
//! - Task IDs are matched after trimming and are case-sensitive.

mod apply;
mod key;
mod parsing;
mod preview;
mod validate_input;

#[cfg(test)]
mod tests;

pub use apply::apply_task_edit;
pub use key::TaskEditKey;
pub use preview::{TaskEditPreview, preview_task_edit};

// Internal helper re-exported for tests (crate-visible only)
#[cfg(test)]
pub(crate) use preview::format_field_value;
