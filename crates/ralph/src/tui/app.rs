//! TUI application state, event handling, and queue display.
//!
//! Responsibilities:
//! - Manage TUI state, rendering, and input handling.
//! - Load queue data for interactive inspection and updates.
//! - Coordinate TUI session setup and teardown.
//!
//! Not handled here:
//! - CLI argument parsing or command dispatch.
//! - Queue persistence details (see `crate::queue`).
//! - Lock ownership metadata (see `crate::lock`).
//! - Filter state management (see `app_filters` module).
//! - Execution phase tracking (see `app_execution` module).
//! - Log management (see `app_logs` module).
//! - Help overlay state (see `app_help` module).
//! - Loop mode state (see `app_loop` module).
//! - ID-to-index caching (see `app_id_index` module).
//!
//! Invariants/assumptions:
//! - Callers acquire the queue lock before mutating state.
//! - TUI runs in a terminal with raw mode support.

use crate::config::ConfigLayer;
use crate::constants::buffers::MAX_ANSI_BUFFER_SIZE;
use crate::constants::timeouts::SPINNER_UPDATE_INTERVAL_MS;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::progress::{ExecutionPhase, SpinnerState};
use crate::queue::TaskEditKey;
use crate::{config as crate_config, lock, queue, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use super::events::{
    handle_key_event, handle_mouse_event, AppMode, ConfirmDiscardAction, PaletteCommand,
    PaletteEntry, ScoredPaletteEntry, TaskBuilderState, TaskBuilderStep, TuiAction, ViewMode,
};
use super::render::draw_ui;
use super::terminal::{BorderStyle, ColorSupport, TerminalCapabilities};
use super::TextInput;
use super::{DetailsContext, DetailsState};
use crate::tui::app_execution::RunningKind;
use crate::tui::app_filters::{FilterKey, FilterSnapshot, FilterState};
use crate::tui::app_navigation::BoardNavigationState;
#[cfg(test)]
use crate::tui::app_options::FilterCacheStats;
use crate::tui::app_options::TuiOptions;
use crate::tui::app_palette::{scan_label, score_palette_entry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusedPanel {
    List,
    Details,
}

impl FocusedPanel {
    fn next(self) -> Self {
        match self {
            Self::List => Self::Details,
            Self::Details => Self::List,
        }
    }

    fn previous(self) -> Self {
        self.next()
    }
}

/// Application state for the TUI.
pub struct App {
    /// The active task queue.
    pub queue: QueueFile,
    /// The done archive queue.
    pub done: QueueFile,
    /// Currently selected task index.
    pub selected: usize,
    /// Current interaction mode.
    pub mode: AppMode,
    /// Scroll offset for the task list.
    pub scroll: usize,
    /// Width of the right panel for text wrapping.
    pub detail_width: u16,
    /// Which panel is focused for navigation input.
    focused_panel: FocusedPanel,
    /// Details panel scroll state using tui-scrollview.
    pub details: DetailsState,
    /// Context key for details content (used to reset scroll on change).
    details_context: Option<DetailsContext>,
    /// Flag indicating if active queue was modified (needs save).
    pub dirty: bool,
    /// Flag indicating if done archive was modified (needs save).
    pub dirty_done: bool,
    /// Project-specific config overrides (.ralph/config.json).
    pub project_config: ConfigLayer,
    /// Optional path to the project config.
    pub project_config_path: Option<PathBuf>,
    /// Flag indicating if project config was modified (needs save).
    pub dirty_config: bool,
    /// Last auto-save error message, if any.
    pub save_error: Option<String>,
    /// Status message shown in the footer (user-visible feedback).
    pub status_message: Option<String>,
    /// Execution logs.
    pub logs: Vec<String>,
    /// Raw ANSI bytes for terminal emulation display (tui-term integration).
    pub log_ansi_buffer: Vec<u8>,
    /// Scroll offset for execution logs.
    pub log_scroll: usize,
    /// Whether to auto-scroll execution logs.
    pub autoscroll: bool,
    /// Last known visible log lines in Executing view (for paging/auto-scroll).
    pub log_visible_lines: usize,
    /// Scroll offset for the help overlay.
    help_scroll: usize,
    /// Last known visible help lines in Help overlay (for paging).
    help_visible_lines: usize,
    /// Last known total help line count (post-wrap).
    help_total_lines: usize,
    /// Previous mode before entering the Help overlay.
    help_previous_mode: Option<AppMode>,
    /// Height of the task list (for scrolling calculation).
    pub list_height: usize,
    /// Last known list panel area (inner rect, without borders) for hit-testing.
    list_area: Option<Rect>,
    /// Whether a runner thread is currently executing a task.
    pub runner_active: bool,
    /// Task ID currently running, if any.
    pub running_task_id: Option<String>,
    /// Kind of runner currently executing (task vs scan).
    pub running_kind: Option<RunningKind>,
    /// Current execution phase for multi-phase workflows.
    pub execution_phase: ExecutionPhase,
    /// Start times for each phase (used for elapsed time tracking).
    pub phase_start_times: HashMap<ExecutionPhase, std::time::Instant>,
    /// Completed phase durations (captured when transitioning to next phase).
    pub phase_completion_times: HashMap<ExecutionPhase, std::time::Duration>,
    /// When the overall execution started (for total time tracking).
    pub total_execution_start: Option<std::time::Instant>,
    /// Whether to show the progress panel in execution view.
    pub show_progress_panel: bool,
    /// Number of configured phases (1, 2, or 3) for the current workflow.
    pub configured_phases: u8,
    /// Spinner state for animated progress indication.
    pub spinner: SpinnerState,
    /// Current operation description (e.g., "Running CI gate...").
    pub current_operation: String,
    /// Last time the spinner was updated.
    spinner_last_update: std::time::Instant,
    /// Spinner update interval.
    spinner_update_interval: std::time::Duration,
    /// Whether loop mode is active.
    pub loop_active: bool,
    /// When loop is enabled while a task is already running, do not count that finishing task.
    pub loop_arm_after_current: bool,
    /// Count of tasks successfully completed in the current loop session.
    pub loop_ran: u32,
    /// Optional cap for loop tasks.
    pub loop_max_tasks: Option<u32>,
    /// Whether draft tasks are eligible for loop selection.
    pub loop_include_draft: bool,
    /// Task ID prefix used for new task creation.
    pub id_prefix: String,
    /// Task ID width used for new task creation.
    pub id_width: usize,
    /// Maximum allowed dependency chain depth before warning.
    pub max_dependency_depth: u8,
    /// Optional path to the queue file.
    pub queue_path: Option<PathBuf>,
    /// Optional path to the done queue.
    pub done_path: Option<PathBuf>,
    /// Active filters applied to the task list.
    pub filters: FilterState,
    /// Snapshot of filters before entering a live filter input mode.
    filter_snapshot: Option<FilterSnapshot>,
    /// Cached filtered task indices into `queue.tasks`.
    pub filtered_indices: Vec<usize>,
    /// Queue revision that changes whenever tasks are reordered or mutated.
    queue_rev: u64,
    /// Cached task-id to queue index mapping.
    id_to_index: HashMap<String, usize>,
    /// Revision that the cached id->index map was built from.
    id_to_index_rev: u64,
    /// Revision that the cached filtered indices were built from.
    filtered_indices_rev: u64,
    /// Filter key used for the cached filtered indices.
    last_filter_key: Option<FilterKey>,
    #[cfg(test)]
    id_index_rebuilds: usize,
    #[cfg(test)]
    filtered_rebuilds: usize,
    /// Terminal capabilities detected at startup.
    pub terminal_capabilities: Option<TerminalCapabilities>,
    /// Color support level resolved from CLI and environment.
    pub color_support: Option<ColorSupport>,
    /// Border style for rendering (Unicode or ASCII).
    pub border_style: BorderStyle,
    /// Cached modification time for queue.json (for detecting external changes).
    queue_mtime: Option<std::time::SystemTime>,
    /// Cached modification time for done.json (for detecting external changes).
    done_mtime: Option<std::time::SystemTime>,
    /// Flag set when terminal was resized, cleared after redraw.
    /// Used to trigger layout recalculation and prevent visual glitches.
    resized: bool,
    /// Current view mode (list or kanban board).
    pub view_mode: ViewMode,
    /// Board-specific navigation state (only meaningful when view_mode == Board).
    pub board_nav: BoardNavigationState,
    /// Multi-select mode flag - when true, navigation keeps selections.
    pub multi_select_mode: bool,
    /// Set of selected task indices (positions in filtered_indices, not queue indices).
    pub selected_indices: HashSet<usize>,
    /// Current ETA estimate for the running task.
    pub current_eta: Option<crate::eta_calculator::EtaEstimate>,
}

impl App {
    /// Create a new TUI app from a queue file.
    pub fn new(queue: QueueFile) -> Self {
        let mut app = Self {
            queue,
            done: QueueFile::default(),
            selected: 0,
            mode: AppMode::Normal,
            scroll: 0,
            detail_width: 60,
            focused_panel: FocusedPanel::List,
            details: DetailsState::new(),
            details_context: None,
            dirty: false,
            dirty_done: false,
            project_config: ConfigLayer::default(),
            project_config_path: None,
            dirty_config: false,
            save_error: None,
            status_message: None,
            logs: Vec::new(),
            log_ansi_buffer: Vec::new(),
            log_scroll: 0,
            autoscroll: true,
            log_visible_lines: 20,
            help_scroll: 0,
            help_visible_lines: 1,
            help_total_lines: 0,
            help_previous_mode: None,
            list_height: 20,
            list_area: None,
            runner_active: false,
            running_task_id: None,
            running_kind: None,
            execution_phase: ExecutionPhase::Planning,
            phase_start_times: HashMap::new(),
            phase_completion_times: HashMap::new(),
            total_execution_start: None,
            show_progress_panel: true,
            configured_phases: 3,
            spinner: SpinnerState::default(),
            current_operation: "Initializing...".to_string(),
            spinner_last_update: std::time::Instant::now(),
            spinner_update_interval: std::time::Duration::from_millis(SPINNER_UPDATE_INTERVAL_MS),
            loop_active: false,
            loop_arm_after_current: false,
            loop_ran: 0,
            loop_max_tasks: None,
            loop_include_draft: false,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            max_dependency_depth: 10,
            queue_path: None,
            done_path: None,
            filters: FilterState::default(),
            filter_snapshot: None,
            filtered_indices: Vec::new(),
            queue_rev: 0,
            id_to_index: HashMap::new(),
            id_to_index_rev: u64::MAX,
            filtered_indices_rev: u64::MAX,
            last_filter_key: None,
            #[cfg(test)]
            id_index_rebuilds: 0,
            #[cfg(test)]
            filtered_rebuilds: 0,
            terminal_capabilities: None,
            color_support: None,
            border_style: BorderStyle::Unicode,
            queue_mtime: None,
            done_mtime: None,
            resized: false,
            view_mode: ViewMode::default(),
            board_nav: BoardNavigationState::new(),
            multi_select_mode: false,
            selected_indices: HashSet::new(),
            current_eta: None,
        };
        app.rebuild_filtered_view();
        app
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    // Multi-select methods

    /// Toggle multi-select mode on/off.
    ///
    /// When exiting multi-select mode, clears all selections.
    pub fn toggle_multi_select_mode(&mut self) {
        self.multi_select_mode = !self.multi_select_mode;
        if !self.multi_select_mode {
            self.selected_indices.clear();
        }
    }

    /// Toggle selection of the current task.
    ///
    /// Only has effect when in multi-select mode.
    pub fn toggle_current_selection(&mut self) {
        if self.multi_select_mode {
            let idx = self.selected;
            if self.selected_indices.contains(&idx) {
                self.selected_indices.remove(&idx);
            } else {
                self.selected_indices.insert(idx);
            }
        }
    }

    /// Clear all selections and exit multi-select mode.
    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
        self.multi_select_mode = false;
    }

    /// Get the count of selected tasks.
    pub fn selection_count(&self) -> usize {
        self.selected_indices.len()
    }

    /// Check if a filtered position is selected.
    pub fn is_selected(&self, filtered_idx: usize) -> bool {
        self.selected_indices.contains(&filtered_idx)
    }

    // Phase tracking methods

    /// Reset phase tracking for a new execution.
    ///
    /// Clears previous phase data and initializes tracking for the given
    /// number of phases (1, 2, or 3).
    pub fn reset_phase_tracking(&mut self, total_phases: u8) {
        self.execution_phase = ExecutionPhase::Planning;
        self.phase_start_times.clear();
        self.phase_completion_times.clear();
        self.total_execution_start = Some(std::time::Instant::now());
        self.configured_phases = total_phases.clamp(1, 3);
        self.show_progress_panel = true;
        self.phase_start_times
            .insert(ExecutionPhase::Planning, std::time::Instant::now());
    }

    /// Transition to a new execution phase.
    ///
    /// Records the completion time for the current phase and starts
    /// tracking the new phase. Also updates ETA estimate if historical
    /// data is available.
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
            self.phase_start_times
                .insert(new_phase, std::time::Instant::now());
        }

        // Update ETA estimate
        self.update_eta_estimate();
    }

    /// Get the cache directory path derived from queue_path.
    fn cache_dir(&self) -> Option<std::path::PathBuf> {
        self.queue_path.as_ref().map(|p| {
            p.parent()
                .map(|parent| parent.join("cache"))
                .unwrap_or_else(|| p.join(".ralph").join("cache"))
        })
    }

    /// Update the ETA estimate based on current progress and historical data.
    fn update_eta_estimate(&mut self) {
        // Only calculate ETA for task executions
        if self.running_kind != Some(RunningKind::Task) {
            return;
        }

        let Some(cache_dir) = self.cache_dir() else {
            return;
        };

        // Get runner and model from config
        let runner = self
            .project_config
            .agent
            .runner
            .as_ref()
            .map(|r| r.as_str().to_string())
            .unwrap_or_else(|| "claude".to_string());
        let model = self
            .project_config
            .agent
            .model
            .as_ref()
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "default".to_string());

        // Load ETA calculator
        let calculator = crate::eta_calculator::EtaCalculator::load(&cache_dir);

        // Calculate ETA
        let phase_elapsed = self.phase_elapsed_map();
        self.current_eta = calculator.calculate_eta(
            runner.as_str(),
            model.as_str(),
            self.configured_phases,
            self.execution_phase,
            &phase_elapsed,
        );
    }

    /// Record execution history for the completed task.
    fn record_execution_history(&self) {
        let Some(ref task_id) = self.running_task_id else {
            return;
        };

        let Some(cache_dir) = self.cache_dir() else {
            return;
        };

        // Get runner and model from config
        let runner = self
            .project_config
            .agent
            .runner
            .as_ref()
            .map(|r| r.as_str().to_string())
            .unwrap_or_else(|| "claude".to_string());
        let model = self
            .project_config
            .agent
            .model
            .as_ref()
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "default".to_string());

        // Get phase durations
        let phase_durations = self.phase_completion_times.clone();

        // Get total duration
        let total_duration = self.total_execution_time();

        // Record execution (ignore errors - this is best-effort)
        let _ = crate::execution_history::record_execution(
            task_id,
            &runner,
            &model,
            self.configured_phases,
            phase_durations,
            total_duration,
            &cache_dir,
        );
    }

    /// Get elapsed time for a specific phase.
    ///
    /// Returns the completed duration if the phase is finished,
    /// or the current elapsed time if it's active or pending.
    pub fn phase_elapsed(&self, phase: ExecutionPhase) -> std::time::Duration {
        if let Some(completed) = self.phase_completion_times.get(&phase) {
            *completed
        } else if let Some(start) = self.phase_start_times.get(&phase) {
            start.elapsed()
        } else {
            std::time::Duration::ZERO
        }
    }

    /// Get total execution time.
    ///
    /// Returns the elapsed time since execution started, or ZERO
    /// if execution hasn't started.
    pub fn total_execution_time(&self) -> std::time::Duration {
        self.total_execution_start
            .map(|start| start.elapsed())
            .unwrap_or(std::time::Duration::ZERO)
    }

    /// Format a duration as MM:SS.
    pub fn format_duration(duration: std::time::Duration) -> String {
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

    /// Calculate overall completion percentage (0-100).
    ///
    /// Based on completed phases. Each completed phase contributes equally
    /// to the percentage (e.g., 1/3 = 33%, 2/3 = 67%, 3/3 = 100%).
    pub fn completion_percentage(&self) -> u8 {
        if self.configured_phases == 0 {
            return 0;
        }

        let completed_phases = self.completed_phase_count();

        // Calculate percentage based on completed phases
        let percentage = (completed_phases as f32 / self.configured_phases as f32) * 100.0;
        percentage.clamp(0.0, 100.0) as u8
    }

    /// Count completed phases.
    fn completed_phase_count(&self) -> u8 {
        // Count phases that have been completed (have a completion time recorded)
        // or phases that are before the current phase
        let mut count = 0u8;

        for phase in [
            ExecutionPhase::Planning,
            ExecutionPhase::Implementation,
            ExecutionPhase::Review,
        ] {
            if self.is_phase_completed(phase) {
                count += 1;
            }
        }

        count
    }

    /// Get elapsed time for all phases as a map.
    pub fn phase_elapsed_map(&self) -> HashMap<ExecutionPhase, std::time::Duration> {
        let mut map = HashMap::new();
        for phase in [
            ExecutionPhase::Planning,
            ExecutionPhase::Implementation,
            ExecutionPhase::Review,
        ] {
            map.insert(phase, self.phase_elapsed(phase));
        }
        map
    }

    /// Process a log line for phase detection.
    ///
    /// Parses runner output to detect phase transitions based on
    /// phase header markers in the output.
    pub fn process_log_line_for_phase(&mut self, line: &str) {
        if line.contains("# PLANNING MODE") {
            self.transition_to_phase(ExecutionPhase::Planning);
        } else if line.contains("# IMPLEMENTATION MODE") {
            self.transition_to_phase(ExecutionPhase::Implementation);
        } else if line.contains("# CODE REVIEW MODE") {
            self.transition_to_phase(ExecutionPhase::Review);
        }
    }

    // Spinner and operation methods

    /// Update the spinner animation if enough time has passed.
    /// Returns true if the spinner frame was advanced.
    pub fn tick_spinner(&mut self) -> bool {
        let now = std::time::Instant::now();
        if now.duration_since(self.spinner_last_update) >= self.spinner_update_interval {
            self.spinner.tick();
            self.spinner_last_update = now;
            true
        } else {
            false
        }
    }

    /// Get the current spinner frame.
    pub fn spinner_frame(&self) -> &str {
        self.spinner.current_frame()
    }

    /// Set the current operation description.
    pub fn set_operation(&mut self, operation: &str) {
        self.current_operation = operation.to_string();
    }

    /// Get the current operation description.
    pub fn operation(&self) -> &str {
        &self.current_operation
    }

    /// Reset the spinner animation state.
    pub fn reset_spinner(&mut self) {
        self.spinner.reset();
        self.spinner_last_update = std::time::Instant::now();
    }

    pub(crate) fn focus_next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    pub(crate) fn focus_previous_panel(&mut self) {
        self.focused_panel = self.focused_panel.previous();
    }

    pub(crate) fn focus_list_panel(&mut self) {
        self.focused_panel = FocusedPanel::List;
    }

    pub(crate) fn details_focused(&self) -> bool {
        self.focused_panel == FocusedPanel::Details
    }

    pub(crate) fn set_list_area(&mut self, area: Rect) {
        self.list_area = Some(area);
    }

    pub(crate) fn clear_list_area(&mut self) {
        self.list_area = None;
    }

    pub(crate) fn list_area(&self) -> Option<Rect> {
        self.list_area
    }

    /// Handle terminal resize events.
    ///
    /// Responsibilities:
    /// - Clear cached list_area to force recalculation on next render.
    /// - Clamp scroll positions to ensure they remain valid after terminal resize.
    ///
    /// Not handled here:
    /// - Layout computation (handled fresh each frame in render loop).
    /// - Widget positioning (ratatui handles this via `f.area()`).
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        // Set flag to trigger immediate redraw and layout recalculation
        self.resized = true;

        // Clear cached list_area to force recalculation
        self.clear_list_area();

        // Clamp selection and scroll to valid range for the filtered list
        self.clamp_selection_and_scroll();

        // Note: ScrollViewState handles its own bounds checking internally

        // Clamp help scroll to valid range
        let help_max = self.max_help_scroll(self.help_total_lines);
        if self.help_scroll > help_max {
            self.help_scroll = help_max;
        }

        // Update detail width for text wrapping calculations
        self.detail_width = width.saturating_sub(4);

        // Clamp log scroll to ensure it stays within bounds after resize
        let log_count = self.logs.len();
        if self.log_scroll > log_count {
            self.log_scroll = log_count;
        }

        // Reset ANSI buffer visible lines to trigger recalculation
        if height > 0 {
            self.log_visible_lines = height.saturating_sub(4) as usize;
        }
    }

    /// Check if the terminal was resized since the last redraw.
    ///
    /// Returns true if a resize occurred and clears the flag.
    pub(crate) fn take_resized(&mut self) -> bool {
        let was_resized = self.resized;
        self.resized = false;
        was_resized
    }

    pub(crate) fn unsafe_to_discard(&self) -> bool {
        self.dirty || self.dirty_done || self.dirty_config || self.save_error.is_some()
    }

    pub(crate) fn bump_queue_rev(&mut self) {
        self.queue_rev = self.queue_rev.wrapping_add(1);
    }

    pub(crate) fn queue_rev(&self) -> u64 {
        self.queue_rev
    }

    fn ensure_id_index_map(&mut self) {
        if self.id_to_index_rev == self.queue_rev {
            return;
        }

        self.id_to_index.clear();
        for (idx, task) in self.queue.tasks.iter().enumerate() {
            self.id_to_index.insert(task.id.clone(), idx);
        }
        self.id_to_index_rev = self.queue_rev;

        #[cfg(test)]
        {
            self.id_index_rebuilds += 1;
        }
    }

    fn ensure_filtered_indices(&mut self) {
        let filter_key = FilterKey::from_filters(&self.filters);
        if self.filtered_indices_rev == self.queue_rev
            && self.last_filter_key.as_ref() == Some(&filter_key)
        {
            return;
        }

        let filtered_ids: Vec<String> = {
            let mut filtered = queue::filter_tasks(
                &self.queue,
                &self.filters.statuses,
                &self.filters.tags,
                &self.filters.search_options.scopes,
                None,
            );

            if !self.filters.query.trim().is_empty() {
                match queue::search_tasks(
                    filtered,
                    &self.filters.query,
                    self.filters.search_options.use_regex,
                    self.filters.search_options.case_sensitive,
                ) {
                    Ok(results) => {
                        filtered = results;
                    }
                    Err(err) => {
                        self.set_status_message(format!("Search error: {}", err));
                        filtered = Vec::new();
                    }
                }
            }

            filtered.into_iter().map(|task| task.id.clone()).collect()
        };

        self.ensure_id_index_map();
        self.filtered_indices = filtered_ids
            .iter()
            .filter_map(|id| self.id_to_index.get(id).copied())
            .collect();
        self.last_filter_key = Some(filter_key);
        self.filtered_indices_rev = self.queue_rev;

        #[cfg(test)]
        {
            self.filtered_rebuilds += 1;
        }
    }

    pub(crate) fn append_log_lines<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = String>,
    {
        for line in lines {
            // Add to text logs (for backward compatibility and scroll calculations)
            self.logs.push(line.clone());
            // Add to ANSI buffer with newline for terminal emulation
            self.log_ansi_buffer.extend_from_slice(line.as_bytes());
            self.log_ansi_buffer.push(b'\n');
        }
        // Trim old logs if we exceed the maximum
        if self.logs.len() > 10000 {
            let excess = self.logs.len() - 10000;
            self.logs.drain(0..excess);
            self.log_scroll = self.log_scroll.saturating_sub(excess);
        }
        // Trim ANSI buffer if it exceeds the maximum size
        if self.log_ansi_buffer.len() > MAX_ANSI_BUFFER_SIZE {
            let excess = self.log_ansi_buffer.len() - MAX_ANSI_BUFFER_SIZE;
            self.log_ansi_buffer.drain(0..excess);
        }
        if self.autoscroll {
            let visible_lines = self.log_visible_lines();
            self.log_scroll = self.max_log_scroll(visible_lines);
        }
    }

    pub(crate) fn set_runner_error(&mut self, msg: &str) {
        let summary_line = msg
            .lines()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
            .unwrap_or("Runner error");
        let status = if summary_line == "Runner error" {
            "Runner error (see logs)".to_string()
        } else {
            format!("Runner error: {} (see logs)", summary_line)
        };
        self.set_status_message(status);

        let mut lines = Vec::new();
        lines.push("Runner error details:".to_string());
        if msg.trim().is_empty() {
            lines.push("(no details provided)".to_string());
        } else {
            for line in msg.lines() {
                lines.push(line.to_string());
            }
        }
        self.append_log_lines(lines);
    }

    /// Return the number of tasks in the filtered view.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
    }

    #[cfg(test)]
    pub(crate) fn filter_cache_stats(&self) -> FilterCacheStats {
        FilterCacheStats {
            id_index_rebuilds: self.id_index_rebuilds,
            filtered_rebuilds: self.filtered_rebuilds,
        }
    }

    /// Return true if any filters are active.
    pub fn has_active_filters(&self) -> bool {
        !self.filters.query.trim().is_empty()
            || !self.filters.tags.is_empty()
            || !self.filters.statuses.is_empty()
            || !self.filters.search_options.scopes.is_empty()
            || self.filters.search_options.use_regex
            || self.filters.search_options.case_sensitive
    }

    /// Create a human-readable summary of active filters.
    pub fn filter_summary(&self) -> Option<String> {
        if !self.has_active_filters() {
            return None;
        }

        let mut parts = Vec::new();
        if let Some(status) = self.filters.statuses.first() {
            parts.push(format!("status={}", status.as_str()));
        }
        if !self.filters.tags.is_empty() {
            parts.push(format!("tags={}", self.filters.tags.join(",")));
        }
        if !self.filters.query.trim().is_empty() {
            parts.push(format!("query={}", self.filters.query.trim()));
        }
        for scope in &self.filters.search_options.scopes {
            parts.push(format!("scope={}", scope));
        }
        if self.filters.search_options.use_regex {
            parts.push("regex".to_string());
        }
        if self.filters.search_options.case_sensitive {
            parts.push("case-sensitive".to_string());
        }
        Some(format!("filters: {}", parts.join(" ")))
    }

    /// Toggle case-sensitive search.
    pub fn toggle_case_sensitive(&mut self) {
        self.filters.search_options.case_sensitive = !self.filters.search_options.case_sensitive;
        let state = if self.filters.search_options.case_sensitive {
            "enabled"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Case-sensitive search {}", state));
        self.rebuild_filtered_view();
    }

    /// Toggle regex search.
    pub fn toggle_regex(&mut self) {
        self.filters.search_options.use_regex = !self.filters.search_options.use_regex;
        // Regex and fuzzy are mutually exclusive
        if self.filters.search_options.use_regex && self.filters.search_options.use_fuzzy {
            self.filters.search_options.use_fuzzy = false;
        }
        let state = if self.filters.search_options.use_regex {
            "enabled (fuzzy disabled)"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Regex search {}", state));
        self.rebuild_filtered_view();
    }

    /// Toggle fuzzy search.
    pub fn toggle_fuzzy(&mut self) {
        self.filters.search_options.use_fuzzy = !self.filters.search_options.use_fuzzy;
        // Fuzzy and regex are mutually exclusive
        if self.filters.search_options.use_fuzzy && self.filters.search_options.use_regex {
            self.filters.search_options.use_regex = false;
        }
        let state = if self.filters.search_options.use_fuzzy {
            "enabled (regex disabled)"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Fuzzy search {}", state));
        self.rebuild_filtered_view();
    }

    /// Get the currently selected task, if any.
    pub fn selected_task(&self) -> Option<&Task> {
        self.selected_task_index()
            .and_then(|idx| self.queue.tasks.get(idx))
    }

    /// Get the currently selected task index in the queue, if any.
    pub fn selected_task_index(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected).copied()
    }

    pub(crate) fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
        self.clamp_selection_and_scroll();
    }

    /// Get the currently selected task mutably, if any.
    pub fn selected_task_mut(&mut self) -> Option<&mut Task> {
        let idx = self.selected_task_index()?;
        self.queue.tasks.get_mut(idx)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.filtered_len() > 0 && self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self, list_height: usize) {
        if self.selected + 1 < self.filtered_len() {
            self.selected += 1;
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected - list_height + 1;
            }
        }
    }

    /// Move selection up by a page.
    pub fn move_page_up(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            return;
        }
        let list_height = list_height.max(1);
        let step = list_height.saturating_sub(1).max(1);
        self.selected = self.selected.saturating_sub(step);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
    }

    /// Move selection down by a page.
    pub fn move_page_down(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            return;
        }
        let list_height = list_height.max(1);
        let step = list_height.saturating_sub(1).max(1);
        let max_index = self.filtered_len().saturating_sub(1);
        self.selected = (self.selected + step).min(max_index);
        if self.selected >= self.scroll + list_height {
            self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
        }
    }

    /// Jump selection to the top of the filtered list.
    pub fn jump_to_top(&mut self) {
        if self.filtered_len() == 0 {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        self.selected = 0;
        self.scroll = 0;
    }

    /// Jump to a task by its ID.
    ///
    /// The ID is matched case-insensitively. If the task is found but not visible
    /// due to active filters, filters are cleared first.
    ///
    /// Returns `true` if the task was found and selected, `false` otherwise.
    pub fn jump_to_task_by_id(&mut self, id: &str) -> bool {
        let normalized_id = id.trim().to_uppercase();
        if normalized_id.is_empty() {
            self.set_status_message("No task ID entered");
            return false;
        }

        // Ensure the id_to_index map is up to date
        self.ensure_id_index_map();

        // Find the task by ID (case-insensitive)
        let queue_index = self
            .id_to_index
            .iter()
            .find(|(k, _)| k.to_uppercase() == normalized_id)
            .map(|(_, &idx)| idx);

        let Some(queue_index) = queue_index else {
            self.set_status_message(format!("Task not found: {}", id));
            return false;
        };

        // Check if the task is visible in the filtered view
        if let Some(filtered_pos) = self
            .filtered_indices
            .iter()
            .position(|&idx| idx == queue_index)
        {
            // Task is visible - select it
            self.set_selected(filtered_pos);
            self.set_status_message(format!("Jumped to task {}", id));
            true
        } else {
            // Task exists but is filtered out - clear filters and try again
            self.clear_filters();
            self.rebuild_filtered_view();

            // Find the position in the new filtered view
            if let Some(filtered_pos) = self
                .filtered_indices
                .iter()
                .position(|&idx| idx == queue_index)
            {
                self.set_selected(filtered_pos);
                self.set_status_message(format!("Jumped to task {} (filters cleared)", id));
                true
            } else {
                // Shouldn't happen unless task was deleted between checks
                self.set_status_message(format!("Task not found: {}", id));
                false
            }
        }
    }

    /// Jump selection to the bottom of the filtered list.
    pub fn jump_to_bottom(&mut self, list_height: usize) {
        if self.filtered_len() == 0 {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        self.selected = self.filtered_len().saturating_sub(1);
        let list_height = list_height.max(1);
        self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
    }

    /// Move the selected task up in the queue.
    pub fn move_task_up(&mut self, now_rfc3339: &str) -> Result<()> {
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

    /// Move the selected task down in the queue.
    pub fn move_task_down(&mut self, now_rfc3339: &str) -> Result<()> {
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

    /// Delete the selected task.
    pub fn delete_selected_task(&mut self) -> Result<Task> {
        let selected_index = self
            .selected_task_index()
            .ok_or_else(|| anyhow!("No task selected"))?;
        let task = self
            .queue
            .tasks
            .get(selected_index)
            .ok_or_else(|| anyhow!("No task selected"))?
            .clone();

        let preferred_id = if selected_index + 1 < self.queue.tasks.len() {
            self.queue
                .tasks
                .get(selected_index + 1)
                .map(|t| t.id.clone())
        } else if selected_index > 0 {
            self.queue
                .tasks
                .get(selected_index - 1)
                .map(|t| t.id.clone())
        } else {
            None
        };

        self.queue.tasks.remove(selected_index);

        self.dirty = true;
        self.bump_queue_rev();
        self.set_status_message(format!("Deleted {}", task.id));
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        Ok(task)
    }

    /// Batch delete tasks by their filtered indices.
    ///
    /// Converts filtered indices to queue indices, deletes in reverse order,
    /// and rebuilds the filtered view.
    pub fn batch_delete_by_filtered_indices(
        &mut self,
        filtered_indices: &[usize],
    ) -> Result<usize> {
        if filtered_indices.is_empty() {
            return Ok(0);
        }

        // Convert filtered indices to queue indices
        let mut queue_indices: Vec<usize> = filtered_indices
            .iter()
            .filter_map(|&filtered_idx| self.filtered_indices.get(filtered_idx).copied())
            .collect();

        if queue_indices.is_empty() {
            return Ok(0);
        }

        // Sort in descending order to delete from end first (maintains index validity)
        queue_indices.sort_unstable_by(|a, b| b.cmp(a));
        queue_indices.dedup();

        let mut deleted_count = 0;
        for idx in queue_indices {
            if idx < self.queue.tasks.len() {
                self.queue.tasks.remove(idx);
                deleted_count += 1;
            }
        }

        if deleted_count > 0 {
            self.dirty = true;
            self.bump_queue_rev();
            self.selected_indices.clear();
            self.rebuild_filtered_view();
            // Adjust selection to stay valid
            if self.selected >= self.filtered_len() {
                self.selected = self.filtered_len().saturating_sub(1);
            }
        }

        Ok(deleted_count)
    }

    /// Batch archive tasks by their filtered indices.
    ///
    /// Converts filtered indices to queue indices, archives in reverse order,
    /// and rebuilds the filtered view.
    pub fn batch_archive_by_filtered_indices(
        &mut self,
        filtered_indices: &[usize],
        now_rfc3339: &str,
    ) -> Result<usize> {
        if filtered_indices.is_empty() {
            return Ok(0);
        }

        // Convert filtered indices to queue indices
        let mut queue_indices: Vec<usize> = filtered_indices
            .iter()
            .filter_map(|&filtered_idx| self.filtered_indices.get(filtered_idx).copied())
            .collect();

        if queue_indices.is_empty() {
            return Ok(0);
        }

        // Sort in descending order to archive from end first (maintains index validity)
        queue_indices.sort_unstable_by(|a, b| b.cmp(a));
        queue_indices.dedup();

        let mut archived_count = 0;
        for idx in queue_indices {
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
            self.selected_indices.clear();
            self.multi_select_mode = false;
            self.rebuild_filtered_view();
            // Adjust selection to stay valid
            if self.selected >= self.filtered_len() {
                self.selected = self.filtered_len().saturating_sub(1);
            }
        }

        Ok(archived_count)
    }

    /// Create a new task with default fields and the provided title.
    pub fn create_task_from_title(&mut self, title: &str, now_rfc3339: &str) -> Result<()> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            bail!(
                "TUI create task failed: title cannot be empty. Provide a non-empty title (e.g., 'Fix login bug')."
            );
        }

        let next_id = queue::next_id_across(
            &self.queue,
            Some(&self.done),
            &self.id_prefix,
            self.id_width,
            self.max_dependency_depth,
        )?;

        let task = Task {
            id: next_id.clone(),
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
        };

        self.queue.tasks.push(task);
        let new_index = self.queue.tasks.len().saturating_sub(1);
        self.bump_queue_rev();
        self.rebuild_filtered_view();

        if let Some(filtered_pos) = self
            .filtered_indices
            .iter()
            .position(|&idx| idx == new_index)
        {
            self.selected = filtered_pos;
            let list_height = self.list_height.max(1);
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
            }
        }

        self.dirty = true;
        self.mode = AppMode::Normal;
        self.set_status_message(format!("Created {}", next_id));
        Ok(())
    }

    /// Archive terminal tasks (Done/Rejected) into the done queue.
    pub fn archive_terminal_tasks(&mut self, now_rfc3339: &str) -> Result<usize> {
        let report =
            queue::archive_terminal_tasks_in_memory(&mut self.queue, &mut self.done, now_rfc3339)?;

        if report.moved_ids.is_empty() {
            self.set_status_message("No done/rejected tasks to archive");
            return Ok(0);
        }

        let moved_count = report.moved_ids.len();

        self.dirty = true;
        self.dirty_done = true;

        self.bump_queue_rev();
        self.rebuild_filtered_view();
        self.set_status_message(format!("Archived {} task(s)", moved_count));
        Ok(moved_count)
    }

    /// Archive a single terminal task (Done/Rejected) into the done queue.
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
        self.rebuild_filtered_view();

        Ok(())
    }

    /// Repair the queue file, optionally in dry-run mode.
    pub fn repair_queue(&mut self, dry_run: bool) -> Result<()> {
        let queue_path = self.queue_path.clone();
        let done_path = self.done_path.clone();
        let queue_path = queue_path
            .as_ref()
            .ok_or_else(|| anyhow!("queue path not resolved"))?;
        let done_path = done_path
            .as_ref()
            .ok_or_else(|| anyhow!("done path not resolved"))?;
        let id_prefix = self.id_prefix.clone();
        let id_width = self.id_width;

        let report = queue::repair_queue(queue_path, done_path, &id_prefix, id_width, dry_run)?;

        if report.is_empty() {
            self.set_status_message("No issues found. Queue is healthy.");
        } else {
            let mut parts = Vec::new();
            if report.fixed_tasks > 0 {
                parts.push(format!("{} tasks fixed", report.fixed_tasks));
            }
            if report.fixed_timestamps > 0 {
                parts.push(format!("{} timestamps fixed", report.fixed_timestamps));
            }
            if !report.remapped_ids.is_empty() {
                parts.push(format!("{} IDs remapped", report.remapped_ids.len()));
            }
            let msg = if dry_run {
                format!("Dry run: {}", parts.join(", "))
            } else {
                format!("Repaired: {}", parts.join(", "))
            };
            self.set_status_message(msg);

            // Reload queue if changes were made
            if !dry_run {
                self.queue = queue::load_queue_or_default(queue_path)?;
                self.done = queue::load_queue_or_default(done_path)?;
                self.dirty = false;
                self.dirty_done = false;
                self.bump_queue_rev();
                self.rebuild_filtered_view();
            }
        }

        Ok(())
    }

    /// Unlock the queue by removing the lock directory.
    pub fn unlock_queue(&mut self) -> Result<()> {
        // Derive repo root from queue_path
        let queue_path = self.queue_path.clone();
        let repo_root = queue_path
            .as_ref()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .ok_or_else(|| anyhow!("cannot determine repo root from queue path"))?;
        let lock_dir = lock::queue_lock_dir(repo_root);

        if lock_dir.exists() {
            std::fs::remove_dir_all(&lock_dir)
                .with_context(|| format!("remove lock dir {}", lock_dir.display()))?;
            self.set_status_message(format!("Queue unlocked (removed {})", lock_dir.display()));
        } else {
            self.set_status_message("Queue is not locked.");
        }

        Ok(())
    }

    /// Set the status of the selected task to a specific value.
    fn set_task_status(&mut self, status: &str, now_rfc3339: &str) {
        let Some(task_id) = self.selected_task().map(|t| t.id.clone()) else {
            self.set_status_message("No task selected");
            return;
        };

        if let Err(e) = self.apply_task_edit(TaskEditKey::Status, status, now_rfc3339) {
            self.set_status_message(format!("Error: {}", e));
            return;
        }

        self.set_status_message(format!("Set status to {}", status));

        // Check for auto-archive if terminal status
        if status == "done" || status == "rejected" {
            if let Err(e) = self.maybe_auto_archive(&task_id, now_rfc3339) {
                self.set_status_message(format!("Error: {}", e));
            }
        }
    }

    /// Set the priority of the selected task to a specific value.
    fn set_task_priority(&mut self, priority: &str, now_rfc3339: &str) {
        if self.selected_task().is_none() {
            self.set_status_message("No task selected");
            return;
        }

        if let Err(e) = self.apply_task_edit(TaskEditKey::Priority, priority, now_rfc3339) {
            self.set_status_message(format!("Error: {}", e));
        } else {
            self.set_status_message(format!("Set priority to {}", priority));
        }
    }

    /// Check if auto-archive should be triggered and handle it based on config.
    fn maybe_auto_archive(&mut self, task_id: &str, now_rfc3339: &str) -> Result<()> {
        use crate::contracts::AutoArchiveBehavior;

        let behavior = self
            .project_config
            .tui
            .auto_archive_terminal
            .unwrap_or_default();

        match behavior {
            AutoArchiveBehavior::Never => Ok(()),
            AutoArchiveBehavior::Always => {
                self.archive_single_task(task_id, now_rfc3339)?;
                self.set_status_message(format!("Archived {}", task_id));
                Ok(())
            }
            AutoArchiveBehavior::Prompt => {
                self.mode = AppMode::ConfirmAutoArchive(task_id.to_string());
                Ok(())
            }
        }
    }

    /// Cycle the active status filter.
    pub fn cycle_status_filter(&mut self) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        let next = match self.filters.statuses.as_slice() {
            [] => Some(TaskStatus::Todo),
            [TaskStatus::Todo] => Some(TaskStatus::Doing),
            [TaskStatus::Doing] => Some(TaskStatus::Done),
            [TaskStatus::Done] => Some(TaskStatus::Draft),
            [TaskStatus::Draft] => Some(TaskStatus::Rejected),
            [TaskStatus::Rejected] => None,
            _ => None,
        };

        self.filters.statuses = next.map(|status| vec![status]).unwrap_or_default();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    /// Set the tag filter list (empty to clear).
    pub fn set_tag_filters(&mut self, tags: Vec<String>) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.tags = tags;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    pub fn set_scope_filters(&mut self, scopes: Vec<String>) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.search_options.scopes = scopes;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    /// Set the search query (empty to clear).
    pub fn set_search_query(&mut self, query: String) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.query = query;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    pub(crate) fn begin_filter_input(&mut self) {
        if self.filter_snapshot.is_some() {
            return;
        }
        self.filter_snapshot = Some(FilterSnapshot {
            filters: self.filters.clone(),
            selected_task_id: self.selected_task().map(|task| task.id.clone()),
        });
    }

    pub(crate) fn commit_filter_input(&mut self) {
        self.filter_snapshot = None;
    }

    pub(crate) fn restore_filter_snapshot(&mut self) {
        let Some(snapshot) = self.filter_snapshot.take() else {
            return;
        };
        self.filters = snapshot.filters;
        self.rebuild_filtered_view_with_preferred(snapshot.selected_task_id.as_deref());
    }

    pub(crate) fn start_search_input(&mut self) {
        self.begin_filter_input();
        self.mode = AppMode::Searching(TextInput::new(self.filters.query.clone()));
    }

    pub(crate) fn start_filter_tags_input(&mut self) {
        self.begin_filter_input();
        self.mode = AppMode::FilteringTags(TextInput::new(self.filters.tags.join(",")));
    }

    pub(crate) fn start_filter_scopes_input(&mut self) {
        self.begin_filter_input();
        self.mode =
            AppMode::FilteringScopes(TextInput::new(self.filters.search_options.scopes.join(",")));
    }

    /// Clear all active filters (query, tags, status).
    pub fn clear_filters(&mut self) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters = FilterState::default();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    pub(crate) fn parse_list(input: &str) -> Vec<String> {
        input
            .split([',', '\n'])
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }

    /// Parse comma or whitespace-separated tags from input string.
    pub(crate) fn parse_tags(input: &str) -> Vec<String> {
        input
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
    }

    pub fn log_visible_lines(&self) -> usize {
        self.log_visible_lines.max(1)
    }

    pub fn set_log_visible_lines(&mut self, visible_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.log_visible_lines = visible_lines;
        let max_scroll = self.max_log_scroll(visible_lines);
        if self.autoscroll || self.log_scroll > max_scroll {
            self.log_scroll = max_scroll;
        }
    }

    pub fn max_log_scroll(&self, visible_lines: usize) -> usize {
        self.logs.len().saturating_sub(visible_lines)
    }

    pub fn scroll_logs_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        self.log_scroll = self.log_scroll.saturating_sub(lines);
    }

    pub fn scroll_logs_down(&mut self, lines: usize, visible_lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        let max_scroll = self.max_log_scroll(visible_lines);
        self.log_scroll = (self.log_scroll + lines).min(max_scroll);
    }

    pub fn enable_autoscroll(&mut self, visible_lines: usize) {
        self.autoscroll = true;
        self.log_scroll = self.max_log_scroll(visible_lines);
    }

    /// Get the current details scroll position.
    pub fn details_scroll(&self) -> usize {
        self.details.scroll()
    }

    /// Get mutable access to the details scroll state for rendering.
    pub fn details_scroll_state(&mut self) -> &mut tui_scrollview::ScrollViewState {
        self.details.scroll_state()
    }

    pub(crate) fn set_details_viewport(
        &mut self,
        visible_lines: usize,
        total_lines: usize,
        context: DetailsContext,
    ) {
        // Delegate to DetailsState which handles scroll reset on context change
        self.details
            .set_viewport(visible_lines, total_lines, context.clone());
        self.details_context = Some(context);
    }

    pub fn scroll_details_up(&mut self, lines: usize) {
        self.details.scroll_up(lines);
    }

    pub fn scroll_details_down(&mut self, lines: usize) {
        self.details.scroll_down(lines);
    }

    pub fn scroll_details_top(&mut self) {
        self.details.scroll_top();
    }

    pub fn scroll_details_bottom(&mut self) {
        self.details.scroll_bottom();
    }

    pub(crate) fn help_visible_lines(&self) -> usize {
        self.help_visible_lines.max(1)
    }

    pub(crate) fn help_total_lines(&self) -> usize {
        self.help_total_lines
    }

    pub(crate) fn help_scroll(&self) -> usize {
        self.help_scroll
    }

    pub(crate) fn set_help_visible_lines(&mut self, visible_lines: usize, total_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.help_visible_lines = visible_lines;
        self.help_total_lines = total_lines;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        if self.help_scroll > max_scroll {
            self.help_scroll = max_scroll;
        }
    }

    pub(crate) fn max_help_scroll(&self, total_lines: usize) -> usize {
        total_lines.saturating_sub(self.help_visible_lines())
    }

    pub(crate) fn scroll_help_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.help_scroll = self.help_scroll.saturating_sub(lines);
    }

    pub(crate) fn scroll_help_down(&mut self, lines: usize, total_lines: usize) {
        if lines == 0 {
            return;
        }
        let max_scroll = self.max_help_scroll(total_lines);
        self.help_scroll = (self.help_scroll + lines).min(max_scroll);
    }

    pub(crate) fn scroll_help_top(&mut self) {
        self.help_scroll = 0;
    }

    pub(crate) fn scroll_help_bottom(&mut self, total_lines: usize) {
        self.help_scroll = self.max_help_scroll(total_lines);
    }

    pub(crate) fn enter_help_mode(&mut self, previous_mode: AppMode) {
        self.help_previous_mode = Some(previous_mode);
        self.help_scroll = 0;
        self.mode = AppMode::Help;
    }

    pub(crate) fn exit_help_mode(&mut self) {
        let previous_mode = self.help_previous_mode.take().unwrap_or(AppMode::Normal);
        self.mode = previous_mode;
    }

    pub(crate) fn help_previous_mode(&self) -> Option<&AppMode> {
        self.help_previous_mode.as_ref()
    }

    /// Enter dependency graph overlay mode.
    pub(crate) fn enter_dependency_graph_mode(&mut self) {
        self.mode = AppMode::DependencyGraphOverlay {
            previous_mode: Box::new(self.mode.clone()),
            show_dependents: false,
            highlight_critical: false,
        };
    }

    /// Build the palette entries for a given query.
    pub fn palette_entries(&self, query: &str) -> Vec<PaletteEntry> {
        let toggle_label = if self.loop_active {
            "Stop loop"
        } else {
            "Start loop"
        };

        let entries = vec![
            PaletteEntry {
                cmd: PaletteCommand::RunSelected,
                title: "Run selected task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::RunNextRunnable,
                title: "Run next runnable task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleLoop,
                title: toggle_label.to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ArchiveTerminal,
                title: "Archive done/rejected tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::NewTask,
                title: "Create new task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::BuildTaskAgent,
                title: "Build task with agent".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::EditTask,
                title: "Edit selected task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::EditConfig,
                title: "Edit project config".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ScanRepo,
                title: "Scan repository for tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::Search,
                title: "Search tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::FilterTags,
                title: "Filter by tags".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::FilterScopes,
                title: "Filter by scope".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ClearFilters,
                title: "Clear filters".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::CycleStatus,
                title: "Cycle selected task status".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::CyclePriority,
                title: "Cycle selected task priority".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetStatusDraft,
                title: "Set status: Draft".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetStatusTodo,
                title: "Set status: Todo".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetStatusDoing,
                title: "Set status: Doing".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetStatusDone,
                title: "Set status: Done".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetStatusRejected,
                title: "Set status: Rejected".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetPriorityCritical,
                title: "Set priority: Critical".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetPriorityHigh,
                title: "Set priority: High".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetPriorityMedium,
                title: "Set priority: Medium".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::SetPriorityLow,
                title: "Set priority: Low".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleCaseSensitive,
                title: "Toggle case-sensitive search".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleRegex,
                title: "Toggle regex search".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleFuzzy,
                title: "Toggle fuzzy search".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ReloadQueue,
                title: "Reload queue from disk".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::MoveTaskUp,
                title: "Move selected task up".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::MoveTaskDown,
                title: "Move selected task down".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::JumpToTask,
                title: "Jump to task by ID".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::RepairQueue,
                title: "Repair queue".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::RepairQueueDryRun,
                title: "Repair queue (dry run)".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::UnlockQueue,
                title: "Unlock queue".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::Quit,
                title: "Quit".to_string(),
            },
        ];

        let q = query.trim();
        if q.is_empty() {
            return entries;
        }

        let q_lower = q.to_lowercase();

        // Score and filter entries using fuzzy matching
        let mut scored: Vec<ScoredPaletteEntry> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let score = score_palette_entry(&entry.title, &q_lower);
                ScoredPaletteEntry {
                    entry,
                    score,
                    original_index: idx,
                }
            })
            .filter(|s| s.score > 0)
            .collect();

        // Sort by score (desc), then title length (asc), then original index (asc)
        scored.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.entry.title.len().cmp(&b.entry.title.len()))
                .then_with(|| a.original_index.cmp(&b.original_index))
        });

        scored.into_iter().map(|s| s.entry).collect()
    }

    /// Execute a palette command (also used by direct keybinds for consistency).
    pub fn execute_palette_command(
        &mut self,
        cmd: PaletteCommand,
        now_rfc3339: &str,
    ) -> Result<TuiAction> {
        match cmd {
            PaletteCommand::RunSelected => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                    return Ok(TuiAction::Continue);
                }
                if self.loop_active {
                    self.loop_active = false;
                    self.loop_arm_after_current = false;
                    self.set_status_message("Loop stopped (manual run)");
                }
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };
                let task_id = task.id.clone();
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::RunNextRunnable => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                    return Ok(TuiAction::Continue);
                }
                let Some(task_id) = self.next_loop_task_id() else {
                    self.set_status_message("No runnable tasks");
                    return Ok(TuiAction::Continue);
                };
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::ToggleLoop => {
                if self.loop_active {
                    self.loop_active = false;
                    self.loop_arm_after_current = false;
                    self.set_status_message(format!("Loop stopped (ran {})", self.loop_ran));
                    return Ok(TuiAction::Continue);
                }

                self.loop_active = true;
                self.loop_ran = 0;

                if self.runner_active {
                    self.loop_arm_after_current = true;
                    self.set_status_message("Loop armed (will start after current task)");
                    return Ok(TuiAction::Continue);
                }

                let Some(task_id) = self.next_loop_task_id() else {
                    self.loop_active = false;
                    self.set_status_message("No runnable tasks");
                    return Ok(TuiAction::Continue);
                };

                self.set_status_message("Loop started");
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::ArchiveTerminal => {
                if self
                    .queue
                    .tasks
                    .iter()
                    .any(|t| matches!(t.status, TaskStatus::Done | TaskStatus::Rejected))
                {
                    self.mode = AppMode::ConfirmArchive;
                } else {
                    self.set_status_message("No done/rejected tasks to archive");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::NewTask => {
                self.mode = AppMode::CreatingTask(TextInput::new(""));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BuildTaskAgent => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                } else {
                    self.start_task_builder_options_flow();
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::EditTask => {
                if self.selected_task().is_some() {
                    self.mode = AppMode::EditingTask {
                        selected: 0,
                        editing_value: None,
                    };
                } else {
                    self.set_status_message("No task selected");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::EditConfig => {
                self.mode = AppMode::EditingConfig {
                    selected: 0,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ScanRepo => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                } else {
                    self.mode = AppMode::Scanning(TextInput::new(""));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::Search => {
                self.start_search_input();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterTags => {
                self.start_filter_tags_input();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterScopes => {
                self.start_filter_scopes_input();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ClearFilters => {
                self.clear_filters();
                self.set_status_message("Filters cleared");
                Ok(TuiAction::Continue)
            }
            PaletteCommand::CycleStatus => {
                if let Err(e) = self.cycle_status(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                } else {
                    self.set_status_message("Status updated");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::CyclePriority => {
                if let Err(e) = self.cycle_priority(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                } else {
                    self.set_status_message("Priority updated");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusDraft => {
                self.set_task_status("draft", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusTodo => {
                self.set_task_status("todo", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusDoing => {
                self.set_task_status("doing", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusDone => {
                self.set_task_status("done", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusRejected => {
                self.set_task_status("rejected", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityCritical => {
                self.set_task_priority("critical", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityHigh => {
                self.set_task_priority("high", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityMedium => {
                self.set_task_priority("medium", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityLow => {
                self.set_task_priority("low", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleCaseSensitive => {
                self.toggle_case_sensitive();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleRegex => {
                self.toggle_regex();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleFuzzy => {
                self.toggle_fuzzy();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ReloadQueue => {
                if self.unsafe_to_discard() {
                    self.mode = AppMode::ConfirmDiscard {
                        action: ConfirmDiscardAction::ReloadQueue,
                    };
                    Ok(TuiAction::Continue)
                } else {
                    Ok(TuiAction::ReloadQueue)
                }
            }
            PaletteCommand::MoveTaskUp => {
                if let Err(e) = self.move_task_up(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::MoveTaskDown => {
                if let Err(e) = self.move_task_down(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::JumpToTask => {
                self.mode = AppMode::JumpingToTask(TextInput::new(""));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::RepairQueue => {
                self.mode = AppMode::ConfirmRepair { dry_run: false };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::RepairQueueDryRun => {
                self.mode = AppMode::ConfirmRepair { dry_run: true };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::UnlockQueue => {
                self.mode = AppMode::ConfirmUnlock;
                Ok(TuiAction::Continue)
            }
            PaletteCommand::Quit => {
                if self.runner_active {
                    self.mode = AppMode::ConfirmQuit;
                    Ok(TuiAction::Continue)
                } else if self.unsafe_to_discard() {
                    self.mode = AppMode::ConfirmDiscard {
                        action: ConfirmDiscardAction::Quit,
                    };
                    Ok(TuiAction::Continue)
                } else {
                    Ok(TuiAction::Quit)
                }
            }
            PaletteCommand::ToggleMultiSelectMode => {
                self.toggle_multi_select_mode();
                if self.multi_select_mode {
                    self.set_status_message(
                        "Multi-select mode ON. Space: toggle, m: exit, a: archive, d: delete",
                    );
                } else {
                    self.set_status_message("Multi-select mode OFF");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleTaskSelection => {
                self.toggle_current_selection();
                let count = self.selection_count();
                self.set_status_message(format!("{} tasks selected", count));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchDelete => {
                let count = self.selection_count();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    self.mode = AppMode::ConfirmBatchDelete { count };
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchArchive => {
                let count = self.selection_count();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    self.mode = AppMode::ConfirmBatchArchive { count };
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchSetStatus(status) => {
                let indices: Vec<usize> = self.selected_indices.iter().copied().collect();
                let count = indices.len();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    // Convert filtered indices to queue indices and set status
                    let queue_indices: Vec<usize> = indices
                        .iter()
                        .filter_map(|&filtered_idx| {
                            self.filtered_indices.get(filtered_idx).copied()
                        })
                        .collect();
                    for idx in queue_indices {
                        if let Some(task) = self.queue.tasks.get_mut(idx) {
                            task.status = status;
                            task.updated_at = Some(now_rfc3339.to_string());
                        }
                    }
                    self.dirty = true;
                    self.bump_queue_rev();
                    self.set_status_message(format!(
                        "Set status to {:?} for {} tasks",
                        status, count
                    ));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ClearSelection => {
                self.clear_selection();
                self.set_status_message("Selection cleared");
                Ok(TuiAction::Continue)
            }
        }
    }

    /// Start execution of a specific task.
    pub(crate) fn start_task_execution(
        &mut self,
        task_id: String,
        focus_logs: bool,
        append_logs: bool,
    ) {
        if append_logs && !self.logs.is_empty() {
            self.logs.push(String::new());
            self.logs.push(format!("=== Executing {} ===", task_id));
            // Also add to ANSI buffer for terminal emulation
            self.log_ansi_buffer.push(b'\n');
            self.log_ansi_buffer
                .extend_from_slice(format!("=== Executing {} ===", task_id).as_bytes());
            self.log_ansi_buffer.push(b'\n');
        } else {
            self.logs.clear();
            self.log_ansi_buffer.clear();
        }

        self.log_scroll = 0;
        self.autoscroll = true;

        self.runner_active = true;
        self.running_task_id = Some(task_id.clone());
        self.running_kind = Some(RunningKind::Task);

        // Initialize phase tracking for task execution
        let phases = self.project_config.agent.phases.unwrap_or(3);
        self.reset_phase_tracking(phases);

        // Initialize ETA calculator
        self.current_eta = None;

        if focus_logs {
            self.mode = AppMode::Executing { task_id };
        }
    }

    /// Start execution of a scan.
    pub(crate) fn start_scan_execution(
        &mut self,
        focus: String,
        focus_logs: bool,
        append_logs: bool,
    ) {
        let label = scan_label(&focus);
        if append_logs && !self.logs.is_empty() {
            self.logs.push(String::new());
            self.logs.push(format!("=== {} ===", label));
            // Also add to ANSI buffer for terminal emulation
            self.log_ansi_buffer.push(b'\n');
            self.log_ansi_buffer
                .extend_from_slice(format!("=== {} ===", label).as_bytes());
            self.log_ansi_buffer.push(b'\n');
        } else {
            self.logs.clear();
            self.log_ansi_buffer.clear();
        }

        self.log_scroll = 0;
        self.autoscroll = true;

        self.runner_active = true;
        self.running_task_id = Some(label);
        self.running_kind = Some(RunningKind::Scan {
            focus: focus.clone(),
        });

        if self.loop_active {
            self.loop_active = false;
            self.loop_arm_after_current = false;
            self.set_status_message("Loop stopped (scan run)");
        }

        if focus_logs {
            self.mode = AppMode::Executing { task_id: focus };
        }
    }

    /// Start execution of the task builder agent.
    pub(crate) fn start_task_builder_execution(&mut self, request: String) {
        self.logs.clear();
        self.log_ansi_buffer.clear();
        let header = format!("=== Building task from: {} ===", request);
        self.logs.push(header.clone());
        self.log_ansi_buffer.extend_from_slice(header.as_bytes());
        self.log_ansi_buffer.push(b'\n');
        self.log_scroll = 0;
        self.autoscroll = true;
        self.set_status_message("Starting task builder...");

        self.runner_active = true;
        self.running_task_id = Some("Task Builder".to_string());
        self.running_kind = Some(RunningKind::TaskBuilder);
        self.mode = AppMode::Executing {
            task_id: "Task Builder".to_string(),
        };
    }

    /// Start the advanced task builder flow with override options.
    pub(crate) fn start_task_builder_options_flow(&mut self) {
        let state = TaskBuilderState {
            step: TaskBuilderStep::Description,
            description: String::new(),
            description_input: TextInput::new(""),
            tags_hint: String::new(),
            scope_hint: String::new(),
            runner_override: None,
            model_override_input: String::new(),
            effort_override: None,
            repoprompt_mode: None,
            selected_field: 0,
            error_message: None,
        };
        self.mode = AppMode::BuildingTaskOptions(state);
    }

    /// Select the next runnable task for loop mode.
    ///
    /// This prefers resuming `doing` tasks, then the first runnable `todo`, then `draft` (when
    /// enabled), while skipping tasks whose dependencies are not met.
    pub fn next_loop_task_id(&self) -> Option<String> {
        let options =
            queue::operations::RunnableSelectionOptions::new(self.loop_include_draft, true);
        queue::operations::select_runnable_task_index(&self.queue, Some(&self.done), options)
            .and_then(|idx| self.queue.tasks.get(idx).map(|task| task.id.clone()))
    }

    /// Rebuild the filtered view.
    pub(crate) fn rebuild_filtered_view(&mut self) {
        self.rebuild_filtered_view_with_preferred(None);
    }

    pub(crate) fn rebuild_filtered_view_with_preferred(&mut self, preferred_id: Option<&str>) {
        self.ensure_filtered_indices();

        if let Some(preferred_id) = preferred_id {
            if let Some(new_pos) =
                self.filtered_indices
                    .iter()
                    .enumerate()
                    .find_map(|(pos, &idx)| {
                        self.queue
                            .tasks
                            .get(idx)
                            .and_then(|task| (task.id == preferred_id).then_some(pos))
                    })
            {
                self.selected = new_pos;
                self.clamp_selection_and_scroll();
                return;
            }
            self.selected = 0;
        }

        self.clamp_selection_and_scroll();

        // Update board columns if in board view
        self.update_board_columns();
    }

    fn clamp_selection_and_scroll(&mut self) {
        if self.filtered_indices.is_empty() {
            self.selected = 0;
            self.scroll = 0;
            return;
        }

        if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len().saturating_sub(1);
        }

        if self.scroll > self.selected {
            self.scroll = self.selected;
        }

        let list_height = self.list_height.max(1);
        if self.selected >= self.scroll + list_height {
            self.scroll = self.selected.saturating_sub(list_height.saturating_sub(1));
        }
    }

    /// Reload the queue + done archive from disk.
    pub(crate) fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());

        match queue::load_queue(queue_path) {
            Ok(new_queue) => {
                self.queue = new_queue;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error: {}", e));
                return;
            }
        }

        match queue::load_queue_or_default(done_path) {
            Ok(new_done) => {
                self.done = new_done;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error (done): {}", e));
                return;
            }
        }

        self.bump_queue_rev();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        self.dirty = false;
        self.dirty_done = false;
        self.save_error = None;
    }

    /// Check if queue files have been modified externally and reload if necessary.
    ///
    /// Returns true if external changes were detected and reloaded.
    pub(crate) fn check_external_changes_and_reload(
        &mut self,
        queue_path: &Path,
        done_path: &Path,
    ) -> bool {
        let queue_current = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        let done_current = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());

        let queue_changed = queue_current != self.queue_mtime;
        let done_changed = done_current != self.done_mtime;

        if queue_changed || done_changed {
            self.reload_queues_from_disk(queue_path, done_path);
            self.queue_mtime = queue_current;
            self.done_mtime = done_current;
            self.set_status_message("External changes detected - reloaded".to_string());
            true
        } else {
            false
        }
    }

    /// Update cached mtimes after save operations.
    pub(crate) fn update_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path) {
        self.queue_mtime = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.done_mtime = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());
    }

    /// Handle scan completion: reload queue, set status, and return to normal mode.
    pub(crate) fn on_scan_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Scan completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle task builder completion: reload queue, set status, and return to normal mode.
    pub(crate) fn on_task_builder_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Task builder completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle scan error: set error message and return to normal mode.
    pub(crate) fn on_scan_error(&mut self, msg: &str) {
        self.set_status_message(format!("Scan error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle task builder error: set error message and return to normal mode.
    pub(crate) fn on_task_builder_error(&mut self, msg: &str) {
        self.set_status_message(format!("Task builder error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    // View mode switching methods

    /// Switch to list view.
    ///
    /// Updates the view mode and syncs the list selection to match
    /// the currently selected board task (if any).
    pub(crate) fn switch_to_list_view(&mut self) {
        if self.view_mode == ViewMode::List {
            return;
        }
        self.view_mode = ViewMode::List;
        // Sync board selection back to list
        self.sync_board_selection_to_list();
        self.set_status_message("Switched to list view (l)");
    }

    /// Switch to board (Kanban) view.
    ///
    /// Updates the view mode, rebuilds the column task mapping,
    /// and syncs the board selection to match the current list selection.
    pub(crate) fn switch_to_board_view(&mut self) {
        if self.view_mode == ViewMode::Board {
            return;
        }
        self.view_mode = ViewMode::Board;
        // Rebuild column mapping from current filtered view
        self.board_nav
            .update_columns(&self.filtered_indices, &self.queue);
        // Sync list selection to board
        self.sync_list_selection_to_board();
        self.set_status_message("Switched to board view (b)");
    }

    /// Sync board navigation selection to list selection.
    ///
    /// Updates the list view's selected index to match the currently
    /// selected task in the board view.
    pub(crate) fn sync_board_selection_to_list(&mut self) {
        if let Some(queue_index) = self.board_nav.selected_task_index() {
            // Find the position of this task in the filtered indices
            if let Some(filtered_pos) = self
                .filtered_indices
                .iter()
                .position(|&idx| idx == queue_index)
            {
                self.selected = filtered_pos;
                self.clamp_selection_and_scroll();
            }
        }
    }

    /// Sync list selection to board navigation.
    ///
    /// Updates the board view's selected column and task to match
    /// the currently selected task in the list view.
    pub(crate) fn sync_list_selection_to_board(&mut self) {
        if let Some(queue_index) = self.filtered_indices.get(self.selected).copied() {
            self.board_nav.select_task(queue_index, &self.queue);
        }
    }

    /// Update board column tasks when filters change.
    ///
    /// Should be called after rebuild_filtered_view to keep the board
    /// in sync with the current filter state.
    pub(crate) fn update_board_columns(&mut self) {
        if self.view_mode == ViewMode::Board {
            self.board_nav
                .update_columns(&self.filtered_indices, &self.queue);
        }
    }
}

pub(crate) fn auto_save_if_dirty(
    app: &mut App,
    queue_path: &Path,
    done_path: &Path,
    project_config_path: Option<&Path>,
) {
    let mut errors: Vec<String> = Vec::new();

    if app.dirty {
        match queue::save_queue(queue_path, &app.queue) {
            Ok(()) => {
                app.dirty = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving queue: {}", e));
            }
        }
    }

    if app.dirty_done {
        match queue::save_queue(done_path, &app.done) {
            Ok(()) => {
                app.dirty_done = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving done: {}", e));
            }
        }
    }

    if app.dirty_config {
        match project_config_path {
            Some(path) => match crate_config::save_layer(path, &app.project_config) {
                Ok(()) => {
                    app.dirty_config = false;
                }
                Err(e) => {
                    errors.push(format!("ERROR saving config: {}", e));
                }
            },
            None => {
                errors.push("ERROR saving config: missing project config_path".to_string());
            }
        }
    }

    if errors.is_empty() {
        app.save_error = None;
        // Update cached mtimes to avoid triggering external change detection
        // for our own saves
        app.update_cached_mtimes(queue_path, done_path);
        return;
    }

    let message = errors.join(" | ");
    let should_log = app.save_error.as_deref() != Some(message.as_str());
    app.save_error = Some(message.clone());
    if should_log {
        app.set_status_message(message);
    }
}

/// Event sent from the runner thread to the TUI.
pub(crate) enum RunnerEvent {
    /// Output chunk received
    Output(String),
    /// Task finished (success)
    Finished,
    /// Task failed with error
    Error(String),
    /// Revert prompt requested by the runner.
    RevertPrompt {
        label: String,
        preface: Option<String>,
        allow_proceed: bool,
        reply: mpsc::Sender<runutil::RevertDecision>,
    },
}

/// Run the TUI application with an active queue lock.
pub fn run_tui<F, E, S, SE>(
    resolved: &crate::config::Resolved,
    force_lock: bool,
    options: TuiOptions,
    runner_factory: F,
    scan_factory: S,
) -> Result<Option<String>>
where
    F: Fn(String, crate::runner::OutputHandler, runutil::RevertPromptHandler) -> E
        + Send
        + Sync
        + 'static,
    E: FnOnce() -> Result<()> + Send + 'static,
    S: Fn(String, crate::runner::OutputHandler, runutil::RevertPromptHandler) -> SE
        + Send
        + Sync
        + 'static,
    SE: FnOnce() -> Result<()> + Send + 'static,
{
    let (mut app, _queue_lock) = prepare_tui_session(resolved, force_lock)?;
    let queue_path = &resolved.queue_path;
    let done_path = &resolved.done_path;

    // Apply boot options.
    app.loop_max_tasks = options.loop_max_tasks;
    app.loop_include_draft = options.loop_include_draft;

    // Show flowchart on start if requested.
    if options.show_flowchart {
        app.mode = AppMode::FlowchartOverlay {
            previous_mode: Box::new(AppMode::Normal),
        };
    }

    // Detect terminal capabilities.
    let capabilities = TerminalCapabilities::detect();
    let color_support = options.color.resolve(capabilities.colors);
    let enable_mouse = !options.no_mouse && capabilities.has_mouse();
    let border_style = BorderStyle::for_capabilities(capabilities, options.ascii_borders);

    // Store capabilities in app for render-time decisions.
    app.terminal_capabilities = Some(capabilities);
    app.color_support = Some(color_support);
    app.border_style = border_style;

    // Setup terminal.
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    if enable_mouse {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("enter alternate screen with mouse")?;
    } else {
        execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    // Create channels for runner events.
    let (tx, rx) = mpsc::channel::<RunnerEvent>();

    let build_handlers = |tx: &mpsc::Sender<RunnerEvent>| {
        let tx_clone_for_handler = tx.clone();
        let tx_clone_for_prompt = tx.clone();
        let handler: crate::runner::OutputHandler = Arc::new(Box::new(move |text: &str| {
            let _ = tx_clone_for_handler.send(RunnerEvent::Output(text.to_string()));
        }));

        let revert_prompt: runutil::RevertPromptHandler =
            Arc::new(move |context: &runutil::RevertPromptContext| {
                let (reply_tx, reply_rx) = mpsc::channel();
                if tx_clone_for_prompt
                    .send(RunnerEvent::RevertPrompt {
                        label: context.label.clone(),
                        preface: context.preface.clone(),
                        allow_proceed: context.allow_proceed,
                        reply: reply_tx,
                    })
                    .is_err()
                {
                    return runutil::RevertDecision::Keep;
                }
                reply_rx.recv().unwrap_or(runutil::RevertDecision::Keep)
            });

        (handler, revert_prompt)
    };

    // Helper to spawn task runner work.
    let spawn_task = |task_id: String, tx: mpsc::Sender<RunnerEvent>| {
        let tx_clone = tx.clone();
        let (handler, revert_prompt) = build_handlers(&tx);

        let runner_fn = runner_factory(task_id.clone(), handler, revert_prompt);
        thread::spawn(move || match runner_fn() {
            Ok(()) => {
                let _ = tx_clone.send(RunnerEvent::Finished);
            }
            Err(e) => {
                let _ = tx_clone.send(RunnerEvent::Error(e.to_string()));
            }
        });
    };

    // Helper to spawn scan runner work.
    let spawn_scan = |focus: String, tx: mpsc::Sender<RunnerEvent>| {
        let tx_clone = tx.clone();
        let (handler, revert_prompt) = build_handlers(&tx);

        let runner_fn = scan_factory(focus.clone(), handler, revert_prompt);
        thread::spawn(move || match runner_fn() {
            Ok(()) => {
                let _ = tx_clone.send(RunnerEvent::Finished);
            }
            Err(e) => {
                let _ = tx_clone.send(RunnerEvent::Error(e.to_string()));
            }
        });
    };

    // Helper to spawn task builder work.
    let spawn_task_builder = |opts: crate::commands::task::TaskBuildOptions,
                              repoprompt_mode: Option<crate::agent::RepoPromptMode>,
                              tx: mpsc::Sender<RunnerEvent>| {
        let tx_clone = tx.clone();
        thread::spawn(move || {
            let result = || -> Result<()> {
                let resolved = crate_config::resolve_from_cwd()?;
                // Determine repoprompt_tool_injection based on mode
                let repoprompt_tool_injection = match repoprompt_mode {
                    Some(crate::agent::RepoPromptMode::Tools) => true,
                    Some(crate::agent::RepoPromptMode::Plan) => true,
                    Some(crate::agent::RepoPromptMode::Off) => false,
                    None => crate::agent::resolve_repoprompt_flags(None, &resolved).tool_injection,
                };
                let opts_with_injection = crate::commands::task::TaskBuildOptions {
                    repoprompt_tool_injection,
                    ..opts
                };
                crate::commands::task::build_task_without_lock(&resolved, opts_with_injection)?;
                Ok(())
            }();

            match result {
                Ok(()) => {
                    let _ = tx_clone.send(RunnerEvent::Output(
                        "Task builder completed successfully".to_string(),
                    ));
                    let _ = tx_clone.send(RunnerEvent::Finished);
                }
                Err(e) => {
                    let _ = tx_clone.send(RunnerEvent::Error(e.to_string()));
                }
            }
        });
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        use std::cell::RefCell;
        let app = RefCell::new(app);

        // Auto-start loop if requested.
        let mut initial_start: Option<String> = None;
        if options.start_loop {
            let mut app_ref = app.borrow_mut();
            app_ref.loop_active = true;
            app_ref.loop_ran = 0;
            if !app_ref.runner_active {
                if let Some(id) = app_ref.next_loop_task_id() {
                    app_ref.start_task_execution(id.clone(), true, false);
                    initial_start = Some(id);
                } else {
                    app_ref.loop_active = false;
                    app_ref.set_status_message("No runnable tasks");
                }
            }
        }
        if let Some(id) = initial_start {
            spawn_task(id, tx.clone());
        }

        let handle_action = |action: TuiAction, app_ref: &mut App| -> Result<bool> {
            match action {
                TuiAction::Quit => Ok(true),
                TuiAction::Continue => Ok(false),
                TuiAction::ReloadQueue => {
                    app_ref.reload_queues_from_disk(queue_path, done_path);
                    Ok(false)
                }
                TuiAction::RunTask(task_id) => {
                    let tx_clone = tx.clone();
                    spawn_task(task_id, tx_clone);
                    Ok(false)
                }
                TuiAction::RunScan(focus) => {
                    app_ref.start_scan_execution(focus.clone(), true, false);
                    let tx_clone = tx.clone();
                    spawn_scan(focus, tx_clone);
                    Ok(false)
                }
                TuiAction::BuildTask(request) => {
                    if app_ref.runner_active {
                        app_ref.set_status_message("Runner already active");
                    } else {
                        app_ref.start_task_builder_execution(request.clone());
                        let tx_clone = tx.clone();
                        let opts = crate::commands::task::TaskBuildOptions {
                            request,
                            hint_tags: String::new(),
                            hint_scope: String::new(),
                            runner_override: None,
                            model_override: None,
                            reasoning_effort_override: None,
                            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(
                            ),
                            force: false,
                            repoprompt_tool_injection: false,
                            template_hint: None,
                            template_target: None,
                        };
                        spawn_task_builder(opts, None, tx_clone);
                    }
                    Ok(false)
                }
                TuiAction::BuildTaskWithOptions(options) => {
                    if app_ref.runner_active {
                        app_ref.set_status_message("Runner already active");
                    } else {
                        app_ref.start_task_builder_execution(options.request.clone());
                        let tx_clone = tx.clone();
                        let opts = crate::commands::task::TaskBuildOptions {
                            request: options.request,
                            hint_tags: options.hint_tags,
                            hint_scope: options.hint_scope,
                            runner_override: options.runner_override,
                            model_override: options.model_override,
                            reasoning_effort_override: options.reasoning_effort_override,
                            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(
                            ),
                            force: false,
                            repoprompt_tool_injection: false,
                            template_hint: None,
                            template_target: None,
                        };
                        spawn_task_builder(opts, options.repoprompt_mode, tx_clone);
                    }
                    Ok(false)
                }
            }
        };

        // Main event loop.
        loop {
            // Check for external changes before drawing
            {
                let mut app_ref = app.borrow_mut();
                let _ = app_ref.check_external_changes_and_reload(queue_path, done_path);
            }

            terminal
                .draw(|f| {
                    let mut app_ref = app.borrow_mut();
                    app_ref.detail_width = f.area().width.saturating_sub(4);
                    draw_ui(f, &mut app_ref)
                })
                .context("draw UI")?;

            // Process runner events.
            let mut next_to_start: Option<String> = None;

            while let Ok(event) = rx.try_recv() {
                let mut app_ref = app.borrow_mut();
                match event {
                    RunnerEvent::Output(text) => {
                        let lines: Vec<String> =
                            text.lines().map(|line| line.to_string()).collect();
                        // Process each line for phase detection
                        if app_ref.running_kind == Some(RunningKind::Task) {
                            for line in &lines {
                                app_ref.process_log_line_for_phase(line);
                            }
                        }
                        app_ref.append_log_lines(lines);
                    }
                    RunnerEvent::Finished => {
                        app_ref.runner_active = false;
                        app_ref.running_task_id = None;
                        // Mark execution as complete for phase tracking
                        if app_ref.running_kind == Some(RunningKind::Task) {
                            app_ref.transition_to_phase(ExecutionPhase::Complete);
                        }
                        let running_kind = app_ref.running_kind.take();

                        match running_kind {
                            Some(RunningKind::Scan { .. }) => {
                                app_ref.on_scan_finished(queue_path, done_path);
                            }
                            Some(RunningKind::TaskBuilder) => {
                                app_ref.on_task_builder_finished(queue_path, done_path);
                            }
                            Some(RunningKind::Task) | None => {
                                // Record execution history for completed task
                                app_ref.record_execution_history();

                                app_ref.reload_queues_from_disk(queue_path, done_path);

                                if app_ref.mode == AppMode::ConfirmQuit {
                                    app_ref.mode = AppMode::Normal;
                                }

                                if app_ref.loop_active {
                                    if app_ref.loop_arm_after_current {
                                        app_ref.loop_arm_after_current = false;
                                    } else {
                                        app_ref.loop_ran = app_ref.loop_ran.saturating_add(1);
                                    }

                                    if let Some(max) = app_ref.loop_max_tasks {
                                        if app_ref.loop_ran >= max {
                                            let loop_ran = app_ref.loop_ran;
                                            app_ref.loop_active = false;
                                            app_ref.set_status_message(format!(
                                                "Loop finished (ran {}/{})",
                                                loop_ran, max
                                            ));
                                        }
                                    }

                                    if app_ref.loop_active {
                                        if let Some(next_id) = app_ref.next_loop_task_id() {
                                            let focus_logs =
                                                matches!(app_ref.mode, AppMode::Executing { .. });
                                            app_ref.start_task_execution(
                                                next_id.clone(),
                                                focus_logs,
                                                true,
                                            );
                                            next_to_start = Some(next_id);
                                        } else {
                                            let loop_ran = app_ref.loop_ran;
                                            app_ref.loop_active = false;
                                            app_ref.set_status_message(format!(
                                                "Loop complete (ran {})",
                                                loop_ran
                                            ));
                                        }
                                    }
                                } else if matches!(
                                    app_ref.mode,
                                    AppMode::Executing { .. } | AppMode::ConfirmQuit
                                ) {
                                    app_ref.mode = AppMode::Normal;
                                }
                            }
                        }
                    }
                    RunnerEvent::Error(msg) => {
                        app_ref.runner_active = false;
                        app_ref.running_task_id = None;
                        let running_kind = app_ref.running_kind.take();

                        app_ref.loop_active = false;
                        app_ref.loop_arm_after_current = false;

                        match running_kind {
                            Some(RunningKind::Scan { .. }) => {
                                app_ref.on_scan_error(&msg);
                            }
                            Some(RunningKind::TaskBuilder) => {
                                app_ref.on_task_builder_error(&msg);
                            }
                            Some(RunningKind::Task) | None => {
                                app_ref.set_runner_error(&msg);
                                if matches!(
                                    app_ref.mode,
                                    AppMode::Executing { .. } | AppMode::ConfirmQuit
                                ) {
                                    app_ref.mode = AppMode::Normal;
                                }
                            }
                        }
                    }
                    RunnerEvent::RevertPrompt {
                        label,
                        preface,
                        allow_proceed,
                        reply,
                    } => {
                        let previous_mode = app_ref.mode.clone();
                        app_ref.mode = AppMode::ConfirmRevert {
                            label,
                            preface,
                            allow_proceed,
                            selected: 0,
                            input: TextInput::new(""),
                            reply_sender: reply,
                            previous_mode: Box::new(previous_mode),
                        };
                    }
                }
            }

            if let Some(id) = next_to_start {
                spawn_task(id, tx.clone());
            }

            // Update spinner animation for progress indication.
            {
                let mut app_ref = app.borrow_mut();
                if app_ref.runner_active {
                    app_ref.tick_spinner();
                }
            }

            // Auto-save if dirty.
            if app.borrow().dirty || app.borrow().dirty_done || app.borrow().dirty_config {
                let mut app_ref = app.borrow_mut();
                let config_path = app_ref.project_config_path.clone();
                auto_save_if_dirty(&mut app_ref, queue_path, done_path, config_path.as_deref());
            }

            // Handle input events with reduced timeout for more responsive resize.
            if event::poll(Duration::from_millis(50)).context("poll event")? {
                let event = event::read().context("read event")?;
                let mut should_quit = false;
                let mut should_redraw = false;
                match event {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Release {
                            continue;
                        }

                        let mut app_ref = app.borrow_mut();
                        let now = timeutil::now_utc_rfc3339()?;
                        let action = handle_key_event(&mut app_ref, key, &now)?;
                        should_quit = handle_action(action, &mut app_ref)?;
                    }
                    Event::Mouse(mouse) => {
                        let mut app_ref = app.borrow_mut();
                        let action = handle_mouse_event(&mut app_ref, mouse)?;
                        should_quit = handle_action(action, &mut app_ref)?;
                    }
                    Event::Resize(width, height) => {
                        let mut app_ref = app.borrow_mut();
                        app_ref.handle_resize(width, height);
                        // Trigger immediate redraw to prevent visual glitches
                        should_redraw = true;
                    }
                    Event::Paste(_) => {
                        // Explicitly ignore paste events for now.
                        // Future enhancement: support paste in text input modes.
                    }
                    Event::FocusGained | Event::FocusLost => {
                        // Explicitly ignore focus events.
                    }
                }
                if should_quit {
                    break;
                }
                // Force immediate redraw on resize to prevent visual artifacts
                if should_redraw {
                    terminal
                        .draw(|f| {
                            let mut app_ref = app.borrow_mut();
                            // Update detail width from current frame area
                            app_ref.detail_width = f.area().width.saturating_sub(4);
                            draw_ui(f, &mut app_ref)
                        })
                        .context("draw UI on resize")?;
                }
            }
        }

        Ok::<_, anyhow::Error>(None)
    }));

    // Cleanup terminal.
    let _ = disable_raw_mode();
    let backend = terminal.backend_mut();
    let _ = execute!(backend, LeaveAlternateScreen);
    if enable_mouse {
        let _ = execute!(backend, DisableMouseCapture);
    }
    let _ = terminal.show_cursor();

    match result {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(e)) => Err(e),
        Err(_) => bail!("TUI panicked"),
    }
}

/// Acquire the queue lock and load the queue for TUI usage.
pub fn prepare_tui_session(
    resolved: &crate::config::Resolved,
    force_lock: bool,
) -> Result<(App, lock::DirLock)> {
    let lock = queue::acquire_queue_lock(&resolved.repo_root, "tui", force_lock)?;
    let (queue, done) = queue::load_and_validate_queues(resolved, true)?;
    let mut app = App::new(queue);
    app.done = done.unwrap_or_default();
    app.id_prefix = resolved.id_prefix.clone();
    app.id_width = resolved.id_width;
    app.queue_path = Some(resolved.queue_path.clone());
    app.done_path = Some(resolved.done_path.clone());

    let mut project_config = ConfigLayer::default();
    let mut project_config_path = None;
    if let Some(path) = resolved.project_config_path.as_ref() {
        project_config_path = Some(path.clone());
        if path.exists() {
            project_config = crate_config::load_layer(path)
                .with_context(|| format!("load project config {}", path.display()))?;
        }
    }
    app.project_config = project_config;
    app.project_config_path = project_config_path;

    // Initialize cached mtimes for external change detection
    app.queue_mtime = std::fs::metadata(&resolved.queue_path)
        .ok()
        .and_then(|m| m.modified().ok());
    app.done_mtime = std::fs::metadata(&resolved.done_path)
        .ok()
        .and_then(|m| m.modified().ok());

    Ok((app, lock))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};

    fn create_test_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            status,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
        }
    }

    fn create_test_app_with_tasks() -> App {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                create_test_task("RQ-0001", TaskStatus::Todo),
                create_test_task("RQ-0002", TaskStatus::Doing),
                create_test_task("RQ-0003", TaskStatus::Done),
                create_test_task("RQ-0004", TaskStatus::Todo),
                create_test_task("RQ-0005", TaskStatus::Rejected),
            ],
        };
        App::new(queue)
    }

    #[test]
    fn test_multi_select_mode_toggle() {
        let mut app = create_test_app_with_tasks();

        // Initially off
        assert!(!app.multi_select_mode);
        assert!(app.selected_indices.is_empty());

        // Toggle on
        app.toggle_multi_select_mode();
        assert!(app.multi_select_mode);

        // Toggle off clears selection
        app.selected_indices.insert(0);
        app.selected_indices.insert(2);
        app.toggle_multi_select_mode();
        assert!(!app.multi_select_mode);
        assert!(app.selected_indices.is_empty());
    }

    #[test]
    fn test_toggle_current_selection() {
        let mut app = create_test_app_with_tasks();

        // Enable multi-select mode
        app.toggle_multi_select_mode();
        app.selected = 1; // Select second task

        // Toggle selection on
        app.toggle_current_selection();
        assert!(app.is_selected(1));
        assert_eq!(app.selection_count(), 1);

        // Toggle selection off
        app.toggle_current_selection();
        assert!(!app.is_selected(1));
        assert_eq!(app.selection_count(), 0);
    }

    #[test]
    fn test_toggle_current_selection_no_op_when_not_in_multi_select() {
        let mut app = create_test_app_with_tasks();

        // Not in multi-select mode
        assert!(!app.multi_select_mode);
        app.selected = 1;

        // Should be no-op
        app.toggle_current_selection();
        assert!(!app.is_selected(1));
        assert_eq!(app.selection_count(), 0);
    }

    #[test]
    fn test_clear_selection() {
        let mut app = create_test_app_with_tasks();

        app.toggle_multi_select_mode();
        app.selected_indices.insert(0);
        app.selected_indices.insert(2);
        app.selected_indices.insert(4);

        assert_eq!(app.selection_count(), 3);
        assert!(app.multi_select_mode);

        app.clear_selection();

        assert!(app.selected_indices.is_empty());
        assert!(!app.multi_select_mode);
    }

    #[test]
    fn test_batch_delete_by_filtered_indices() {
        let mut app = create_test_app_with_tasks();
        let initial_count = app.queue.tasks.len();

        // Select tasks at filtered positions 0 and 2
        let deleted = app.batch_delete_by_filtered_indices(&[0, 2]).unwrap();

        assert_eq!(deleted, 2);
        assert_eq!(app.queue.tasks.len(), initial_count - 2);
        assert!(app.dirty);
    }

    #[test]
    fn test_batch_delete_empty_selection() {
        let mut app = create_test_app_with_tasks();
        let initial_count = app.queue.tasks.len();

        let deleted = app.batch_delete_by_filtered_indices(&[]).unwrap();

        assert_eq!(deleted, 0);
        assert_eq!(app.queue.tasks.len(), initial_count);
    }

    #[test]
    fn test_batch_archive_by_filtered_indices() {
        let mut app = create_test_app_with_tasks();
        let initial_queue_count = app.queue.tasks.len();
        let initial_done_count = app.done.tasks.len();

        // Select tasks at filtered positions 1 and 3
        let archived = app
            .batch_archive_by_filtered_indices(&[1, 3], "2024-01-01T00:00:00Z")
            .unwrap();

        assert_eq!(archived, 2);
        assert_eq!(app.queue.tasks.len(), initial_queue_count - 2);
        assert_eq!(app.done.tasks.len(), initial_done_count + 2);
        assert!(app.dirty);
        assert!(app.dirty_done);
        // Selection should be cleared after archive
        assert!(app.selected_indices.is_empty());
        assert!(!app.multi_select_mode);
    }

    #[test]
    fn test_batch_archive_empty_selection() {
        let mut app = create_test_app_with_tasks();
        let initial_queue_count = app.queue.tasks.len();

        let archived = app
            .batch_archive_by_filtered_indices(&[], "2024-01-01T00:00:00Z")
            .unwrap();

        assert_eq!(archived, 0);
        assert_eq!(app.queue.tasks.len(), initial_queue_count);
    }

    #[test]
    fn test_selection_persists_across_filter_changes() {
        let mut app = create_test_app_with_tasks();

        app.toggle_multi_select_mode();
        app.selected = 1;
        app.toggle_current_selection();
        app.selected = 3;
        app.toggle_current_selection();

        assert_eq!(app.selection_count(), 2);
        assert!(app.is_selected(1));
        assert!(app.is_selected(3));

        // Change filters (this rebuilds filtered view)
        app.clear_filters();

        // Selection indices are preserved (they refer to filtered positions)
        assert_eq!(app.selection_count(), 2);
    }
}
