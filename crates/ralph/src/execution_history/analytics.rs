//! Execution-history weighting and timestamp helpers.
//!
//! Purpose:
//! - Execution-history weighting and timestamp helpers.
//!
//! Responsibilities:
//! - Compute weighted historical averages for phase durations.
//! - Parse persisted timestamps into Unix seconds for recency calculations.
//!
//! Not handled here:
//! - Disk IO.
//! - Real-time progress tracking.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Recent executions receive higher weight than older ones.
//! - Timestamp parsing only accepts RFC3339 values already used by persisted history.

use std::collections::HashMap;
use std::time::Duration;

use crate::progress::ExecutionPhase;

use super::model::ExecutionHistory;

const SECONDS_PER_DAY: f64 = 24.0 * 3600.0;
const RECENCY_DECAY_PER_DAY: f64 = 0.9;

/// Calculate weighted average duration for a specific phase.
///
/// Uses exponential weighting where recent entries are weighted higher.
/// weight = 0.9^(age_in_days)
pub fn weighted_average_duration(
    history: &ExecutionHistory,
    runner: &str,
    model: &str,
    phase_count: u8,
    phase: ExecutionPhase,
) -> Option<Duration> {
    let relevant_entries: Vec<_> = history
        .entries
        .iter()
        .filter(|entry| {
            entry.runner == runner
                && entry.model == model
                && entry.phase_count == phase_count
                && entry.phase_durations.contains_key(&phase)
        })
        .collect();

    if relevant_entries.is_empty() {
        return None;
    }

    let now = current_unix_time_secs();
    let mut total_weight = 0.0;
    let mut weighted_sum = 0.0;

    for entry in relevant_entries {
        let entry_secs = parse_timestamp_to_secs(&entry.timestamp).unwrap_or(now as u64) as f64;
        let age_days = (now - entry_secs) / SECONDS_PER_DAY;
        let weight = RECENCY_DECAY_PER_DAY.powf(age_days);

        if let Some(duration) = entry.phase_durations.get(&phase) {
            weighted_sum += duration.as_secs_f64() * weight;
            total_weight += weight;
        }
    }

    (total_weight > 0.0).then(|| Duration::from_secs_f64(weighted_sum / total_weight))
}

/// Get historical average durations for all phases.
pub fn get_phase_averages(
    history: &ExecutionHistory,
    runner: &str,
    model: &str,
    phase_count: u8,
) -> HashMap<ExecutionPhase, Duration> {
    let mut averages = HashMap::new();
    for phase in [
        ExecutionPhase::Planning,
        ExecutionPhase::Implementation,
        ExecutionPhase::Review,
    ] {
        if let Some(avg) = weighted_average_duration(history, runner, model, phase_count, phase) {
            averages.insert(phase, avg);
        }
    }
    averages
}

fn current_unix_time_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as f64
}

/// Parse RFC3339 timestamp to Unix seconds using proper RFC3339 parsing.
///
/// Uses the timeutil module for accurate parsing that correctly handles:
/// - Leap years
/// - Variable month lengths
/// - Timezone offsets
pub(crate) fn parse_timestamp_to_secs(timestamp: &str) -> Option<u64> {
    let dt = crate::timeutil::parse_rfc3339_opt(timestamp)?;
    let ts = dt.unix_timestamp();
    (ts >= 0).then_some(ts as u64)
}
