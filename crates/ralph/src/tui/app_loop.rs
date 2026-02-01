//! Loop mode state management for the TUI.
//!
//! Responsibilities:
//! - Track loop mode activation state and configuration.
//! - Count tasks completed in the current loop session.
//! - Manage loop arming (waiting for current task to finish before starting loop).
//! - Determine the next runnable task for loop execution.
//!
//! Not handled here:
//! - Actual task execution (handled by execution state module).
//! - Task queue mutation (handled by queue module).
//! - Runner lifecycle management (handled by app module).
//!
//! Invariants/assumptions:
//! - Loop mode can be armed while a task is running, starting after completion.
//! - Loop stops when no more runnable tasks are available or max_tasks is reached.
//! - Draft tasks are only eligible when `include_draft` is true.

#![allow(dead_code)]

use crate::contracts::QueueFile;
use crate::queue;

/// State for loop mode execution.
#[derive(Debug, Default)]
pub struct LoopState {
    /// Whether loop mode is active.
    pub active: bool,
    /// When loop is enabled while a task is already running, do not count that finishing task.
    pub arm_after_current: bool,
    /// Count of tasks successfully completed in the current loop session.
    pub ran: u32,
    /// Optional cap for loop tasks.
    pub max_tasks: Option<u32>,
    /// Whether draft tasks are eligible for loop selection.
    pub include_draft: bool,
}

impl LoopState {
    /// Create a new loop state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the loop mode.
    ///
    /// If a runner is currently active, arms the loop to start after
    /// the current task completes.
    pub fn start(&mut self, runner_active: bool) -> LoopStartResult {
        if self.active {
            return LoopStartResult::AlreadyRunning;
        }

        self.active = true;
        self.ran = 0;

        if runner_active {
            self.arm_after_current = true;
            LoopStartResult::Armed
        } else {
            LoopStartResult::Started
        }
    }

    /// Stop the loop mode.
    ///
    /// Returns the number of tasks that ran during this loop session.
    pub fn stop(&mut self) -> u32 {
        let ran = self.ran;
        self.active = false;
        self.arm_after_current = false;
        ran
    }

    /// Record a task completion in the loop.
    ///
    /// Returns true if the loop should continue, false if it should stop
    /// (e.g., max_tasks reached).
    pub fn record_completion(&mut self) -> bool {
        // If armed, just disarm without counting
        if self.arm_after_current {
            self.arm_after_current = false;
            return self.active;
        }

        // Increment counter
        self.ran = self.ran.saturating_add(1);

        // Check if we've reached the max
        if let Some(max) = self.max_tasks
            && self.ran >= max
        {
            self.active = false;
            return false;
        }

        self.active
    }

    /// Check if the loop has reached its task limit.
    pub fn at_limit(&self) -> bool {
        match self.max_tasks {
            Some(max) => self.ran >= max,
            None => false,
        }
    }

    /// Get the number of remaining tasks allowed (if max is set).
    pub fn remaining(&self) -> Option<u32> {
        self.max_tasks.map(|max| max.saturating_sub(self.ran))
    }

    /// Select the next runnable task for loop mode.
    ///
    /// This prefers resuming `doing` tasks, then the first runnable `todo`, then `draft` (when
    /// enabled), while skipping tasks whose dependencies are not met.
    pub fn next_task_id(&self, queue: &QueueFile, done: Option<&QueueFile>) -> Option<String> {
        let options = queue::operations::RunnableSelectionOptions::new(self.include_draft, true);
        queue::operations::select_runnable_task_index(queue, done, options)
            .and_then(|idx| queue.tasks.get(idx).map(|task| task.id.clone()))
    }

    /// Check if there are any runnable tasks available.
    pub fn has_runnable_tasks(&self, queue: &QueueFile, done: Option<&QueueFile>) -> bool {
        self.next_task_id(queue, done).is_some()
    }

    /// Reset the loop state to defaults.
    pub fn reset(&mut self) {
        self.active = false;
        self.arm_after_current = false;
        self.ran = 0;
        self.max_tasks = None;
        self.include_draft = false;
    }

    /// Get a status message describing the current loop state.
    pub fn status_message(&self) -> String {
        if !self.active {
            if self.ran > 0 {
                format!("Loop stopped (ran {})", self.ran)
            } else {
                "Loop stopped".to_string()
            }
        } else if self.arm_after_current {
            "Loop armed (will start after current task)".to_string()
        } else if let Some(max) = self.max_tasks {
            format!("Loop running ({}/{})", self.ran, max)
        } else {
            format!("Loop running (ran {})", self.ran)
        }
    }
}

/// Result of attempting to start the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopStartResult {
    /// Loop started immediately.
    Started,
    /// Loop armed to start after current task.
    Armed,
    /// Loop was already running.
    AlreadyRunning,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};

    fn create_task_with_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            status,
            ..Default::default()
        }
    }

    #[test]
    fn test_loop_start_stop() {
        let mut state = LoopState::new();

        // Start when not running
        assert_eq!(state.start(false), LoopStartResult::Started);
        assert!(state.active);
        assert_eq!(state.ran, 0);

        // Start when already running
        assert_eq!(state.start(false), LoopStartResult::AlreadyRunning);

        // Stop
        let ran = state.stop();
        assert_eq!(ran, 0);
        assert!(!state.active);
    }

    #[test]
    fn test_loop_arm_when_runner_active() {
        let mut state = LoopState::new();

        let result = state.start(true);
        assert_eq!(result, LoopStartResult::Armed);
        assert!(state.active);
        assert!(state.arm_after_current);
    }

    #[test]
    fn test_record_completion() {
        let mut state = LoopState::new();
        state.active = true;

        // First completion when armed should just disarm
        state.arm_after_current = true;
        assert!(state.record_completion());
        assert!(!state.arm_after_current);
        assert_eq!(state.ran, 0);

        // Subsequent completions should increment counter
        assert!(state.record_completion());
        assert_eq!(state.ran, 1);

        assert!(state.record_completion());
        assert_eq!(state.ran, 2);
    }

    #[test]
    fn test_max_tasks_limit() {
        let mut state = LoopState::new();
        state.active = true;
        state.max_tasks = Some(3);

        assert!(state.record_completion()); // ran = 1, continue
        assert!(state.record_completion()); // ran = 2, continue
        assert!(!state.record_completion()); // ran = 3, limit reached, loop stops

        assert!(!state.active);
        assert_eq!(state.ran, 3);
    }

    #[test]
    fn test_at_limit() {
        let mut state = LoopState::new();
        state.max_tasks = Some(2);
        state.ran = 2;

        assert!(state.at_limit());

        state.ran = 1;
        assert!(!state.at_limit());

        // No limit means never at limit
        state.max_tasks = None;
        state.ran = 1000;
        assert!(!state.at_limit());
    }

    #[test]
    fn test_remaining() {
        let mut state = LoopState::new();
        state.max_tasks = Some(10);
        state.ran = 3;

        assert_eq!(state.remaining(), Some(7));

        state.ran = 10;
        assert_eq!(state.remaining(), Some(0));

        state.max_tasks = None;
        assert_eq!(state.remaining(), None);
    }

    #[test]
    fn test_status_message() {
        let mut state = LoopState::new();

        // Stopped, no tasks ran
        assert_eq!(state.status_message(), "Loop stopped");

        // Stopped, some tasks ran
        state.ran = 5;
        assert_eq!(state.status_message(), "Loop stopped (ran 5)");

        // Armed
        state.active = true;
        state.arm_after_current = true;
        assert_eq!(
            state.status_message(),
            "Loop armed (will start after current task)"
        );

        // Running with max
        state.arm_after_current = false;
        state.max_tasks = Some(10);
        assert_eq!(state.status_message(), "Loop running (5/10)");

        // Running without max
        state.max_tasks = None;
        assert_eq!(state.status_message(), "Loop running (ran 5)");
    }
}
