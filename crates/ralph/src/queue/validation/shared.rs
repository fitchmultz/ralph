//! Shared queue validation data structures.
//!
//! Purpose:
//! - Shared queue validation data structures.
//!
//! Responsibilities:
//! - Build stable task lookup structures shared across validators.
//! - Expose active-task and all-task views without repeating collection code.
//!
//! Not handled here:
//! - Validation policy or warning generation.
//! - Queue mutation or repair.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task IDs are trimmed before use as lookup keys.
//! - The active queue is the only source used for dependency-depth warnings.

use crate::contracts::{QueueFile, Task};
use std::collections::{HashMap, HashSet};

pub(crate) struct TaskCatalog<'a> {
    active_task_count: usize,
    pub(crate) tasks: Vec<&'a Task>,
    pub(crate) all_tasks: HashMap<&'a str, &'a Task>,
    pub(crate) all_task_ids: HashSet<&'a str>,
}

impl<'a> TaskCatalog<'a> {
    pub(crate) fn new(active: &'a QueueFile, done: Option<&'a QueueFile>) -> Self {
        let active_task_count = active.tasks.len();
        let done_task_count = done.map_or(0, |done_file| done_file.tasks.len());
        let mut tasks = Vec::with_capacity(active_task_count + done_task_count);
        tasks.extend(active.tasks.iter());
        if let Some(done_file) = done {
            tasks.extend(done_file.tasks.iter());
        }

        let mut all_tasks = HashMap::with_capacity(tasks.len());
        for task in &tasks {
            all_tasks.insert(task.id.trim(), *task);
        }

        let all_task_ids = all_tasks.keys().copied().collect();

        Self {
            active_task_count,
            tasks,
            all_tasks,
            all_task_ids,
        }
    }

    pub(crate) fn active_tasks(&self) -> &[&'a Task] {
        &self.tasks[..self.active_task_count]
    }
}
