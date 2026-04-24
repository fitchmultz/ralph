//! Configuration validation facade.
//!
//! Purpose:
//! - Configuration validation facade.
//!
//! Responsibilities:
//! - Coordinate focused validators for queue, trust, CI gate, agent, and git-ref rules.
//! - Re-export the config validation API used throughout the crate.
//!
//! Not handled here:
//! - Config loading or layer resolution.
//! - Queue file validation or prompt management.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Config version remains pinned to `1`.
//! - Validation errors stay descriptive and actionable.

mod agent;
mod ci_gate;
mod config_rules;
mod git_refs;
mod queue;
mod trust;

#[cfg(test)]
mod tests;

pub use agent::{validate_agent_binary_paths, validate_agent_patch};
pub use config_rules::validate_config;
pub use git_refs::git_ref_invalid_reason;
#[cfg(test)]
pub use queue::{
    ERR_EMPTY_QUEUE_DONE_FILE, ERR_EMPTY_QUEUE_FILE, ERR_EMPTY_QUEUE_ID_PREFIX,
    ERR_INVALID_QUEUE_ID_WIDTH,
};
pub use queue::{
    validate_queue_done_file_override, validate_queue_file_override,
    validate_queue_id_prefix_override, validate_queue_id_width_override, validate_queue_overrides,
};
#[cfg(test)]
pub use trust::ERR_PROJECT_EXECUTION_TRUST;
pub use trust::validate_project_execution_trust;
