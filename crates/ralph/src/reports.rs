//! Task statistics and reporting commands.
//!
//! Purpose:
//! - Task statistics and reporting commands.
//!
//! Responsibilities:
//! - Provide analytics reports for queue inspection.
//! - Define shared report output format used by queue CLI.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled:
//! - CLI argument parsing or queue persistence.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Inputs are validated queue files.

mod aging;
mod burndown;
mod dashboard;
mod history;
mod shared;
mod stats;

pub(crate) use shared::ReportFormat;

pub(crate) use aging::print_aging;
pub(crate) use burndown::print_burndown;
pub(crate) use dashboard::{build_dashboard_report, print_dashboard};
pub(crate) use history::print_history;
pub(crate) use stats::print_stats;

// Re-export aging types for CLI usage (e.g., cli/queue/aging.rs).
pub(crate) use aging::AgingThresholds;
