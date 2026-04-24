//! Command implementation layer.
//!
//! Purpose:
//! - Centralize "do the work" command implementations separate from the CLI surface (`crate::cli`).
//! - Mirror the CLI structure for easier navigation and maintenance.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Non-goals:
//! - This module does not define clap argument parsing; see `crate::cli` for that.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

pub mod app;
pub mod cleanup;
pub mod cli_spec;
pub mod context;
pub mod daemon;
pub mod doctor;

// Re-export commonly used doctor types for convenience
pub use doctor::types::{CheckResult, CheckSeverity, DoctorReport};
pub use doctor::{print_doctor_report_text, run_doctor};
pub mod init;
pub mod plugin;
pub mod prd;
pub mod prompt;
pub mod run;
pub mod runner;
pub mod scan;
pub mod task;
pub mod tutorial;
pub mod watch;
