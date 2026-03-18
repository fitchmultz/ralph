//! Purpose: Directory-backed facade for managed subprocess helpers.
//!
//! Responsibilities:
//! - Declare the focused managed-subprocess companion modules.
//! - Re-export the stable shell-execution surface used by `crate::runutil` and sibling modules.
//!
//! Scope:
//! - Thin facade only; command execution behavior lives in companion modules.
//! - Regression coverage for shell helpers lives in `tests.rs`.
//!
//! Usage:
//! - Imported by `crate::runutil` and adjacent operational helpers that need managed subprocesses.
//!
//! Invariants/Assumptions:
//! - Re-exported managed-subprocess APIs keep their existing signatures.
//! - Companion modules stay private to the shell boundary.

mod argv;
mod capture;
mod execute;
mod sleep;
mod types;
mod wait;

#[cfg(test)]
mod tests;

pub(crate) use execute::{execute_checked_command, execute_managed_command, execute_safe_command};
pub(crate) use sleep::sleep_with_cancellation;
pub(crate) use types::{ManagedCommand, SafeCommand, TimeoutClass};
