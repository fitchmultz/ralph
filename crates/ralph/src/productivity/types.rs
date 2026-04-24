//! Productivity stats data structures.
//!
//! Purpose:
//! - Productivity stats data structures.
//!
//! Responsibilities:
//! - Define all data structures for productivity tracking (stats, streaks, milestones, reports).
//!
//! Not handled here:
//! - Persistence logic (see `super::persistence`).
//! - Calculations and business logic (see `super::calculations`).
//! - Report formatting and display (see `super::reports`).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::constants::versions::STATS_SCHEMA_VERSION;
use crate::timeutil;

/// Root productivity data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductivityStats {
    /// Schema version for migrations
    pub version: u32,
    /// When stats were first created
    pub first_task_completed_at: Option<String>,
    /// Last update timestamp
    pub last_updated_at: String,
    /// Daily completion records (YYYY-MM-DD -> DayStats)
    pub daily: BTreeMap<String, DayStats>,
    /// Current streak information
    pub streak: StreakInfo,
    /// Total completed task counter for milestones
    pub total_completed: u64,
    /// Milestones achieved
    pub milestones: Vec<Milestone>,
}

impl Default for ProductivityStats {
    fn default() -> Self {
        let now = timeutil::now_utc_rfc3339_or_fallback();
        Self {
            version: STATS_SCHEMA_VERSION,
            first_task_completed_at: None,
            last_updated_at: now,
            daily: BTreeMap::new(),
            streak: StreakInfo::default(),
            total_completed: 0,
            milestones: Vec::new(),
        }
    }
}

/// Stats for a single day
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DayStats {
    pub date: String,
    pub completed_count: u32,
    pub tasks: Vec<CompletedTaskRef>,
}

/// Reference to a completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedTaskRef {
    pub id: String,
    pub title: String,
    pub completed_at: String,
}

/// Streak tracking information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreakInfo {
    pub current_streak: u32,
    pub longest_streak: u32,
    pub last_completed_date: Option<String>,
}

/// A milestone achievement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub threshold: u64,
    pub achieved_at: String,
    pub celebrated: bool,
}

/// Result of recording a task completion
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// Milestone achieved (if any)
    pub milestone_achieved: Option<u64>,
    /// Whether streak was updated
    pub streak_updated: bool,
    /// New streak count
    pub new_streak: u32,
    /// Total completed count
    pub total_completed: u64,
}

/// Velocity metrics for a given time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityMetrics {
    pub days: u32,
    pub total_completed: u32,
    pub average_per_day: f64,
    pub best_day: Option<(String, u32)>,
}

/// Estimation accuracy metrics for tasks with both estimated and actual minutes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimationMetrics {
    /// Number of tasks analyzed (have both estimated and actual minutes).
    pub tasks_analyzed: u32,
    /// Average estimation accuracy ratio (actual/estimated).
    /// 1.0 = perfect estimation, <1.0 = overestimated, >1.0 = underestimated.
    pub average_accuracy_ratio: f64,
    /// Median estimation accuracy ratio.
    pub median_accuracy_ratio: f64,
    /// Percentage of tasks estimated within 25% of actual.
    pub within_25_percent: f64,
    /// Average absolute error in minutes.
    pub average_absolute_error_minutes: f64,
}

/// Single task estimation data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEstimationPoint {
    pub task_id: String,
    pub task_title: String,
    pub estimated_minutes: u32,
    pub actual_minutes: u32,
    pub accuracy_ratio: f64,
}

/// Session summary for display after run loop
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub tasks_completed: Vec<String>,
    pub session_start: String,
    pub session_duration_seconds: i64,
}

/// Productivity summary report
#[derive(Debug, Clone, Serialize)]
pub struct ProductivitySummaryReport {
    pub total_completed: u64,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub last_completed_date: Option<String>,
    pub next_milestone: Option<u64>,
    pub milestones: Vec<Milestone>,
    pub recent_completions: Vec<CompletedTaskRef>,
}

/// Productivity streak report
#[derive(Debug, Clone, Serialize)]
pub struct ProductivityStreakReport {
    pub current_streak: u32,
    pub longest_streak: u32,
    pub last_completed_date: Option<String>,
}

/// Productivity velocity report
#[derive(Debug, Clone, Serialize)]
pub struct ProductivityVelocityReport {
    pub window_days: u32,
    pub total_completed: u32,
    pub average_per_day: f64,
    pub best_day: Option<(String, u32)>,
}

/// Productivity estimation report
#[derive(Debug, Clone, Serialize)]
pub struct ProductivityEstimationReport {
    pub tasks_analyzed: u32,
    pub average_accuracy_ratio: f64,
    pub median_accuracy_ratio: f64,
    pub within_25_percent: f64,
    pub average_absolute_error_minutes: f64,
}
