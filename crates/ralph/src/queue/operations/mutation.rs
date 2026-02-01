//! Collection/mutation helpers for queue tasks.

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{Result, anyhow};
use std::collections::HashSet;

/// Suggests the insertion index for new tasks based on the first task's status.
///
/// Returns `1` if the first task has status `Doing` (insert after the in-progress task),
/// otherwise returns `0` (insert at top of the queue). Returns `0` for empty queues.
pub fn suggest_new_task_insert_index(queue: &QueueFile) -> usize {
    match queue.tasks.first() {
        Some(first_task) if matches!(first_task.status, TaskStatus::Doing) => 1,
        _ => 0,
    }
}

/// Repositions newly added tasks to the specified insertion index in the queue.
///
/// This function extracts tasks identified by `new_task_ids` from their current positions
/// and splices them into the queue at `insert_at`, preserving the relative order of
/// existing tasks and the new tasks themselves.
///
/// The `insert_at` index is clamped to `queue.tasks.len()` to prevent out-of-bounds errors.
pub fn reposition_new_tasks(queue: &mut QueueFile, new_task_ids: &[String], insert_at: usize) {
    if new_task_ids.is_empty() || queue.tasks.is_empty() {
        return;
    }

    let insert_at = insert_at.min(queue.tasks.len());
    let new_task_set: HashSet<String> = new_task_ids.iter().cloned().collect();

    let mut new_tasks = Vec::new();
    let mut retained_tasks = Vec::new();

    for task in queue.tasks.drain(..) {
        if new_task_set.contains(&task.id) {
            new_tasks.push(task);
        } else {
            retained_tasks.push(task);
        }
    }

    // Splice new tasks at the calculated insertion point
    let split_index = insert_at.min(retained_tasks.len());
    let mut before_split = Vec::new();
    let mut after_split = retained_tasks;
    for task in after_split.drain(..split_index) {
        before_split.push(task);
    }

    queue.tasks = before_split
        .into_iter()
        .chain(new_tasks)
        .chain(after_split)
        .collect();
}

pub fn added_tasks(before: &HashSet<String>, after: &QueueFile) -> Vec<(String, String)> {
    let mut added = Vec::new();
    for task in &after.tasks {
        let id = task.id.trim();
        if id.is_empty() || before.contains(id) {
            continue;
        }
        added.push((id.to_string(), task.title.trim().to_string()));
    }
    added
}

pub fn backfill_missing_fields(
    queue: &mut QueueFile,
    new_task_ids: &[String],
    default_request: &str,
    now_utc: &str,
) {
    let now = now_utc.trim();
    if now.is_empty() || new_task_ids.is_empty() || queue.tasks.is_empty() {
        return;
    }

    let new_task_set: HashSet<&str> = new_task_ids.iter().map(|id| id.as_str()).collect();
    for task in queue.tasks.iter_mut() {
        if !new_task_set.contains(task.id.trim()) {
            continue;
        }

        if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
            let req = default_request.trim();
            if !req.is_empty() {
                task.request = Some(req.to_string());
            }
        }

        if task.created_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.created_at = Some(now.to_string());
        }

        if task.updated_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.updated_at = Some(now.to_string());
        }
    }
}

/// Ensure terminal tasks have a completed_at timestamp.
///
/// Returns the number of tasks updated.
pub fn backfill_terminal_completed_at(queue: &mut QueueFile, now_utc: &str) -> usize {
    let now = now_utc.trim();
    if now.is_empty() {
        return 0;
    }

    let mut updated = 0;
    for task in queue.tasks.iter_mut() {
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            continue;
        }

        if task
            .completed_at
            .as_ref()
            .is_none_or(|t| t.trim().is_empty())
        {
            task.completed_at = Some(now.to_string());
            updated += 1;
        }
    }

    updated
}

pub fn sort_tasks_by_priority(queue: &mut QueueFile, descending: bool) {
    queue.tasks.sort_by(|a, b| {
        let ord = if descending {
            a.priority.cmp(&b.priority).reverse()
        } else {
            a.priority.cmp(&b.priority)
        };
        match ord {
            std::cmp::Ordering::Equal => a.id.cmp(&b.id),
            other => other,
        }
    });
}

pub fn task_id_set(queue: &QueueFile) -> HashSet<String> {
    let mut set = HashSet::new();
    for task in &queue.tasks {
        let id = task.id.trim();
        if id.is_empty() {
            continue;
        }
        set.insert(id.to_string());
    }
    set
}

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
                "Source task '{}' not found in queue or done archive",
                opts.source_id
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
                "Source task '{}' not found in active queue (cannot split archived tasks)",
                opts.source_id
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

    for i in 0..opts.number {
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
            child.plan = plan_distribution[i].clone();
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

/// Distribute plan items evenly across child tasks using round-robin.
fn distribute_plan_items(plan: &[String], num_children: usize) -> Vec<Vec<String>> {
    let mut distribution: Vec<Vec<String>> = vec![Vec::new(); num_children];

    for (i, item) in plan.iter().enumerate() {
        distribution[i % num_children].push(item.clone());
    }

    distribution
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
