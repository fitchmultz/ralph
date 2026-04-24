//! Stats report assembly helpers.
//!
//! Purpose:
//! - Stats report assembly helpers.
//!
//! Responsibilities:
//! - Compose the full `StatsReport` from filtered tasks and focused helpers.
//! - Keep top-level stats facade free of metric-construction details.
//!
//! Not handled here:
//! - Text or JSON rendering.
//! - Execution-history ETA lookup.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue inputs are already validated.
//! - Tag filtering happens before all downstream metric calculations.

use crate::contracts::QueueFile;

use super::breakdowns::{calc_slow_groups, calc_velocity_breakdowns};
use super::model::{StatsFilters, StatsReport};
use super::summary::{
    build_time_tracking_stats, collect_all_tasks, filter_tasks_by_tags, summarize_tasks,
};
use super::tag_breakdown::build_tag_breakdown;

pub(crate) fn build_stats_report(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    tags: &[String],
) -> StatsReport {
    let all_tasks = collect_all_tasks(queue, done);
    let filtered_tasks = filter_tasks_by_tags(all_tasks, tags);
    let summary = summarize_tasks(&filtered_tasks);
    let time_tracking = build_time_tracking_stats(&filtered_tasks);
    let velocity = calc_velocity_breakdowns(&filtered_tasks);
    let slow_groups = calc_slow_groups(&filtered_tasks);
    let tag_breakdown = build_tag_breakdown(&filtered_tasks, summary.total);

    StatsReport {
        summary,
        durations: time_tracking.lead_time.clone(),
        time_tracking,
        velocity,
        slow_groups,
        tag_breakdown,
        filters: StatsFilters {
            tags: tags.to_vec(),
        },
        execution_history_eta: None,
    }
}
