//! Command implementation layer.
//!
//! Purpose:
//! - Centralize "do the work" command implementations separate from the CLI surface (`crate::cli`).
//! - Mirror the CLI structure for easier navigation and maintenance.
//!
//! Non-goals:
//! - This module does not define clap argument parsing; see `crate::cli` for that.

pub mod app;
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
