//! TUI application state definition and core methods.
//!
//! Responsibilities:
//! - Define the App struct with all TUI state fields.
//! - Provide App::new constructor and basic state accessors.
//! - Export core types and re-exports from submodules.
//!
//! Not handled here:
//! - Runtime event loop (see `app_runtime` module).
//! - Terminal session setup (see `app_session` module).
//! - Resize and frame handling (see `app_resize` module).
//! - Filter state management (see `app_filters` module).
//! - Execution phase tracking (see `app_execution` module).
//! - Log management (see `app_logs` module).
//! - Task mutation operations (see `app_tasks` module).
//! - Navigation operations (see `app_navigation` module).
//!
//! Invariants/assumptions:
//! - The App struct is the single source of truth for TUI state.
//! - Most methods are defined in submodules via `impl App` blocks.

use crate::config::ConfigLayer;
use crate::constants::buffers::MAX_ANSI_BUFFER_SIZE;
use crate::constants::timeouts::SPINNER_UPDATE_INTERVAL_MS;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::progress::{ExecutionPhase, SpinnerState};
use crate::queue::TaskEditKey;
use crate::{lock, queue};
use anyhow::{Context, Result, anyhow, bail};
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::TextInput;
use super::events::{AppMode, PaletteEntry, TaskBuilderState, TaskBuilderStep, ViewMode};
use super::terminal::{BorderStyle, ColorSupport, TerminalCapabilities};
use super::{DetailsContext, DetailsState};
use crate::tui::app_execution::RunningKind;
use crate::tui::app_filters::FilterManagementOperations;

use crate::tui::app_filters::{FilterKey, FilterSnapshot, FilterState};
use crate::tui::app_logs::LogOperations;

use crate::tui::app_navigation::BoardNavigationState;
#[cfg(test)]
use crate::tui::app_options::FilterCacheStats;
use crate::tui::app_palette::scan_label;

use crate::tui::app_panel::FocusedPanel;
use crate::tui::app_resize::ResizeOperations;
use crate::tui::app_scroll::ScrollOperations;

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
    pub(crate) focused_panel: FocusedPanel,
    /// Details panel scroll state using tui-scrollview.
    pub details: DetailsState,
    /// Context key for details content (used to reset scroll on change).
    pub(crate) details_context: Option<DetailsContext>,
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
    pub(crate) help_scroll: usize,
    /// Last known visible help lines in Help overlay (for paging).
    pub(crate) help_visible_lines: usize,
    /// Last known total help line count (post-wrap).
    pub(crate) help_total_lines: usize,
    /// Previous mode before entering the Help overlay.
    help_previous_mode: Option<AppMode>,
    /// Height of the task list (for scrolling calculation).
    pub list_height: usize,
    /// Last known list panel area (inner rect, without borders) for hit-testing.
    pub(crate) list_area: Option<Rect>,
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
    pub(crate) queue_mtime: Option<std::time::SystemTime>,
    /// Cached modification time for done.json (for detecting external changes).
    pub(crate) done_mtime: Option<std::time::SystemTime>,
    /// Flag set when terminal was resized, cleared after redraw.
    /// Used to trigger layout recalculation and prevent visual glitches.
    pub(crate) resized: bool,
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
    pub(crate) ui_frame: u64,
    /// Frame number when the help overlay was first shown (for animation).
    /// None when help is not visible or animation has reset.
    pub(crate) help_overlay_start_frame: Option<u64>,
    /// Parallel state overlay state (lazy-initialized on first use).
    pub(crate) parallel_state_overlay:
        Option<crate::tui::app_parallel_state::ParallelStateOverlayState>,
    /// Aging thresholds for task aging indicators.
    pub aging_thresholds: crate::reports::AgingThresholds,
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
            aging_thresholds: crate::reports::AgingThresholds::default(),
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
    ///
    /// Only records if the task is actually Done in the done archive.
    pub(crate) fn record_execution_history_for_task(
        &self,
        task_id: &str,
        done_path: &std::path::Path,
    ) {
        let Some(cache_dir) = self.cache_dir() else {
            return;
        };

        // Only record if the task is actually Done in done.json.
        let task_in_done = match crate::queue::load_queue_or_default(done_path) {
            Ok(done) => done
                .tasks
                .iter()
                .any(|t| t.id == task_id && t.status == crate::contracts::TaskStatus::Done),
            Err(err) => {
                log::warn!(
                    "Skipping execution history for {}: read done.json failed: {}",
                    task_id,
                    err
                );
                return;
            }
        };
        if !task_in_done {
            log::debug!(
                "Skipping execution history for {}: task not in done with Done status",
                task_id
            );
            return;
        }

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
        let result = crate::execution_history::record_execution(
            task_id,
            &runner,
            &model,
            self.configured_phases,
            phase_durations,
            total_duration,
            &cache_dir,
        );

        match result {
            Ok(()) => log::debug!("Recorded execution history for {} in TUI mode", task_id),
            Err(err) => log::warn!(
                "Failed to record execution history for {}: {}",
                task_id,
                err
            ),
        }
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
            description: None,
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
            started_at: None,
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
    pub(crate) fn set_task_status(&mut self, status: &str, now_rfc3339: &str) {
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
    pub(crate) fn set_task_priority(&mut self, priority: &str, now_rfc3339: &str) {
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
    pub(crate) fn clamp_selection_and_scroll(&mut self) {
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
