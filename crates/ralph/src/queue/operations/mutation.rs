//! Queue mutation workflows for cloning, splitting, and shared task reshaping.
//!
//! Purpose:
//! - Queue mutation workflows for cloning, splitting, and shared task reshaping.
//!
//! Responsibilities:
//! - Clone existing tasks into new queue entries with fresh IDs and timestamps.
//! - Split a parent task into rejected-source plus child-task outputs.
//! - Re-export shared queue mutation helpers from `mutation/helpers.rs`.
//!
//! Non-scope:
//! - Queue persistence, locking, or repair flows.
//! - Batch orchestration around clone/split operations.
//! - Schema validation beyond targeted queue-set checks before clone/split.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Caller-provided timestamps are already normalized RFC3339 UTC strings.
//! - Queue and done IDs remain unique across the validated queue set.
//! - Split children always clear dependency edges inherited from the source task.

mod helpers;

pub use helpers::{
    added_tasks, backfill_missing_fields, backfill_terminal_completed_at, reposition_new_tasks,
    sort_tasks_by_priority, suggest_new_task_insert_index, task_id_set,
};

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{Result, anyhow};
use helpers::distribute_plan_items;

/// Options for cloning a task.
#[derive(Debug, Clone)]
pub struct CloneTaskOptions<'a> {
    /// ID of the task to clone.
    pub source_id: &'a str,
    /// Status for the cloned task.
    pub status: TaskStatus,
    /// Optional prefix to prepend to the cloned task's title.
    pub title_prefix: Option<&'a str>,
    /// Current timestamp (RFC3339) for created_at/updated_at.
    pub now_utc: &'a str,
    /// Prefix for new task ID (e.g., "RQ").
    pub id_prefix: &'a str,
    /// Width of the numeric portion of the ID.
    pub id_width: usize,
    /// Max dependency depth for validation.
    pub max_depth: u8,
}

impl<'a> CloneTaskOptions<'a> {
    /// Create new clone options with required fields.
    pub fn new(
        source_id: &'a str,
        status: TaskStatus,
        now_utc: &'a str,
        id_prefix: &'a str,
        id_width: usize,
    ) -> Self {
        Self {
            source_id,
            status,
            title_prefix: None,
            now_utc,
            id_prefix,
            id_width,
            max_depth: 10,
        }
    }

    /// Set the title prefix.
    pub fn with_title_prefix(mut self, prefix: Option<&'a str>) -> Self {
        self.title_prefix = prefix;
        self
    }

    /// Set the max dependency depth.
    pub fn with_max_depth(mut self, depth: u8) -> Self {
        self.max_depth = depth;
        self
    }
}

/// Clone an existing task to create a new task with the same fields.
///
/// The cloned task will have:
/// - A new task ID (generated using the provided prefix/width)
/// - Fresh timestamps (created_at, updated_at = now)
/// - Cleared completed_at
/// - Status set to the provided value (default: Draft)
/// - Cleared depends_on (to avoid unintended dependencies)
/// - Optional title prefix applied
///
/// # Arguments
/// * `queue` - The active queue to insert the cloned task into
/// * `done` - Optional done queue to search for source task
/// * `opts` - Clone options (source_id, status, title_prefix, now_utc, id_prefix, id_width, max_depth)
///
/// # Returns
/// A tuple of (new_task_id, cloned_task)
pub fn clone_task(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    opts: &CloneTaskOptions<'_>,
) -> Result<(String, Task)> {
    use crate::queue::{next_id_across, validation::validate_queue_set};

    // Validate queues first
    let warnings = validate_queue_set(queue, done, opts.id_prefix, opts.id_width, opts.max_depth)?;
    if !warnings.is_empty() {
        for warning in &warnings {
            log::warn!("Queue validation warning: {}", warning.message);
        }
    }

    // Find source task in queue or done
    let source_task = queue
        .tasks
        .iter()
        .find(|t| t.id.trim() == opts.source_id.trim())
        .or_else(|| {
            done.and_then(|d| {
                d.tasks
                    .iter()
                    .find(|t| t.id.trim() == opts.source_id.trim())
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "{}",
                crate::error_messages::source_task_not_found(opts.source_id, true)
            )
        })?;

    // Generate new task ID
    let new_id = next_id_across(queue, done, opts.id_prefix, opts.id_width, opts.max_depth)?;

    // Clone the task
    let mut cloned = source_task.clone();
    cloned.id = new_id.clone();

    // Apply title prefix if provided
    if let Some(prefix) = opts.title_prefix
        && !prefix.is_empty()
    {
        cloned.title = format!("{}{}", prefix, cloned.title);
    }

    // Set status
    cloned.status = opts.status;

    // Set fresh timestamps
    cloned.created_at = Some(opts.now_utc.to_string());
    cloned.updated_at = Some(opts.now_utc.to_string());
    cloned.completed_at = None;

    // Clear dependencies to avoid unintended dependencies
    cloned.depends_on.clear();

    Ok((new_id, cloned))
}

/// Options for splitting a task.
#[derive(Debug, Clone)]
pub struct SplitTaskOptions<'a> {
    /// ID of the task to split.
    pub source_id: &'a str,
    /// Number of child tasks to create.
    pub number: usize,
    /// Status for child tasks.
    pub status: TaskStatus,
    /// Optional prefix to prepend to child task titles.
    pub title_prefix: Option<&'a str>,
    /// Distribute plan items across children.
    pub distribute_plan: bool,
    /// Current timestamp (RFC3339) for created_at/updated_at.
    pub now_utc: &'a str,
    /// Prefix for new task ID (e.g., "RQ").
    pub id_prefix: &'a str,
    /// Width of the numeric portion of the ID.
    pub id_width: usize,
    /// Max dependency depth for validation.
    pub max_depth: u8,
}

impl<'a> SplitTaskOptions<'a> {
    /// Create new split options with required fields.
    pub fn new(
        source_id: &'a str,
        number: usize,
        status: TaskStatus,
        now_utc: &'a str,
        id_prefix: &'a str,
        id_width: usize,
    ) -> Self {
        Self {
            source_id,
            number,
            status,
            title_prefix: None,
            distribute_plan: false,
            now_utc,
            id_prefix,
            id_width,
            max_depth: 10,
        }
    }

    /// Set the title prefix.
    pub fn with_title_prefix(mut self, prefix: Option<&'a str>) -> Self {
        self.title_prefix = prefix;
        self
    }

    /// Set distribute plan flag.
    pub fn with_distribute_plan(mut self, distribute: bool) -> Self {
        self.distribute_plan = distribute;
        self
    }

    /// Set the max dependency depth.
    pub fn with_max_depth(mut self, depth: u8) -> Self {
        self.max_depth = depth;
        self
    }
}

/// Split an existing task into multiple child tasks.
pub fn split_task(
    queue: &mut QueueFile,
    _done: Option<&QueueFile>,
    opts: &SplitTaskOptions<'_>,
) -> Result<(Task, Vec<Task>)> {
    use crate::queue::{next_id_across, validation::validate_queue_set};

    // Validate queues first
    let warnings = validate_queue_set(queue, _done, opts.id_prefix, opts.id_width, opts.max_depth)?;
    if !warnings.is_empty() {
        for warning in &warnings {
            log::warn!("Queue validation warning: {}", warning.message);
        }
    }

    // Find source task in queue only (splitting from done archive doesn't make sense)
    let source_index = queue
        .tasks
        .iter()
        .position(|t| t.id.trim() == opts.source_id.trim())
        .ok_or_else(|| {
            anyhow!(
                "{}",
                crate::error_messages::source_task_not_found(opts.source_id, false)
            )
        })?;

    let source_task = &queue.tasks[source_index];

    // Mark source task as split
    let mut updated_source = source_task.clone();
    updated_source
        .custom_fields
        .insert("split".to_string(), "true".to_string());
    updated_source.status = TaskStatus::Rejected;
    updated_source.updated_at = Some(opts.now_utc.to_string());
    if updated_source.notes.is_empty() {
        updated_source.notes = vec![format!("Task split into {} child tasks", opts.number)];
    } else {
        updated_source
            .notes
            .push(format!("Task split into {} child tasks", opts.number));
    }

    // Generate child tasks
    let mut child_tasks = Vec::with_capacity(opts.number);
    let mut next_id = next_id_across(queue, _done, opts.id_prefix, opts.id_width, opts.max_depth)?;

    // Distribute plan items if requested
    let plan_distribution = if opts.distribute_plan && !source_task.plan.is_empty() {
        distribute_plan_items(&source_task.plan, opts.number)
    } else {
        vec![Vec::new(); opts.number]
    };

    for (i, plan_items) in plan_distribution.iter().enumerate().take(opts.number) {
        let mut child = source_task.clone();
        child.id = next_id.clone();
        child.parent_id = Some(opts.source_id.to_string());

        // Build title with optional prefix and index
        let title_suffix = format!(" ({}/{})", i + 1, opts.number);
        if let Some(prefix) = opts.title_prefix {
            child.title = format!("{}{}{}", prefix, source_task.title, title_suffix);
        } else {
            child.title = format!("{}{}", source_task.title, title_suffix);
        }

        // Set status and timestamps
        child.status = opts.status;
        child.created_at = Some(opts.now_utc.to_string());
        child.updated_at = Some(opts.now_utc.to_string());
        child.completed_at = None;

        // Clear dependencies for children
        child.depends_on.clear();
        child.blocks.clear();
        child.relates_to.clear();
        child.duplicates = None;

        // Distribute plan items
        if opts.distribute_plan {
            child.plan = plan_items.clone();
        } else {
            child.plan.clear();
        }

        // Add note about being a child task
        child.notes = vec![format!(
            "Child task {} of {} from parent {}",
            i + 1,
            opts.number,
            opts.source_id
        )];

        child_tasks.push(child);

        // Generate next ID for the next child (simulate insertion)
        let numeric_part = next_id
            .strip_prefix(opts.id_prefix)
            .and_then(|s| s.strip_prefix('-'))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        next_id = format!(
            "{}-{:0>width$}",
            opts.id_prefix,
            numeric_part + 1,
            width = opts.id_width
        );
    }

    Ok((updated_source, child_tasks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribute_plan_items_distributes_evenly() {
        let plan = vec![
            "Step A".to_string(),
            "Step B".to_string(),
            "Step C".to_string(),
            "Step D".to_string(),
        ];

        let distributed = distribute_plan_items(&plan, 2);
        assert_eq!(distributed.len(), 2);
        assert_eq!(distributed[0], vec!["Step A", "Step C"]);
        assert_eq!(distributed[1], vec!["Step B", "Step D"]);
    }

    #[test]
    fn distribute_plan_items_handles_uneven() {
        let plan = vec![
            "Step A".to_string(),
            "Step B".to_string(),
            "Step C".to_string(),
        ];

        let distributed = distribute_plan_items(&plan, 2);
        assert_eq!(distributed.len(), 2);
        assert_eq!(distributed[0], vec!["Step A", "Step C"]);
        assert_eq!(distributed[1], vec!["Step B"]);
    }

    #[test]
    fn distribute_plan_items_handles_empty() {
        let plan: Vec<String> = vec![];
        let distributed = distribute_plan_items(&plan, 2);
        assert_eq!(distributed.len(), 2);
        assert!(distributed[0].is_empty());
        assert!(distributed[1].is_empty());
    }
}
