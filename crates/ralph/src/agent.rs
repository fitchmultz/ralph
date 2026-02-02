//! Agent argument resolution and configuration (public compatibility surface).
//!
//! Responsibilities:
//! - Provide the stable `crate::agent::*` API used across the crate.
//! - Re-export the actual implementation, which lives under `src/agent/`.
//!
//! Not handled here:
//! - Any parsing, validation, or resolution logic (see `src/agent/*.rs`).
//!
//! Invariants/assumptions:
//! - All public items previously available via `crate::agent::*` remain available
//!   with the same names and semantics after refactors.

mod args;
mod parse;
mod repoprompt;
mod resolve;

// Public API re-exports (backward compatibility)
pub use args::{AgentArgs, RunAgentArgs, RunnerCliArgs};
pub use parse::{parse_git_revert_mode, parse_runner};
pub use repoprompt::{
    RepoPromptMode, RepopromptFlags, resolve_repoprompt_flags, resolve_rp_required,
};
pub use resolve::{
    AgentOverrides, resolve_agent_overrides, resolve_repoprompt_flags_from_overrides,
    resolve_run_agent_overrides,
};
