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
use anyhow::{Context, Result, anyhow, bail};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use super::TextInput;
use super::events::{
    AppMode, ConfirmDiscardAction, PaletteCommand, PaletteEntry, TaskBuilderState, TaskBuilderStep,
    TuiAction, ViewMode, handle_key_event, handle_mouse_event,
};
use super::render::draw_ui;
use super::terminal::{BorderStyle, ColorSupport, TerminalCapabilities};
use super::{DetailsContext, DetailsState};
use crate::tui::app_execution::RunningKind;
use crate::tui::app_filters::FilterManagementOperations;
use crate::tui::app_filters::FilterOperations;
use crate::tui::app_filters::{FilterKey, FilterSnapshot, FilterState};
use crate::tui::app_logs::LogOperations;
use crate::tui::app_multi_select::MultiSelectOperations;
use crate::tui::app_navigation::BoardNavigationState;
#[cfg(test)]
use crate::tui::app_options::FilterCacheStats;
use crate::tui::app_options::TuiOptions;
use crate::tui::app_palette::scan_label;
use crate::tui::app_palette_ops::PaletteOperations;
use crate::tui::app_panel::{FocusedPanel, PanelOperations};
use crate::tui::app_reload::ReloadOperations;
use crate::tui::app_scroll::ScrollOperations;
use crate::tui::app_tasks::TaskMovementOperations;
use crate::tui::app_view::ViewOperations;

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
    /// Done archive revision that changes whenever done tasks are modified.
    done_rev: u64,
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
    /// Cached dependency graph and critical paths for overlay rendering.
    pub dependency_graph_cache: crate::tui::DependencyGraphCache,
    /// Focus manager for deterministic focus handling across panels and overlays.
    pub(crate) focus_manager: crate::tui::foundation::FocusManager,
    /// UI frame counter for animation timing.
    /// Incremented once per draw cycle for deterministic animations.
    ui_frame: u64,
    /// Frame number when the help overlay was first shown (for animation).
    /// None when help is not visible or animation has reset.
    help_overlay_start_frame: Option<u64>,
    /// Parallel state overlay state (lazy-initialized on first use).
    parallel_state_overlay: Option<crate::tui::app_parallel_state::ParallelStateOverlayState>,
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
            done_rev: 0,
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
            dependency_graph_cache: crate::tui::DependencyGraphCache::new(),
            focus_manager: crate::tui::foundation::FocusManager::default(),
            ui_frame: 0,
            help_overlay_start_frame: None,
            parallel_state_overlay: None,
        };
        app.rebuild_filtered_view();
        app
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    // Multi-select methods

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
    pub fn phase_elapsed(&self, phase: ExecutionPhase) -> std::time::Duration {
        use crate::tui::get_phase_elapsed;
        get_phase_elapsed(phase, &self.phase_completion_times, &self.phase_start_times)
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
        use crate::tui::count_completed_phases;
        count_completed_phases(&self.phase_completion_times, self.execution_phase) > 0
            && (self.phase_completion_times.contains_key(&phase)
                || phase.phase_number() < self.execution_phase.phase_number())
    }

    /// Check if a phase is currently active.
    pub fn is_phase_active(&self, phase: ExecutionPhase) -> bool {
        self.execution_phase == phase
    }

    /// Calculate overall completion percentage (0-100).
    pub fn completion_percentage(&self) -> u8 {
        use crate::tui::{calculate_completion_percentage, count_completed_phases};
        let completed = count_completed_phases(&self.phase_completion_times, self.execution_phase);
        calculate_completion_percentage(completed, self.configured_phases)
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

    /// Set the resized flag to trigger layout recalculation.
    #[allow(dead_code)]
    pub(crate) fn set_resized(&mut self, _width: u16, _height: u16) {
        self.resized = true;
    }

    // UI Frame counter methods for animation timing

    /// Get the current UI frame number.
    pub(crate) fn ui_frame(&self) -> u64 {
        self.ui_frame
    }

    /// Increment the UI frame counter.
    /// Should be called once per draw cycle.
    pub(crate) fn bump_ui_frame(&mut self) {
        self.ui_frame = self.ui_frame.wrapping_add(1);
    }

    /// Reset the UI frame counter to zero.
    #[allow(dead_code)]
    pub(crate) fn reset_ui_frame(&mut self) {
        self.ui_frame = 0;
    }

    // Help overlay animation methods

    /// Get or initialize the help overlay start frame.
    /// Returns the stored start frame if set, otherwise stores and returns `now_frame`.
    pub(crate) fn help_overlay_start_frame(&mut self, now_frame: u64) -> u64 {
        *self.help_overlay_start_frame.get_or_insert(now_frame)
    }

    /// Get the help overlay start frame without initializing it.
    #[allow(dead_code)]
    pub(crate) fn get_help_overlay_start_frame(&self) -> Option<u64> {
        self.help_overlay_start_frame
    }

    /// Clear the help overlay start frame (called when leaving help mode).
    pub(crate) fn clear_help_overlay_start_frame(&mut self) {
        self.help_overlay_start_frame = None;
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

    #[allow(dead_code)]
    pub(crate) fn bump_done_rev(&mut self) {
        self.done_rev = self.done_rev.wrapping_add(1);
    }

    pub(crate) fn bump_queue_and_done_rev(&mut self) {
        self.queue_rev = self.queue_rev.wrapping_add(1);
        self.done_rev = self.done_rev.wrapping_add(1);
    }

    pub(crate) fn done_rev(&self) -> u64 {
        self.done_rev
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
            self.bump_queue_and_done_rev();
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
            parent_id: None,
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

        self.bump_queue_and_done_rev();
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
        self.bump_queue_and_done_rev();
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
        if (status == "done" || status == "rejected")
            && let Err(e) = self.maybe_auto_archive(&task_id, now_rfc3339)
        {
            self.set_status_message(format!("Error: {}", e));
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

    pub(crate) fn enter_help_mode(&mut self, previous_mode: AppMode) {
        self.help_previous_mode = Some(previous_mode);
        self.help_scroll = 0;
        self.mode = AppMode::Help;
    }

    pub(crate) fn exit_help_mode(&mut self) {
        let previous_mode = self.help_previous_mode.take().unwrap_or(AppMode::Normal);
        self.mode = previous_mode;
        // Clear the help overlay animation state when exiting
        self.clear_help_overlay_start_frame();
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
        use crate::tui::app_palette::{build_palette_entries, filter_and_score_entries};
        let entries = build_palette_entries(self.loop_active);
        filter_and_score_entries(entries, query)
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

    /// Clamp selection and scroll to valid range after filter or resize changes.
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
}

// ============================================================================
// Trait implementations for extracted modules
// ============================================================================

impl PanelOperations for App {
    fn focus_next_panel(&mut self) {
        use crate::tui::app_panel::{DETAILS_PANEL_FOCUS, LIST_PANEL_FOCUS};

        // Determine current focus from focus manager, falling back to legacy field
        let current_focused = self.focus_manager.focused().or({
            // Fallback to legacy focused_panel for backward compatibility during migration
            match self.focused_panel {
                FocusedPanel::List => Some(LIST_PANEL_FOCUS),
                FocusedPanel::Details => Some(DETAILS_PANEL_FOCUS),
            }
        });

        let next = match current_focused {
            Some(id) if id == LIST_PANEL_FOCUS => DETAILS_PANEL_FOCUS,
            _ => LIST_PANEL_FOCUS,
        };
        self.focus_manager.focus(next);
        // Keep focused_panel in sync for backward compatibility during migration
        self.focused_panel = if next == DETAILS_PANEL_FOCUS {
            FocusedPanel::Details
        } else {
            FocusedPanel::List
        };
    }

    fn focus_previous_panel(&mut self) {
        // Same as next for 2 panels
        self.focus_next_panel();
    }

    fn focus_list_panel(&mut self) {
        use crate::tui::app_panel::LIST_PANEL_FOCUS;
        self.focus_manager.focus(LIST_PANEL_FOCUS);
        self.focused_panel = FocusedPanel::List;
    }

    fn details_focused(&self) -> bool {
        use crate::tui::app_panel::DETAILS_PANEL_FOCUS;
        self.focus_manager.is_focused(DETAILS_PANEL_FOCUS)
            || self.focused_panel == FocusedPanel::Details
    }

    fn set_list_area(&mut self, area: Rect) {
        self.list_area = Some(area);
    }

    fn clear_list_area(&mut self) {
        self.list_area = None;
    }

    fn list_area(&self) -> Option<Rect> {
        self.list_area
    }
}

impl ScrollOperations for App {
    fn scroll_details_up(&mut self, lines: usize) {
        self.details.scroll_up(lines);
    }

    fn scroll_details_down(&mut self, lines: usize) {
        self.details.scroll_down(lines);
    }

    fn scroll_details_top(&mut self) {
        self.details.scroll_top();
    }

    fn scroll_details_bottom(&mut self) {
        self.details.scroll_bottom();
    }

    fn details_scroll(&self) -> usize {
        self.details.scroll()
    }

    fn details_scroll_state(&mut self) -> &mut tui_scrollview::ScrollViewState {
        self.details.scroll_state()
    }

    fn set_details_viewport(
        &mut self,
        visible_lines: usize,
        total_lines: usize,
        context: DetailsContext,
    ) {
        self.details
            .set_viewport(visible_lines, total_lines, context.clone());
        self.details_context = Some(context);
    }

    fn scroll_help_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.help_scroll = self.help_scroll.saturating_sub(lines);
    }

    fn scroll_help_down(&mut self, lines: usize, total_lines: usize) {
        if lines == 0 {
            return;
        }
        let max_scroll = self.max_help_scroll(total_lines);
        self.help_scroll = (self.help_scroll + lines).min(max_scroll);
    }

    fn scroll_help_top(&mut self) {
        self.help_scroll = 0;
    }

    fn scroll_help_bottom(&mut self, total_lines: usize) {
        self.help_scroll = self.max_help_scroll(total_lines);
    }

    fn help_visible_lines(&self) -> usize {
        self.help_visible_lines.max(1)
    }

    fn help_total_lines(&self) -> usize {
        self.help_total_lines
    }

    fn help_scroll(&self) -> usize {
        self.help_scroll
    }

    fn set_help_visible_lines(&mut self, visible_lines: usize, total_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.help_visible_lines = visible_lines;
        self.help_total_lines = total_lines;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        if self.help_scroll > max_scroll {
            self.help_scroll = max_scroll;
        }
    }

    fn max_help_scroll(&self, total_lines: usize) -> usize {
        total_lines.saturating_sub(self.help_visible_lines())
    }

    fn log_visible_lines(&self) -> usize {
        self.log_visible_lines.max(1)
    }

    fn set_log_visible_lines(&mut self, lines: usize) {
        let visible_lines = lines.max(1);
        self.log_visible_lines = visible_lines;
        let max_scroll = self.max_log_scroll(visible_lines);
        if self.autoscroll || self.log_scroll > max_scroll {
            self.log_scroll = max_scroll;
        }
    }
}

impl ViewOperations for App {
    fn switch_to_list_view(&mut self) {
        if self.view_mode == ViewMode::List {
            return;
        }
        self.view_mode = ViewMode::List;
        self.sync_board_selection_to_list();
        self.set_status_message("Switched to list view (l)");
    }

    fn switch_to_board_view(&mut self) {
        if self.view_mode == ViewMode::Board {
            return;
        }
        self.view_mode = ViewMode::Board;
        self.board_nav
            .update_columns(&self.filtered_indices, &self.queue);
        self.sync_list_selection_to_board();
        self.set_status_message("Switched to board view (b)");
    }

    fn sync_board_selection_to_list(&mut self) {
        if let Some(queue_index) = self.board_nav.selected_task_index()
            && let Some(filtered_pos) = self
                .filtered_indices
                .iter()
                .position(|&idx| idx == queue_index)
        {
            self.selected = filtered_pos;
            self.clamp_selection_and_scroll();
        }
    }

    fn sync_list_selection_to_board(&mut self) {
        if let Some(queue_index) = self.filtered_indices.get(self.selected).copied() {
            self.board_nav.select_task(queue_index, &self.queue);
        }
    }

    fn update_board_columns(&mut self) {
        if self.view_mode == ViewMode::Board {
            self.board_nav
                .update_columns(&self.filtered_indices, &self.queue);
        }
    }
}

impl ReloadOperations for App {
    fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path) {
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

    fn check_external_changes_and_reload(&mut self, queue_path: &Path, done_path: &Path) -> bool {
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

    fn update_cached_mtimes(&mut self, queue_path: &Path, done_path: &Path) {
        self.queue_mtime = std::fs::metadata(queue_path)
            .ok()
            .and_then(|m| m.modified().ok());
        self.done_mtime = std::fs::metadata(done_path)
            .ok()
            .and_then(|m| m.modified().ok());
    }

    fn on_scan_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Scan completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_task_builder_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Task builder completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_scan_error(&mut self, msg: &str) {
        self.set_status_message(format!("Scan error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    fn on_task_builder_error(&mut self, msg: &str) {
        self.set_status_message(format!("Task builder error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }
}

impl PaletteOperations for App {
    fn execute_palette_command(
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
            PaletteCommand::OpenScopeInEditor => {
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };

                if task.scope.is_empty() {
                    self.set_status_message("Selected task has no scope paths");
                    return Ok(TuiAction::Continue);
                }

                Ok(TuiAction::OpenScopeInEditor(task.scope.clone()))
            }
            PaletteCommand::CopyFileLineRef => {
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };

                let refs = crate::tui::file_line_refs::extract_file_line_refs(
                    task.notes
                        .iter()
                        .chain(task.evidence.iter())
                        .map(|s| s.as_str()),
                );

                if refs.is_empty() {
                    self.set_status_message("No file:line references found in notes/evidence");
                    return Ok(TuiAction::Continue);
                }

                let text = crate::tui::file_line_refs::format_refs_for_clipboard(&refs);
                Ok(TuiAction::CopyToClipboard(text))
            }
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
                            strict_templates: false,
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
                            strict_templates: false,
                        };
                        spawn_task_builder(opts, options.repoprompt_mode, tx_clone);
                    }
                    Ok(false)
                }
                TuiAction::OpenScopeInEditor(scope) => {
                    let Some(queue_path) = app_ref.queue_path.as_ref() else {
                        app_ref.set_status_message("Cannot open editor: queue path not set");
                        return Ok(false);
                    };
                    let repo_root =
                        crate::tui::external_tools::repo_root_from_queue_path(queue_path);
                    let paths = crate::tui::external_tools::resolve_scope_paths(
                        repo_root.as_deref(),
                        &scope,
                    );

                    match crate::tui::external_tools::open_paths_in_editor(&paths) {
                        Ok(()) => app_ref.set_status_message(format!(
                            "Opened {} scope path(s) in editor",
                            paths.len()
                        )),
                        Err(e) => {
                            app_ref.set_status_message(format!("Open in editor failed: {}", e))
                        }
                    }

                    Ok(false)
                }
                TuiAction::CopyToClipboard(text) => {
                    match crate::tui::external_tools::copy_text_to_clipboard(&text) {
                        Ok(()) => {
                            app_ref.set_status_message("Copied file:line reference(s) to clipboard")
                        }
                        Err(e) => app_ref.set_status_message(format!("Copy failed: {}", e)),
                    }
                    Ok(false)
                }
                TuiAction::OpenUrlInBrowser(url) => {
                    match crate::tui::external_tools::open_url_in_browser(&url) {
                        Ok(()) => app_ref.set_status_message(format!("Opening URL: {}", url)),
                        Err(e) => app_ref.set_status_message(format!("Open URL failed: {}", e)),
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

                                    if let Some(max) = app_ref.loop_max_tasks
                                        && app_ref.loop_ran >= max
                                    {
                                        let loop_ran = app_ref.loop_ran;
                                        app_ref.loop_active = false;
                                        app_ref.set_status_message(format!(
                                            "Loop finished (ran {}/{})",
                                            loop_ran, max
                                        ));
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

// Parallel state overlay implementation
impl App {
    /// Get or initialize the parallel state overlay state.
    fn parallel_state_overlay(
        &mut self,
    ) -> &mut crate::tui::app_parallel_state::ParallelStateOverlayState {
        if self.parallel_state_overlay.is_none() {
            self.parallel_state_overlay =
                Some(crate::tui::app_parallel_state::ParallelStateOverlayState::new());
        }
        self.parallel_state_overlay.as_mut().unwrap()
    }

    /// Get a reference to the parallel state overlay state (may be None if not initialized).
    fn parallel_state_overlay_ref(
        &self,
    ) -> Option<&crate::tui::app_parallel_state::ParallelStateOverlayState> {
        self.parallel_state_overlay.as_ref()
    }

    /// Enter parallel state overlay mode.
    pub fn enter_parallel_state_overlay(&mut self) {
        let previous_mode = Box::new(self.mode.clone());
        self.mode = crate::tui::events::AppMode::ParallelStateOverlay { previous_mode };

        // Initialize overlay state and load from disk
        self.parallel_state_overlay_reload_from_disk();
    }

    /// Reload state from disk.
    pub fn parallel_state_overlay_reload_from_disk(&mut self) {
        // Get queue_path first to avoid borrow issues
        let Some(queue_path) = self.queue_path.clone() else {
            if let Some(overlay) = self.parallel_state_overlay.as_mut() {
                overlay.clear_snapshot();
            }
            return;
        };

        let Some(repo_root) = crate::tui::external_tools::repo_root_from_queue_path(&queue_path)
        else {
            if let Some(overlay) = self.parallel_state_overlay.as_mut() {
                overlay.clear_snapshot();
            }
            return;
        };

        let overlay = self.parallel_state_overlay();

        let state_path = crate::commands::run::state_file_path(&repo_root);

        match crate::commands::run::load_state(&state_path) {
            Ok(Some(state)) => {
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state },
                );
            }
            Ok(None) => {
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                        path: state_path.display().to_string(),
                    },
                );
            }
            Err(e) => {
                let error_str = e.to_string();
                overlay.set_snapshot(
                    crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Invalid {
                        path: state_path.display().to_string(),
                        error: error_str,
                    },
                );
            }
        }
    }

    /// Get the current snapshot.
    pub fn parallel_state_overlay_snapshot(
        &self,
    ) -> crate::tui::app_parallel_state::ParallelStateOverlaySnapshot {
        match self.parallel_state_overlay_ref() {
            Some(overlay) => overlay.snapshot().cloned().unwrap_or_else(|| {
                crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                    path: "unknown".to_string(),
                }
            }),
            None => crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Missing {
                path: "unknown".to_string(),
            },
        }
    }

    /// Get the active tab.
    pub fn parallel_state_overlay_active_tab(
        &self,
    ) -> crate::tui::app_parallel_state::ParallelStateTab {
        self.parallel_state_overlay_ref()
            .map(|o| o.active_tab())
            .unwrap_or_default()
    }

    /// Get tab counts and active tab.
    pub fn parallel_state_overlay_tab_counts_and_active(
        &self,
    ) -> (
        crate::tui::app_parallel_state::TabCounts,
        crate::tui::app_parallel_state::ParallelStateTab,
    ) {
        let active_tab = self.parallel_state_overlay_active_tab();

        let mut counts = crate::tui::app_parallel_state::TabCounts::default();

        if let Some(overlay) = self.parallel_state_overlay_ref()
            && let Some(snapshot) = overlay.snapshot()
            && let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state } =
                snapshot
        {
            counts.in_flight = state.tasks_in_flight.len();
            counts.prs = state.prs.len();
            counts.finished_without_pr = state.finished_without_pr.len();
        }

        (counts, active_tab)
    }

    /// Move to the next tab.
    pub fn parallel_state_overlay_next_tab(&mut self) {
        self.parallel_state_overlay().next_tab();
    }

    /// Move to the previous tab.
    pub fn parallel_state_overlay_prev_tab(&mut self) {
        self.parallel_state_overlay().prev_tab();
    }

    /// Set the visible rows count.
    pub fn parallel_state_overlay_set_visible_rows(&mut self, rows: usize) {
        self.parallel_state_overlay().set_visible_rows(rows);
    }

    /// Scroll up in the content.
    pub fn parallel_state_overlay_up(&mut self) {
        let active_tab = self.parallel_state_overlay_active_tab();
        let overlay = self.parallel_state_overlay();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::Prs => {
                overlay.select_pr_up();
            }
            _ => {
                overlay.scroll_up(1);
            }
        }
    }

    /// Scroll down in the content.
    pub fn parallel_state_overlay_down(&mut self) {
        let active_tab = self.parallel_state_overlay_active_tab();
        let total_items = self.parallel_state_overlay_total_items();
        let overlay = self.parallel_state_overlay();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::Prs => {
                if let Some(snapshot) = overlay.snapshot()
                    && let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded {
                        state,
                    } = snapshot
                {
                    overlay.select_pr_down(state.prs.len());
                }
            }
            _ => {
                overlay.scroll_down(1, total_items);
            }
        }
    }

    /// Page up in the content.
    pub fn parallel_state_overlay_page_up(&mut self) {
        self.parallel_state_overlay().page_up();
    }

    /// Page down in the content.
    pub fn parallel_state_overlay_page_down(&mut self) {
        let total_items = self.parallel_state_overlay_total_items();
        self.parallel_state_overlay().page_down(total_items);
    }

    /// Scroll to the top.
    pub fn parallel_state_overlay_top(&mut self) {
        self.parallel_state_overlay().scroll_top();
    }

    /// Scroll to the bottom.
    pub fn parallel_state_overlay_bottom(&mut self) {
        let total_items = self.parallel_state_overlay_total_items();
        self.parallel_state_overlay().scroll_bottom(total_items);
    }

    /// Get the selected PR index.
    pub fn parallel_state_overlay_selected_pr_index(&self) -> usize {
        self.parallel_state_overlay_ref()
            .map(|o| o.selected_pr())
            .unwrap_or(0)
    }

    /// Get the PR scroll offset.
    pub fn parallel_state_overlay_pr_scroll(&self) -> usize {
        self.parallel_state_overlay_ref()
            .map(|o| o.scroll())
            .unwrap_or(0)
    }

    /// Get the selected PR URL, if any.
    pub fn parallel_state_overlay_selected_pr_url(&self) -> Option<String> {
        let overlay = self.parallel_state_overlay_ref()?;
        let snapshot = overlay.snapshot()?;

        if let crate::tui::app_parallel_state::ParallelStateOverlaySnapshot::Loaded { state } =
            snapshot
        {
            let selected_idx = overlay.selected_pr();
            state.prs.get(selected_idx).map(|pr| pr.pr_url.clone())
        } else {
            None
        }
    }

    /// Get the metadata line for display.
    pub fn parallel_state_overlay_metadata_line(&self, _max_width: usize) -> String {
        let (counts, _) = self.parallel_state_overlay_tab_counts_and_active();
        format!(
            "In-Flight: {} | PRs: {} | Finished w/o PR: {}",
            counts.in_flight, counts.prs, counts.finished_without_pr
        )
    }

    /// Get the footer hint for display.
    pub fn parallel_state_overlay_footer_hint(&self) -> String {
        "Esc/P: close | Tab: section | r: reload | ↑↓/j/k: nav | o/Enter: open | y: copy"
            .to_string()
    }

    /// Helper to get total items for the active tab.
    fn parallel_state_overlay_total_items(&self) -> usize {
        let (counts, active_tab) = self.parallel_state_overlay_tab_counts_and_active();

        match active_tab {
            crate::tui::app_parallel_state::ParallelStateTab::InFlight => counts.in_flight,
            crate::tui::app_parallel_state::ParallelStateTab::Prs => counts.prs,
            crate::tui::app_parallel_state::ParallelStateTab::FinishedWithoutPr => {
                counts.finished_without_pr
            }
        }
    }
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
            parent_id: None,
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
