//! Task statistics and reporting commands.
//!
//! Responsibilities:
//! - Provide analytics reports for queue inspection.
//! - Define shared report output format used by queue CLI.
//!
//! Not handled:
//! - CLI argument parsing or queue persistence.
//!
//! Invariants/assumptions:
//! - Inputs are validated queue files.

mod aging;
mod burndown;
mod history;
mod shared;
mod stats;

pub(crate) use shared::ReportFormat;

pub(crate) use aging::print_aging;
pub(crate) use burndown::print_burndown;
pub(crate) use history::print_history;
pub(crate) use stats::print_stats;

// Re-export aging types for non-CLI clients (e.g., the macOS app).
#[allow(unused_imports)]
pub(crate) use aging::{AgingBucket, AgingThresholds, compute_task_aging};
