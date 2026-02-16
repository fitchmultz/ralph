//! Batch task filtering and ID resolution.
//!
//! Responsibilities:
//! - Filter tasks by tags, status, priority, scope, and age.
//! - Resolve task IDs from explicit lists or tag filters.
//! - Parse "older than" specifications into RFC3339 cutoffs.
//!
//! Does not handle:
//! - Actual task mutations (see update.rs, delete.rs, etc.).
//! - Display/output formatting (see display.rs).
//!
//! Assumptions/invariants:
//! - Tag filtering is case-insensitive and OR-based (any tag matches).
//! - Status/priority/scope filters use OR logic within each filter type.

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use anyhow::{Result, bail};

/// Filters for batch task selection.
#[derive(Debug, Clone, Default)]
pub struct BatchTaskFilters {
    pub status_filter: Vec<TaskStatus>,
    pub priority_filter: Vec<TaskPriority>,
    pub scope_filter: Vec<String>,
    pub older_than: Option<String>,
}

/// Filter tasks by tags (case-insensitive, OR-based).
///
/// Returns tasks where ANY of the task's tags match ANY of the filter tags (case-insensitive).
pub fn filter_tasks_by_tags<'a>(queue: &'a QueueFile, tags: &[String]) -> Vec<&'a Task> {
    if tags.is_empty() {
        return Vec::new();
    }

    let normalized_filter_tags: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    queue
        .tasks
        .iter()
        .filter(|task| {
            task.tags.iter().any(|task_tag| {
                let normalized_task_tag = task_tag.trim().to_lowercase();
                normalized_filter_tags
                    .iter()
                    .any(|filter_tag| filter_tag == &normalized_task_tag)
            })
        })
        .collect()
}

/// Parse an "older than" specification into an RFC3339 cutoff.
///
/// Supports:
/// - Duration expressions: "7d", "1w" (weeks), "30d" (days)
/// - Date: "2026-01-01"
/// - RFC3339: "2026-01-01T00:00:00Z"
pub fn parse_older_than_cutoff(now_rfc3339: &str, spec: &str) -> Result<String> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        bail!("Empty older_than specification");
    }

    let lower = trimmed.to_lowercase();

    // Try to parse as RFC3339 first
    if let Ok(dt) = crate::timeutil::parse_rfc3339(trimmed) {
        return crate::timeutil::format_rfc3339(dt);
    }

    // Try duration patterns like "7d", "1w"
    if let Some(days) = lower.strip_suffix('d') {
        let num_days: i64 = days
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid days in older_than: {}", spec))?;
        let now = crate::timeutil::parse_rfc3339(now_rfc3339)?;
        let cutoff = now - time::Duration::days(num_days);
        return crate::timeutil::format_rfc3339(cutoff);
    }

    if let Some(weeks) = lower.strip_suffix('w') {
        let num_weeks: i64 = weeks
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid weeks in older_than: {}", spec))?;
        let now = crate::timeutil::parse_rfc3339(now_rfc3339)?;
        let cutoff = now - time::Duration::weeks(num_weeks);
        return crate::timeutil::format_rfc3339(cutoff);
    }

    // Try date-only format "YYYY-MM-DD"
    if lower.len() == 10 && lower.contains('-') {
        let date_str = format!("{}T00:00:00Z", lower);
        if let Ok(dt) = crate::timeutil::parse_rfc3339(&date_str) {
            return crate::timeutil::format_rfc3339(dt);
        }
    }

    bail!(
        "Unable to parse older_than: '{}'. Supported formats: '7d', '1w', '2026-01-01', RFC3339",
        spec
    )
}

/// Resolve task IDs from explicit list or tag filter, then apply additional filters.
///
/// If `tag_filter` is provided, returns tasks matching any of the tags.
/// Otherwise, returns the explicit task IDs (after deduplication).
/// Then applies status, priority, scope, and older_than filters if provided.
///
/// # Arguments
/// * `queue` - The queue file to search
/// * `task_ids` - Explicit list of task IDs
/// * `tag_filter` - Optional list of tags to filter by
/// * `filters` - Additional filters to apply
/// * `now_rfc3339` - Current timestamp for age-based filtering
///
/// # Returns
/// A deduplicated list of task IDs to operate on.
pub fn resolve_task_ids_filtered(
    queue: &QueueFile,
    task_ids: &[String],
    tag_filter: &[String],
    filters: &BatchTaskFilters,
    now_rfc3339: &str,
) -> Result<Vec<String>> {
    use super::{collect_task_ids, deduplicate_task_ids};

    // Check if any selection criteria is provided
    let has_task_ids = !task_ids.is_empty();
    let has_tag_filter = !tag_filter.is_empty();
    let has_other_filters = !filters.status_filter.is_empty()
        || !filters.priority_filter.is_empty()
        || !filters.scope_filter.is_empty()
        || filters.older_than.is_some();

    if !has_task_ids && !has_tag_filter && !has_other_filters {
        bail!(
            "No tasks specified. Provide task IDs, use --tag-filter, or use other filters like --status-filter, --priority-filter, --scope-filter, or --older-than."
        );
    }

    // First resolve base IDs via existing logic
    let base_ids = if has_tag_filter {
        let matching_tasks = filter_tasks_by_tags(queue, tag_filter);
        if matching_tasks.is_empty() {
            let tags_str = tag_filter.join(", ");
            bail!("No tasks found with tags: {}", tags_str);
        }
        collect_task_ids(&matching_tasks)
    } else if has_task_ids {
        deduplicate_task_ids(task_ids)
    } else {
        // No tag filter and no explicit IDs - use all tasks from the queue
        // (other filters will be applied below)
        queue.tasks.iter().map(|t| t.id.clone()).collect()
    };

    // If no additional filters, return early
    if filters.status_filter.is_empty()
        && filters.priority_filter.is_empty()
        && filters.scope_filter.is_empty()
        && filters.older_than.is_none()
    {
        return Ok(base_ids);
    }

    // Apply additional filters
    let cutoff = filters
        .older_than
        .as_ref()
        .map(|spec| parse_older_than_cutoff(now_rfc3339, spec))
        .transpose()?;

    let normalized_scope_filters: Vec<String> = filters
        .scope_filter
        .iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let filtered: Vec<String> = base_ids
        .into_iter()
        .filter(|id| {
            let task = match queue.tasks.iter().find(|t| t.id == *id) {
                Some(t) => t,
                None => return false,
            };

            // Status filter (OR logic)
            if !filters.status_filter.is_empty() && !filters.status_filter.contains(&task.status) {
                return false;
            }

            // Priority filter (OR logic)
            if !filters.priority_filter.is_empty()
                && !filters.priority_filter.contains(&task.priority)
            {
                return false;
            }

            // Scope filter (OR logic, case-insensitive substring match)
            if !normalized_scope_filters.is_empty() {
                let matches_scope = task.scope.iter().any(|s| {
                    let s_lower = s.to_lowercase();
                    normalized_scope_filters.iter().any(|f| s_lower.contains(f))
                });
                if !matches_scope {
                    return false;
                }
            }

            // Age filter
            if let Some(ref cutoff_str) = cutoff
                && let Some(ref updated_at) = task.updated_at
                && let Ok(updated) = crate::timeutil::parse_rfc3339(updated_at)
                && let Ok(cutoff_dt) = crate::timeutil::parse_rfc3339(cutoff_str)
                && updated > cutoff_dt
            {
                return false;
            }

            true
        })
        .collect();

    Ok(filtered)
}

/// Resolve task IDs from either explicit list or tag filter (legacy, without additional filters).
///
/// If `tag_filter` is provided, returns tasks matching any of the tags.
/// Otherwise, returns the explicit task IDs (after deduplication).
///
/// # Arguments
/// * `queue` - The queue file to search
/// * `task_ids` - Explicit list of task IDs
/// * `tag_filter` - Optional list of tags to filter by
///
/// # Returns
/// A deduplicated list of task IDs to operate on.
pub fn resolve_task_ids(
    queue: &QueueFile,
    task_ids: &[String],
    tag_filter: &[String],
) -> Result<Vec<String>> {
    let filters = BatchTaskFilters::default();
    let now = crate::timeutil::now_utc_rfc3339_or_fallback();
    resolve_task_ids_filtered(queue, task_ids, tag_filter, &filters, &now)
}
