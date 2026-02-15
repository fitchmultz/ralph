//! Runner management command implementations.
//!
//! Responsibilities:
//! - Provide handlers for runner CLI subcommands.
//! - Expose capability data for CLI and potential future use.
//!
//! Not handled here:
//! - CLI argument parsing (see cli/runner.rs).
//! - Runner execution (see runner/ module).

pub mod capabilities;
pub mod detection;
pub mod list;
