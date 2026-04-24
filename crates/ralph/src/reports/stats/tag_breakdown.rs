//! Tag-breakdown helpers for stats reports.
//!
//! Purpose:
//! - Tag-breakdown helpers for stats reports.
//!
//! Responsibilities:
//! - Count tag occurrences across the filtered report task set.
//! - Convert normalized counts into stable, sorted report output.
//!
//! Not handled here:
//! - Task filtering or summary totals.
//! - Text rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tags are normalized to lowercase for grouping.
//! - Percentages are measured against filtered task count, preserving existing report semantics.

use std::collections::HashMap;

use crate::contracts::Task;

use super::model::TagBreakdown;

pub(super) fn build_tag_breakdown(tasks: &[&Task], total_tasks: usize) -> Vec<TagBreakdown> {
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for task in tasks {
        for tag in &task.tags {
            *tag_counts.entry(tag.to_lowercase()).or_insert(0) += 1;
        }
    }

    let mut sorted_tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    sorted_tags.sort_by(|left, right| right.1.cmp(&left.1));

    let total = total_tasks as f64;
    sorted_tags
        .into_iter()
        .map(|(tag, count)| TagBreakdown {
            tag,
            count,
            percentage: if total == 0.0 {
                0.0
            } else {
                (count as f64 / total) * 100.0
            },
        })
        .collect()
}
