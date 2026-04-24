//! Productivity stats tracking for task completions, streaks, and velocity metrics.
//!
//! Purpose:
//! - Productivity stats tracking for task completions, streaks, and velocity metrics.
//!
//! Responsibilities:
//! - Track daily task completions and calculate streaks
//! - Record milestone achievements (10, 50, 100, etc.)
//! - Calculate velocity metrics (tasks per day/week)
//! - Persist stats to `.ralph/cache/productivity.jsonc`
//!
//! Not handled here:
//! - Queue/task management (see `crate::queue`)
//! - Notification delivery (see `crate::notification`)
//! - Celebration rendering (see `crate::celebrations`)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Stats file is JSON with schema version for migrations
//! - Timestamps are RFC3339 format
//! - All operations are atomic (read-modify-write with file locking)

// Submodules
mod calculations;
mod date_utils;
mod persistence;
mod reports;
mod types;

#[cfg(test)]
mod tests;

// Re-export types
pub use types::{
    CompletedTaskRef, CompletionResult, DayStats, EstimationMetrics, Milestone,
    ProductivityEstimationReport, ProductivityStats, ProductivityStreakReport,
    ProductivitySummaryReport, ProductivityVelocityReport, SessionSummary, StreakInfo,
    TaskEstimationPoint, VelocityMetrics,
};

// Re-export persistence functions
pub use persistence::{load_productivity_stats, save_productivity_stats};

// Re-export calculation functions
pub use calculations::{
    calculate_estimation_metrics, calculate_velocity, mark_milestone_celebrated, next_milestone,
    record_task_completion, record_task_completion_by_id, update_streak,
};

// Re-export report functions
pub use reports::{
    build_estimation_report, build_streak_report, build_summary_report, build_velocity_report,
    format_duration, print_estimation_report_text, print_streak_report_text,
    print_summary_report_text, print_velocity_report_text,
};

// Re-export date utilities for advanced use cases
pub use date_utils::{date_key_add_days, format_date_key, parse_date_key, previous_date_key};
