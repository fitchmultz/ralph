//! Stats report data structures.
//!
//! Purpose:
//! - Stats report data structures.
//!
//! Responsibilities:
//! - Define the serializable stats report model shared by text and JSON rendering.
//! - Keep report payload types independent from calculation helpers.
//!
//! Not handled here:
//! - Metric calculation.
//! - Rendering or ETA resolution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - These types stay serialization-friendly for CLI and dashboard consumers.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct StatsSummary {
    pub total: usize,
    pub done: usize,
    pub rejected: usize,
    pub terminal: usize,
    pub active: usize,
    pub terminal_rate: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct StatsFilters {
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DurationStats {
    pub count: usize,
    pub average_seconds: i64,
    pub median_seconds: i64,
    pub average_human: String,
    pub median_human: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct TimeTrackingStats {
    pub lead_time: Option<DurationStats>,
    pub work_time: Option<DurationStats>,
    pub start_lag: Option<DurationStats>,
}

#[derive(Debug, Serialize)]
pub(crate) struct VelocityBreakdownEntry {
    pub key: String,
    pub last_7_days: u32,
    pub last_30_days: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct VelocityBreakdowns {
    pub by_tag: Vec<VelocityBreakdownEntry>,
    pub by_runner: Vec<VelocityBreakdownEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SlowGroupEntry {
    pub key: String,
    pub count: usize,
    pub median_seconds: i64,
    pub median_human: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct SlowGroups {
    pub by_tag: Vec<SlowGroupEntry>,
    pub by_runner: Vec<SlowGroupEntry>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TagBreakdown {
    pub tag: String,
    pub count: usize,
    pub percentage: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ExecutionHistoryEtaReport {
    pub runner: String,
    pub model: String,
    pub phase_count: u8,
    pub sample_count: usize,
    pub estimated_total_seconds: u64,
    pub estimated_total_human: String,
    pub confidence: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct StatsReport {
    pub summary: StatsSummary,
    pub durations: Option<DurationStats>,
    pub time_tracking: TimeTrackingStats,
    pub velocity: VelocityBreakdowns,
    pub slow_groups: SlowGroups,
    pub tag_breakdown: Vec<TagBreakdown>,
    pub filters: StatsFilters,
    pub execution_history_eta: Option<ExecutionHistoryEtaReport>,
}
