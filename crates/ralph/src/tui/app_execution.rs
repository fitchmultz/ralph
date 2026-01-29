//! Execution state and phase tracking for the TUI.
//!
//! Responsibilities:
//! - Track runner execution state (active, task ID, execution kind).
//! - Manage execution phases (Planning, Implementation, Review, Complete).
//! - Track phase timing and completion status.
//! - Parse log lines for phase detection.
//!
//! Not handled here:
//! - Actual runner execution (handled by runner module).
//! - Log storage and scrolling (handled by app_logs module).
//! - Task queue management (handled by queue module).
//!
//! Invariants/assumptions:
//! - Phase tracking starts when a task execution begins.
//! - Phase transitions are triggered by log line markers or explicit calls.
//! - Completion times are recorded when transitioning to the next phase.

#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Kind of runner currently executing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunningKind {
    /// Executing a task.
    Task,
    /// Running a scan with the given focus.
    Scan { focus: String },
    /// Running the task builder.
    TaskBuilder,
}

/// Execution phase for multi-phase task workflows.
///
/// Tracks which phase of a 1-3 phase workflow is currently active.
/// Used for progress visualization in the TUI execution view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionPhase {
    /// Phase 1: Planning and analysis
    Planning,
    /// Phase 2: Implementation and CI
    Implementation,
    /// Phase 3: Review and completion
    Review,
    /// Execution completed
    Complete,
}

impl ExecutionPhase {
    /// Returns the human-readable name for this phase.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionPhase::Planning => "Planning",
            ExecutionPhase::Implementation => "Implementation",
            ExecutionPhase::Review => "Review",
            ExecutionPhase::Complete => "Complete",
        }
    }

    /// Returns the phase number (1-3) or 0 for Complete.
    pub fn phase_number(&self) -> u8 {
        match self {
            ExecutionPhase::Planning => 1,
            ExecutionPhase::Implementation => 2,
            ExecutionPhase::Review => 3,
            ExecutionPhase::Complete => 0,
        }
    }
}

/// Tracks the state of task execution including phases and timing.
#[derive(Debug)]
pub struct ExecutionState {
    /// Whether a runner thread is currently executing a task.
    pub runner_active: bool,
    /// Task ID currently running, if any.
    pub running_task_id: Option<String>,
    /// Kind of runner currently executing (task vs scan vs task builder).
    pub running_kind: Option<RunningKind>,
    /// Current execution phase for multi-phase workflows.
    pub execution_phase: ExecutionPhase,
    /// Start times for each phase (used for elapsed time tracking).
    phase_start_times: HashMap<ExecutionPhase, Instant>,
    /// Completed phase durations (captured when transitioning to next phase).
    phase_completion_times: HashMap<ExecutionPhase, Duration>,
    /// When the overall execution started (for total time tracking).
    pub total_execution_start: Option<Instant>,
    /// Whether to show the progress panel in execution view.
    pub show_progress_panel: bool,
    /// Number of configured phases (1, 2, or 3) for the current workflow.
    pub configured_phases: u8,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            runner_active: false,
            running_task_id: None,
            running_kind: None,
            execution_phase: ExecutionPhase::Planning,
            phase_start_times: HashMap::new(),
            phase_completion_times: HashMap::new(),
            total_execution_start: None,
            show_progress_panel: true,
            configured_phases: 3,
        }
    }
}

impl ExecutionState {
    /// Create a new execution state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset phase tracking for a new execution.
    ///
    /// Clears previous phase data and initializes tracking for the given
    /// number of phases (1, 2, or 3).
    pub fn reset_phase_tracking(&mut self, total_phases: u8) {
        self.execution_phase = ExecutionPhase::Planning;
        self.phase_start_times.clear();
        self.phase_completion_times.clear();
        self.total_execution_start = Some(Instant::now());
        self.configured_phases = total_phases.clamp(1, 3);
        self.show_progress_panel = true;
        self.phase_start_times
            .insert(ExecutionPhase::Planning, Instant::now());
    }

    /// Transition to a new execution phase.
    ///
    /// Records the completion time for the current phase and starts
    /// tracking the new phase.
    pub fn transition_to_phase(&mut self, new_phase: ExecutionPhase) {
        // Record completion time for current phase
        if let Some(start) = self.phase_start_times.get(&self.execution_phase) {
            let elapsed = start.elapsed();
            self.phase_completion_times
                .insert(self.execution_phase, elapsed);
        }

        // Start new phase
        self.execution_phase = new_phase;
        if new_phase != ExecutionPhase::Complete {
            self.phase_start_times.insert(new_phase, Instant::now());
        }
    }

    /// Get elapsed time for a specific phase.
    ///
    /// Returns the completed duration if the phase is finished,
    /// or the current elapsed time if it's active or pending.
    pub fn phase_elapsed(&self, phase: ExecutionPhase) -> Duration {
        if let Some(completed) = self.phase_completion_times.get(&phase) {
            *completed
        } else if let Some(start) = self.phase_start_times.get(&phase) {
            start.elapsed()
        } else {
            Duration::ZERO
        }
    }

    /// Get total execution time.
    ///
    /// Returns the elapsed time since execution started, or ZERO
    /// if execution hasn't started.
    pub fn total_execution_time(&self) -> Duration {
        self.total_execution_start
            .map(|start| start.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Format a duration as MM:SS.
    pub fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    /// Check if a phase is completed.
    pub fn is_phase_completed(&self, phase: ExecutionPhase) -> bool {
        self.phase_completion_times.contains_key(&phase)
            || phase.phase_number() < self.execution_phase.phase_number()
    }

    /// Check if a phase is currently active.
    pub fn is_phase_active(&self, phase: ExecutionPhase) -> bool {
        self.execution_phase == phase
    }

    /// Process a log line for phase detection.
    ///
    /// Parses runner output to detect phase transitions based on
    /// phase header markers in the output.
    pub fn process_log_line(&mut self, line: &str) {
        if line.contains("# PLANNING MODE") {
            self.transition_to_phase(ExecutionPhase::Planning);
        } else if line.contains("# IMPLEMENTATION MODE") {
            self.transition_to_phase(ExecutionPhase::Implementation);
        } else if line.contains("# CODE REVIEW MODE") {
            self.transition_to_phase(ExecutionPhase::Review);
        }
    }

    /// Mark execution as complete.
    pub fn mark_complete(&mut self) {
        self.transition_to_phase(ExecutionPhase::Complete);
        self.runner_active = false;
        self.running_task_id = None;
        self.running_kind = None;
    }

    /// Start a new task execution.
    ///
    /// Initializes the execution state for running a task.
    pub fn start_task(&mut self, task_id: String, phases: u8) {
        self.runner_active = true;
        self.running_task_id = Some(task_id);
        self.running_kind = Some(RunningKind::Task);
        self.reset_phase_tracking(phases);
    }

    /// Start a scan execution.
    pub fn start_scan(&mut self, focus: String) {
        self.runner_active = true;
        self.running_task_id = Some(format!("Scan: {}", focus));
        self.running_kind = Some(RunningKind::Scan {
            focus: focus.clone(),
        });
        // Scans don't use phase tracking
        self.execution_phase = ExecutionPhase::Complete;
        self.total_execution_start = Some(Instant::now());
    }

    /// Start task builder execution.
    pub fn start_task_builder(&mut self) {
        self.runner_active = true;
        self.running_task_id = Some("Task Builder".to_string());
        self.running_kind = Some(RunningKind::TaskBuilder);
        // Task builder doesn't use phase tracking
        self.execution_phase = ExecutionPhase::Complete;
        self.total_execution_start = Some(Instant::now());
    }

    /// Get the current phase number (1-3) or 0 for Complete.
    pub fn current_phase_number(&self) -> u8 {
        self.execution_phase.phase_number()
    }

    /// Get the name of the current phase.
    pub fn current_phase_name(&self) -> &'static str {
        self.execution_phase.as_str()
    }

    /// Check if execution is currently running a task (not scan or builder).
    pub fn is_running_task(&self) -> bool {
        self.runner_active && matches!(self.running_kind, Some(RunningKind::Task))
    }

    /// Check if execution is currently running a scan.
    pub fn is_running_scan(&self) -> bool {
        self.runner_active && matches!(self.running_kind, Some(RunningKind::Scan { .. }))
    }

    /// Check if execution is currently running the task builder.
    pub fn is_running_task_builder(&self) -> bool {
        self.runner_active && matches!(self.running_kind, Some(RunningKind::TaskBuilder))
    }

    /// Reset the execution state to idle.
    pub fn reset(&mut self) {
        self.runner_active = false;
        self.running_task_id = None;
        self.running_kind = None;
        self.execution_phase = ExecutionPhase::Planning;
        self.phase_start_times.clear();
        self.phase_completion_times.clear();
        self.total_execution_start = None;
        self.show_progress_panel = true;
        self.configured_phases = 3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_phase_as_str() {
        assert_eq!(ExecutionPhase::Planning.as_str(), "Planning");
        assert_eq!(ExecutionPhase::Implementation.as_str(), "Implementation");
        assert_eq!(ExecutionPhase::Review.as_str(), "Review");
        assert_eq!(ExecutionPhase::Complete.as_str(), "Complete");
    }

    #[test]
    fn test_execution_phase_number() {
        assert_eq!(ExecutionPhase::Planning.phase_number(), 1);
        assert_eq!(ExecutionPhase::Implementation.phase_number(), 2);
        assert_eq!(ExecutionPhase::Review.phase_number(), 3);
        assert_eq!(ExecutionPhase::Complete.phase_number(), 0);
    }

    #[test]
    fn test_reset_phase_tracking() {
        let mut state = ExecutionState::new();
        state.reset_phase_tracking(2);

        assert_eq!(state.execution_phase, ExecutionPhase::Planning);
        assert_eq!(state.configured_phases, 2);
        assert!(state.total_execution_start.is_some());
        assert!(state
            .phase_start_times
            .contains_key(&ExecutionPhase::Planning));
    }

    #[test]
    fn test_phase_transition() {
        let mut state = ExecutionState::new();
        state.reset_phase_tracking(3);

        // Transition to implementation
        state.transition_to_phase(ExecutionPhase::Implementation);
        assert_eq!(state.execution_phase, ExecutionPhase::Implementation);
        assert!(state
            .phase_completion_times
            .contains_key(&ExecutionPhase::Planning));

        // Transition to review
        state.transition_to_phase(ExecutionPhase::Review);
        assert_eq!(state.execution_phase, ExecutionPhase::Review);
        assert!(state
            .phase_completion_times
            .contains_key(&ExecutionPhase::Implementation));
    }

    #[test]
    fn test_is_phase_completed() {
        let mut state = ExecutionState::new();
        state.reset_phase_tracking(3);

        assert!(!state.is_phase_completed(ExecutionPhase::Planning));

        state.transition_to_phase(ExecutionPhase::Implementation);
        assert!(state.is_phase_completed(ExecutionPhase::Planning));
        assert!(!state.is_phase_completed(ExecutionPhase::Implementation));
    }

    #[test]
    fn test_process_log_line() {
        let mut state = ExecutionState::new();
        state.reset_phase_tracking(3);

        state.process_log_line("# IMPLEMENTATION MODE");
        assert_eq!(state.execution_phase, ExecutionPhase::Implementation);

        state.process_log_line("# CODE REVIEW MODE");
        assert_eq!(state.execution_phase, ExecutionPhase::Review);

        state.process_log_line("# PLANNING MODE");
        assert_eq!(state.execution_phase, ExecutionPhase::Planning);
    }

    #[test]
    fn test_format_duration() {
        let duration = Duration::from_secs(125); // 2:05
        assert_eq!(ExecutionState::format_duration(duration), "02:05");

        let duration = Duration::from_secs(59); // 0:59
        assert_eq!(ExecutionState::format_duration(duration), "00:59");
    }

    #[test]
    fn test_running_kind_checks() {
        let mut state = ExecutionState::new();

        assert!(!state.is_running_task());
        assert!(!state.is_running_scan());
        assert!(!state.is_running_task_builder());

        state.start_task("RQ-0001".to_string(), 3);
        assert!(state.is_running_task());
        assert!(!state.is_running_scan());
        assert!(!state.is_running_task_builder());

        state.start_scan("test".to_string());
        assert!(!state.is_running_task());
        assert!(state.is_running_scan());
        assert!(!state.is_running_task_builder());

        state.start_task_builder();
        assert!(!state.is_running_task());
        assert!(!state.is_running_scan());
        assert!(state.is_running_task_builder());
    }
}
