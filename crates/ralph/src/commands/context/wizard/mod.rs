//! Interactive AGENTS.md wizard facade.
//!
//! Purpose:
//! - Interactive AGENTS.md wizard facade.
//!
//! Responsibilities:
//! - Re-export the prompt, init, and update helpers used by context workflows.
//! - Keep the root wizard module thin while delegating behavior to focused companions.
//!
//! Not handled here:
//! - Prompt backend implementations.
//! - Init/update wizard step orchestration details.
//! - Wizard-specific test scenarios.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Wizard callers run in interactive TTY environments enforced by the CLI/workflow layer.
//! - Re-exported helpers preserve the prior wizard contracts exactly.

mod init;
mod prompt;
#[cfg(test)]
mod scripted;
mod types;
mod update;

pub(crate) use init::run_init_wizard;
pub(crate) use prompt::{ContextPrompter, DialoguerPrompter};
pub(crate) use types::ConfigHints;
pub(crate) use update::run_update_wizard;

#[cfg(test)]
mod tests;
