//! Aging report data structures.
//!
//! Purpose:
//! - Aging report data structures.
//!
//! Responsibilities:
//! - Define serializable aging report payload types.
//! - Keep aggregation output stable across text and JSON renderers.
//!
//! Not handled here:
//! - Per-task bucket computation.
//! - Text rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Bucket order is decided by the report builder, not by these types.

use serde::Serialize;

use crate::contracts::TaskStatus;

#[derive(Debug, Serialize)]
pub(super) struct AgingTaskEntry {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub age_seconds: i64,
    pub age_human: String,
    pub basis: String,
    pub anchor_ts: String,
}

#[derive(Debug, Serialize)]
pub(super) struct AgingBucketEntry {
    pub bucket: String,
    pub count: usize,
    pub tasks: Vec<AgingTaskEntry>,
}

#[derive(Debug, Serialize)]
pub(super) struct AgingTotals {
    pub total: usize,
    pub fresh: usize,
    pub warning: usize,
    pub stale: usize,
    pub rotten: usize,
    pub unknown: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct AgingThresholdsOutput {
    pub warning_days: u32,
    pub stale_days: u32,
    pub rotten_days: u32,
}

#[derive(Debug, Serialize)]
pub(super) struct AgingFilters {
    pub statuses: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct AgingReport {
    pub as_of: String,
    pub thresholds: AgingThresholdsOutput,
    pub filters: AgingFilters,
    pub totals: AgingTotals,
    pub buckets: Vec<AgingBucketEntry>,
}
