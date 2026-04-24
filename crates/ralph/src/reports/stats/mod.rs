//! Stats report implementation.
//!
//! Purpose:
//! - Stats report implementation.
//!
//! Responsibilities:
//! - Assemble queue statistics from validated queue and done files.
//! - Coordinate summary, time-tracking, breakdown, ETA, and rendering helpers.
//! - Keep `build_stats_report` and `print_stats` as the public stats entrypoints.
//!
//! Not handled here:
//! - CLI argument parsing.
//! - Queue persistence or mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue inputs are already validated.
//! - Rendering reuses the computed `StatsReport` instead of recomputing metrics.

mod breakdowns;
mod entry;
mod eta;
mod model;
pub(super) mod render;
mod report;
mod summary;
mod tag_breakdown;
#[cfg(test)]
mod tests;

pub(crate) use model::StatsReport;

pub(crate) use entry::print_stats;
pub(crate) use report::build_stats_report;
