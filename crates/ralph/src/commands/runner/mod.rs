//! Runner management command implementations.
//!
//! Purpose:
//! - Runner management command implementations.
//!
//! Responsibilities:
//! - Provide handlers for runner CLI subcommands.
//! - Expose capability data for CLI and potential future use.
//!
//! Not handled here:
//! - CLI argument parsing (see cli/runner.rs).
//! - Runner execution (see runner/ module).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

pub mod capabilities;
pub mod detection;
pub mod list;
