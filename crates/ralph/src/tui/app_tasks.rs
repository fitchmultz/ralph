//! Task mutation operations for the TUI.
//!
//! Responsibilities:
//! - Task creation, deletion, and archiving.
//! - Task reordering (move up/down in queue).
//! - Auto-archive logic for terminal tasks.
//! - Task status and priority updates.
//!
//! Not handled here:
//! - Task navigation and selection (see app_navigation module).
//! - Filter management (see app_filters module).
//! - Queue persistence (see app_session module).
//!
//! Invariants/assumptions:
//! - All operations mark the queue as dirty when mutating.
//! - Queue revision is bumped after mutations to invalidate caches.
//! - Task IDs are generated using the queue's ID scheme.

use crate::contracts::{AutoArchiveBehavior, QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;
use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;

/// Result of a task move operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveResult {
    /// The ID of the task that was moved.
    pub task_id: String,
    /// The new position in the queue.
    pub new_position: usize,
}

/// Result of an archive operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResult {
    /// Number of tasks that were archived.
    pub archived_count: usize,
    /// IDs of the archived tasks.
    pub archived_ids: Vec<String>,
}

/// Trait for task operations.
///
/// This trait provides task mutation methods that work with any type
/// that has access to the queue, done archive, and navigation state.
pub trait AppTasks {
    /// Move the selected task up in the queue.
    ///
    /// Swaps the selected task with the one above it in the filtered view.
    fn move_task_up(
        &mut self,
        now_rfc3339: &str,
        filtered_indices: &[usize],
        selected: usize,
    ) -> Result<MoveResult>;

    /// Move the selected task down in the queue.
    ///
    /// Swaps the selected task with the one below it in the filtered view.
    fn move_task_down(
        &mut self,
        now_rfc3339: &str,
        filtered_indices: &[usize],
        selected: usize,
    ) -> Result<MoveResult>;

    /// Delete a task by its queue index.
    ///
    /// Returns the deleted task. The caller should handle updating
    /// selection and scroll position.
    fn delete_task_by_index(&mut self, index: usize) -> Result<Task>;

    /// Create a new task with the given title.
    ///
    /// Generates a new task ID and adds the task to the queue.
    fn create_task_from_title(
        &mut self,
        title: &str,
        now_rfc3339: &str,
        id_prefix: &str,
        id_width: usize,
        max_dependency_depth: u8,
    ) -> Result<Task>;

    /// Archive all terminal tasks (Done/Rejected) into the done queue.
    fn archive_terminal_tasks(&mut self, now_rfc3339: &str) -> Result<ArchiveResult>;

    /// Archive a single terminal task by ID.
    fn archive_single_task(&mut self, task_id: &str, now_rfc3339: &str) -> Result<()>;

    /// Check if auto-archive should be triggered and handle it.
    fn maybe_auto_archive(
        &mut self,
        task_id: &str,
        now_rfc3339: &str,
        auto_archive_behavior: AutoArchiveBehavior,
    ) -> Result<AutoArchiveAction>;

    /// Set the status of a task.
    fn set_task_status(
        &mut self,
        task_id: &str,
        status: TaskStatus,
        now_rfc3339: &str,
    ) -> Result<()>;

    /// Set the priority of a task.
    fn set_task_priority(
        &mut self,
        task_id: &str,
        priority: TaskPriority,
        now_rfc3339: &str,
    ) -> Result<()>;

    /// Cycle the status of a task to the next value.
    fn cycle_status(&mut self, task_id: &str, now_rfc3339: &str) -> Result<TaskStatus>;

    /// Cycle the priority of a task to the next value.
    fn cycle_priority(&mut self, task_id: &str, now_rfc3339: &str) -> Result<TaskPriority>;

    /// Delete multiple tasks by their filtered indices.
    ///
    /// # Arguments
    /// * `indices` - Slice of filtered indices to delete
    ///
    /// # Returns
    /// * `Result<usize>` - Number of tasks actually deleted
    ///
    /// # Invariants
    /// - Deletes in reverse order to maintain index validity
    /// - Marks queue as dirty after deletions
    /// - Clears selection after successful deletion
    fn batch_delete_tasks(&mut self, indices: &[usize]) -> Result<usize>;

    /// Archive multiple tasks by their filtered indices.
    ///
    /// # Arguments
    /// * `indices` - Slice of filtered indices to archive
    /// * `now_rfc3339` - Current timestamp for archive metadata
    ///
    /// # Returns
    /// * `Result<usize>` - Number of tasks actually archived
    ///
    /// # Invariants
    /// - Archives in reverse order to maintain index validity
    /// - Marks both queue and done as dirty after operations
    /// - Clears selection after successful archive
    fn batch_archive_tasks(&mut self, indices: &[usize], now_rfc3339: &str) -> Result<usize>;

    /// Set status on multiple tasks by their filtered indices.
    ///
    /// # Arguments
    /// * `indices` - Slice of filtered indices to update
    /// * `status` - The new status to set
    /// * `now_rfc3339` - Current timestamp for updated_at
    ///
    /// # Returns
    /// * `Result<usize>` - Number of tasks actually updated
    fn batch_set_status(
        &mut self,
        indices: &[usize],
        status: TaskStatus,
        now_rfc3339: &str,
    ) -> Result<usize>;
}

/// Action to take for auto-archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoArchiveAction {
    /// No action needed.
    None,
    /// Task was archived automatically.
    Archived,
    /// Prompt the user for confirmation.
    Prompt,
}

/// Task operations implementation.
///
/// This struct encapsulates task mutation operations and can be composed
/// into the main App struct.
pub struct TaskOperations {
    /// The active task queue.
    pub queue: QueueFile,
    /// The done archive queue.
    pub done: QueueFile,
    /// Whether the queue has unsaved changes.
    pub dirty: bool,
    /// Whether the done archive has unsaved changes.
    pub dirty_done: bool,
    /// Queue revision counter (for cache invalidation).
    pub queue_rev: u64,
}

impl TaskOperations {
    /// Create a new TaskOperations with the given queues.
    pub fn new(queue: QueueFile, done: QueueFile) -> Self {
        Self {
            queue,
            done,
            dirty: false,
            dirty_done: false,
            queue_rev: 0,
        }
    }

    /// Bump the queue revision to invalidate caches.
    pub fn bump_queue_rev(&mut self) {
        self.queue_rev = self.queue_rev.wrapping_add(1);
    }

    /// Get the current queue revision.
    pub fn queue_rev(&self) -> u64 {
        self.queue_rev
    }

    /// Move the task at the given filtered position up.
    pub fn move_task_up(
        &mut self,
        now_rfc3339: &str,
        filtered_indices: &[usize],
        selected: usize,
    ) -> Result<MoveResult> {
        if selected == 0 || filtered_indices.is_empty() {
            return Err(anyhow!("Cannot move task up"));
        }

        let current_idx = filtered_indices[selected];
        let prev_idx = filtered_indices[selected - 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[prev_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, prev_idx);
        self.dirty = true;
        self.bump_queue_rev();

        let task_id = self.queue.tasks[prev_idx].id.clone();

        Ok(MoveResult {
            task_id,
            new_position: selected - 1,
        })
    }

    /// Move the task at the given filtered position down.
    pub fn move_task_down(
        &mut self,
        now_rfc3339: &str,
        filtered_indices: &[usize],
        selected: usize,
    ) -> Result<MoveResult> {
        if selected + 1 >= filtered_indices.len() || filtered_indices.is_empty() {
            return Err(anyhow!("Cannot move task down"));
        }

        let current_idx = filtered_indices[selected];
        let next_idx = filtered_indices[selected + 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[next_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, next_idx);
        self.dirty = true;
        self.bump_queue_rev();

        let task_id = self.queue.tasks[next_idx].id.clone();

        Ok(MoveResult {
            task_id,
            new_position: selected + 1,
        })
    }

    /// Delete a task by its queue index.
    pub fn delete_task_by_index(&mut self, index: usize) -> Result<Task> {
        if index >= self.queue.tasks.len() {
            bail!("Task index out of bounds");
        }

        let task = self.queue.tasks.remove(index);
        self.dirty = true;
        self.bump_queue_rev();

        Ok(task)
    }

    /// Create a new task with the given title.
    #[allow(clippy::too_many_arguments)]
    pub fn create_task_from_title(
        &mut self,
        title: &str,
        now_rfc3339: &str,
        id_prefix: &str,
        id_width: usize,
        max_dependency_depth: u8,
    ) -> Result<Task> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            bail!("Task title cannot be empty");
        }

        let next_id = queue::next_id_across(
            &self.queue,
            Some(&self.done),
            id_prefix,
            id_width,
            max_dependency_depth,
        )?;

        let task = Task {
            id: next_id,
            title: trimmed.to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some(now_rfc3339.to_string()),
            updated_at: Some(now_rfc3339.to_string()),
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        };

        self.queue.tasks.push(task.clone());
        self.dirty = true;
        self.bump_queue_rev();

        Ok(task)
    }

    /// Archive all terminal tasks (Done/Rejected) into the done queue.
    pub fn archive_terminal_tasks(&mut self, now_rfc3339: &str) -> Result<ArchiveResult> {
        let report =
            queue::archive_terminal_tasks_in_memory(&mut self.queue, &mut self.done, now_rfc3339)?;

        if report.moved_ids.is_empty() {
            return Ok(ArchiveResult {
                archived_count: 0,
                archived_ids: vec![],
            });
        }

        let archived_count = report.moved_ids.len();
        let archived_ids = report.moved_ids.clone();

        self.dirty = true;
        self.dirty_done = true;
        self.bump_queue_rev();

        Ok(ArchiveResult {
            archived_count,
            archived_ids,
        })
    }

    /// Archive a single terminal task by ID.
    pub fn archive_single_task(&mut self, task_id: &str, now_rfc3339: &str) -> Result<()> {
        let idx = self
            .queue
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        let task = &self.queue.tasks[idx];
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            return Err(anyhow!(
                "Task {} is not in a terminal status (Done/Rejected)",
                task_id
            ));
        }

        let mut task = self.queue.tasks.remove(idx);
        task.updated_at = Some(now_rfc3339.to_string());
        self.done.tasks.push(task);

        self.dirty = true;
        self.dirty_done = true;
        self.bump_queue_rev();

        Ok(())
    }

    /// Check if auto-archive should be triggered and handle it.
    pub fn maybe_auto_archive(
        &mut self,
        task_id: &str,
        now_rfc3339: &str,
        behavior: AutoArchiveBehavior,
    ) -> Result<AutoArchiveAction> {
        match behavior {
            AutoArchiveBehavior::Never => Ok(AutoArchiveAction::None),
            AutoArchiveBehavior::Always => {
                self.archive_single_task(task_id, now_rfc3339)?;
                Ok(AutoArchiveAction::Archived)
            }
            AutoArchiveBehavior::Prompt => Ok(AutoArchiveAction::Prompt),
        }
    }

    /// Set the status of a task.
    pub fn set_task_status(
        &mut self,
        task_id: &str,
        status: TaskStatus,
        now_rfc3339: &str,
    ) -> Result<()> {
        let task = self
            .queue
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        task.status = status;
        task.updated_at = Some(now_rfc3339.to_string());
        self.dirty = true;
        self.bump_queue_rev();

        Ok(())
    }

    /// Set the priority of a task.
    pub fn set_task_priority(
        &mut self,
        task_id: &str,
        priority: TaskPriority,
        now_rfc3339: &str,
    ) -> Result<()> {
        let task = self
            .queue
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        task.priority = priority;
        task.updated_at = Some(now_rfc3339.to_string());
        self.dirty = true;
        self.bump_queue_rev();

        Ok(())
    }

    /// Cycle the status of a task to the next value.
    pub fn cycle_status(&mut self, task_id: &str, now_rfc3339: &str) -> Result<TaskStatus> {
        let task = self
            .queue
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        let next_status = match task.status {
            TaskStatus::Draft => TaskStatus::Todo,
            TaskStatus::Todo => TaskStatus::Doing,
            TaskStatus::Doing => TaskStatus::Done,
            TaskStatus::Done => TaskStatus::Rejected,
            TaskStatus::Rejected => TaskStatus::Draft,
        };

        self.set_task_status(task_id, next_status, now_rfc3339)?;
        Ok(next_status)
    }

    /// Cycle the priority of a task to the next value.
    pub fn cycle_priority(&mut self, task_id: &str, now_rfc3339: &str) -> Result<TaskPriority> {
        let task = self
            .queue
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        let next_priority = match task.priority {
            TaskPriority::Low => TaskPriority::Medium,
            TaskPriority::Medium => TaskPriority::High,
            TaskPriority::High => TaskPriority::Critical,
            TaskPriority::Critical => TaskPriority::Low,
        };

        self.set_task_priority(task_id, next_priority, now_rfc3339)?;
        Ok(next_priority)
    }

    /// Delete multiple tasks by their filtered indices.
    ///
    /// Deletes in reverse order to maintain index validity.
    pub fn batch_delete_tasks(&mut self, indices: &[usize]) -> Result<usize> {
        if indices.is_empty() {
            return Ok(0);
        }

        // Sort indices in descending order to delete from end first
        let mut sorted_indices: Vec<usize> = indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
        sorted_indices.dedup();

        let mut deleted_count = 0;
        for &idx in &sorted_indices {
            if idx < self.queue.tasks.len() {
                self.queue.tasks.remove(idx);
                deleted_count += 1;
            }
        }

        if deleted_count > 0 {
            self.dirty = true;
            self.bump_queue_rev();
        }

        Ok(deleted_count)
    }

    /// Archive multiple tasks by their filtered indices.
    ///
    /// Archives in reverse order to maintain index validity.
    pub fn batch_archive_tasks(&mut self, indices: &[usize], now_rfc3339: &str) -> Result<usize> {
        if indices.is_empty() {
            return Ok(0);
        }

        // Sort indices in descending order to archive from end first
        let mut sorted_indices: Vec<usize> = indices.to_vec();
        sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
        sorted_indices.dedup();

        let mut archived_count = 0;
        for &idx in &sorted_indices {
            if idx < self.queue.tasks.len() {
                let mut task = self.queue.tasks.remove(idx);
                task.updated_at = Some(now_rfc3339.to_string());
                self.done.tasks.push(task);
                archived_count += 1;
            }
        }

        if archived_count > 0 {
            self.dirty = true;
            self.dirty_done = true;
            self.bump_queue_rev();
        }

        Ok(archived_count)
    }

    /// Set status on multiple tasks by their filtered indices.
    pub fn batch_set_status(
        &mut self,
        indices: &[usize],
        status: TaskStatus,
        now_rfc3339: &str,
    ) -> Result<usize> {
        if indices.is_empty() {
            return Ok(0);
        }

        let mut updated_count = 0;
        for &idx in indices {
            if let Some(task) = self.queue.tasks.get_mut(idx) {
                task.status = status;
                task.updated_at = Some(now_rfc3339.to_string());
                updated_count += 1;
            }
        }

        if updated_count > 0 {
            self.dirty = true;
            self.bump_queue_rev();
        }

        Ok(updated_count)
    }
}

// ============================================================================
// TaskMovementOperations trait for App
// ============================================================================

use crate::tui::App;

/// Trait for task movement operations.
pub trait TaskMovementOperations {
    /// Move the selected task up in the queue.
    fn move_task_up(&mut self, now_rfc3339: &str) -> anyhow::Result<()>;

    /// Move the selected task down in the queue.
    fn move_task_down(&mut self, now_rfc3339: &str) -> anyhow::Result<()>;
}

impl TaskMovementOperations for App {
    fn move_task_up(&mut self, now_rfc3339: &str) -> anyhow::Result<()> {
        if self.selected == 0 || self.filtered_indices.is_empty() {
            return Ok(());
        }

        let current_idx = self.filtered_indices[self.selected];
        let prev_idx = self.filtered_indices[self.selected - 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[prev_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, prev_idx);
        self.dirty = true;
        self.bump_queue_rev();

        let task_id = self.queue.tasks[prev_idx].id.clone();
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        self.set_status_message(format!("Moved {} up", task_id));

        Ok(())
    }

    fn move_task_down(&mut self, now_rfc3339: &str) -> anyhow::Result<()> {
        if self.selected + 1 >= self.filtered_indices.len() || self.filtered_indices.is_empty() {
            return Ok(());
        }

        let current_idx = self.filtered_indices[self.selected];
        let next_idx = self.filtered_indices[self.selected + 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[next_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, next_idx);
        self.dirty = true;
        self.bump_queue_rev();

        let task_id = self.queue.tasks[next_idx].id.clone();
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        self.set_status_message(format!("Moved {} down", task_id));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            status,
            ..Default::default()
        }
    }

    fn create_test_queues() -> (QueueFile, QueueFile) {
        let queue = QueueFile {
            tasks: vec![
                create_task("RQ-0001", TaskStatus::Todo),
                create_task("RQ-0002", TaskStatus::Doing),
                create_task("RQ-0003", TaskStatus::Done),
                create_task("RQ-0004", TaskStatus::Rejected),
            ],
            ..Default::default()
        };
        let done = QueueFile::default();
        (queue, done)
    }

    #[test]
    fn test_move_task_up() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);
        let filtered_indices = vec![0, 1, 2, 3];

        let result = ops
            .move_task_up("2024-01-01T00:00:00Z", &filtered_indices, 2)
            .unwrap();

        assert_eq!(result.task_id, "RQ-0003");
        assert_eq!(result.new_position, 1);
        assert_eq!(ops.queue.tasks[1].id, "RQ-0003");
        assert_eq!(ops.queue.tasks[2].id, "RQ-0002");
        assert!(ops.dirty);
    }

    #[test]
    fn test_move_task_up_at_top() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);
        let filtered_indices = vec![0, 1, 2, 3];

        let result = ops.move_task_up("2024-01-01T00:00:00Z", &filtered_indices, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_move_task_down() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);
        let filtered_indices = vec![0, 1, 2, 3];

        let result = ops
            .move_task_down("2024-01-01T00:00:00Z", &filtered_indices, 1)
            .unwrap();

        assert_eq!(result.task_id, "RQ-0002");
        assert_eq!(result.new_position, 2);
        assert_eq!(ops.queue.tasks[1].id, "RQ-0003");
        assert_eq!(ops.queue.tasks[2].id, "RQ-0002");
        assert!(ops.dirty);
    }

    #[test]
    fn test_delete_task() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let deleted = ops.delete_task_by_index(1).unwrap();

        assert_eq!(deleted.id, "RQ-0002");
        assert_eq!(ops.queue.tasks.len(), 3);
        assert!(ops.dirty);
    }

    #[test]
    fn test_archive_terminal_tasks() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let result = ops.archive_terminal_tasks("2024-01-01T00:00:00Z").unwrap();

        assert_eq!(result.archived_count, 2);
        assert_eq!(result.archived_ids, vec!["RQ-0003", "RQ-0004"]);
        assert_eq!(ops.queue.tasks.len(), 2);
        assert_eq!(ops.done.tasks.len(), 2);
        assert!(ops.dirty);
        assert!(ops.dirty_done);
    }

    #[test]
    fn test_set_task_status() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        ops.set_task_status("RQ-0001", TaskStatus::Doing, "2024-01-01T00:00:00Z")
            .unwrap();

        assert_eq!(ops.queue.tasks[0].status, TaskStatus::Doing);
        assert_eq!(
            ops.queue.tasks[0].updated_at,
            Some("2024-01-01T00:00:00Z".to_string())
        );
        assert!(ops.dirty);
    }

    #[test]
    fn test_cycle_status() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let new_status = ops.cycle_status("RQ-0001", "2024-01-01T00:00:00Z").unwrap();

        assert_eq!(new_status, TaskStatus::Doing);
        assert_eq!(ops.queue.tasks[0].status, TaskStatus::Doing);
    }

    #[test]
    fn test_cycle_priority() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let new_priority = ops
            .cycle_priority("RQ-0001", "2024-01-01T00:00:00Z")
            .unwrap();

        assert_eq!(new_priority, TaskPriority::High);
        assert_eq!(ops.queue.tasks[0].priority, TaskPriority::High);
    }

    #[test]
    fn test_maybe_auto_archive_never() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let action = ops
            .maybe_auto_archive(
                "RQ-0003",
                "2024-01-01T00:00:00Z",
                AutoArchiveBehavior::Never,
            )
            .unwrap();

        assert_eq!(action, AutoArchiveAction::None);
        assert_eq!(ops.queue.tasks.len(), 4);
    }

    #[test]
    fn test_maybe_auto_archive_always() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let action = ops
            .maybe_auto_archive(
                "RQ-0003",
                "2024-01-01T00:00:00Z",
                AutoArchiveBehavior::Always,
            )
            .unwrap();

        assert_eq!(action, AutoArchiveAction::Archived);
        assert_eq!(ops.queue.tasks.len(), 3);
        assert_eq!(ops.done.tasks.len(), 1);
    }

    #[test]
    fn test_maybe_auto_archive_prompt() {
        let (queue, done) = create_test_queues();
        let mut ops = TaskOperations::new(queue, done);

        let action = ops
            .maybe_auto_archive(
                "RQ-0003",
                "2024-01-01T00:00:00Z",
                AutoArchiveBehavior::Prompt,
            )
            .unwrap();

        assert_eq!(action, AutoArchiveAction::Prompt);
        assert_eq!(ops.queue.tasks.len(), 4); // Not moved yet
    }
}
