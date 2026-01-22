//! Interactive Terminal UI for browsing and managing the task queue.
//!
//! The TUI provides a split-pane interface:
//! - Left panel: list of tasks with ID, status, priority, and title
//! - Right panel: detailed view of the selected task
//!
//! Key bindings:
//! - `q` / `Esc`: Quit (prompts if a task is still running)
//! - `Up` / `Down` / `j` / `k`: Navigate task list
//! - `Enter`: Execute task (suspends TUI, runs task, restores)
//! - `d`: Delete task (with confirmation)
//! - `e`: Edit task title
//! - `s`: Cycle status (Draft → Todo → Doing → Done → Rejected → Draft)
//! - `f`: Cycle status filter (All → Todo → Doing → Done → Draft → Rejected → All)
//! - `p`: Cycle priority (Low → Medium → High → Critical → Low)
//! - `r`: Reload queue from disk
//! - `n`: Create a new task
//! - `/`: Search tasks by text
//! - `t`: Filter tasks by tags
//! - `x`: Clear active filters
//! - Executing view: `↑`/`↓`/`j`/`k` scroll, `PgUp`/`PgDn` page, `a` toggles auto-scroll

use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::timeutil;
use crate::{fsutil, queue};

pub mod events;
pub mod render;

pub use events::{handle_key_event, AppMode, TuiAction};
pub use render::draw_ui;

/// Active filters applied to the task list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterState {
    /// Free-text search query (substring match across task fields).
    pub query: String,
    /// Status filters (empty means all statuses).
    pub statuses: Vec<TaskStatus>,
    /// Tag filters (empty means all tags).
    pub tags: Vec<String>,
}

/// Application state for the TUI.
pub struct App {
    /// The task queue (cloned for mutable operations during TUI session)
    pub queue: QueueFile,
    /// Currently selected task index
    pub selected: usize,
    /// Current interaction mode
    pub mode: AppMode,
    /// Scroll offset for the task list
    pub scroll: usize,
    /// Width of the right panel for text wrapping
    pub detail_width: u16,
    /// Flag indicating if queue was modified (needs save)
    pub dirty: bool,
    /// Last auto-save error message, if any.
    pub save_error: Option<String>,
    /// Execution logs (when in Executing mode)
    pub logs: Vec<String>,
    /// Scroll offset for execution logs
    pub log_scroll: usize,
    /// Whether to auto-scroll execution logs
    pub autoscroll: bool,
    /// Last known visible log lines in Executing view (for paging/auto-scroll).
    pub log_visible_lines: usize,
    /// Height of the task list (for scrolling calculation)
    pub list_height: usize,
    /// Whether a runner thread is currently executing a task.
    pub runner_active: bool,
    /// Task ID prefix used for new task creation.
    pub id_prefix: String,
    /// Task ID width used for new task creation.
    pub id_width: usize,
    /// Optional path to the done queue for ID generation.
    pub done_path: Option<PathBuf>,
    /// Active filters applied to the task list.
    pub filters: FilterState,
    /// Cached filtered task indices into `queue.tasks`.
    pub filtered_indices: Vec<usize>,
}

impl App {
    /// Create a new TUI app from a queue file.
    pub fn new(queue: QueueFile) -> Self {
        let mut app = Self {
            queue,
            selected: 0,
            mode: AppMode::Normal,
            scroll: 0,
            detail_width: 60,
            dirty: false,
            save_error: None,
            logs: Vec::new(),
            log_scroll: 0,
            autoscroll: true,
            log_visible_lines: 20,
            list_height: 20,
            runner_active: false,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            done_path: None,
            filters: FilterState::default(),
            filtered_indices: Vec::new(),
        };
        app.rebuild_filtered_view();
        app
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

    /// Get the currently selected task mutably, if any.
    pub fn selected_task_mut(&mut self) -> Option<&mut Task> {
        let idx = self.selected_task_index()?;
        self.queue.tasks.get_mut(idx)
    }

    /// Return the number of tasks in the filtered view.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
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
            // Scroll down if selection is below visible area
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected - list_height + 1;
            }
        }
    }

    /// Cycle the status of the selected task.
    pub fn cycle_status(&mut self, now_rfc3339: &str) -> Result<()> {
        let task_id = self
            .selected_task()
            .map(|t| t.id.clone())
            .ok_or_else(|| anyhow!("No task selected"))?;

        let task = self
            .selected_task_mut()
            .ok_or_else(|| anyhow!("No task selected"))?;

        let new_status = match task.status {
            TaskStatus::Draft => TaskStatus::Todo,
            TaskStatus::Todo => TaskStatus::Doing,
            TaskStatus::Doing => TaskStatus::Done,
            TaskStatus::Done => TaskStatus::Rejected,
            TaskStatus::Rejected => TaskStatus::Draft,
        };

        task.status = new_status;
        task.updated_at = Some(now_rfc3339.to_string());

        match new_status {
            TaskStatus::Done | TaskStatus::Rejected => {
                task.completed_at = Some(now_rfc3339.to_string());
            }
            TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
                task.completed_at = None;
            }
        }

        self.dirty = true;
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        Ok(())
    }

    /// Cycle the priority of the selected task.
    pub fn cycle_priority(&mut self, now_rfc3339: &str) -> Result<()> {
        let task_id = self
            .selected_task()
            .map(|t| t.id.clone())
            .ok_or_else(|| anyhow!("No task selected"))?;

        let task = self
            .selected_task_mut()
            .ok_or_else(|| anyhow!("No task selected"))?;

        task.priority = task.priority.cycle();
        task.updated_at = Some(now_rfc3339.to_string());
        self.dirty = true;
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        Ok(())
    }

    /// Delete the selected task (returns the deleted task for confirmation).
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

        // Try to preserve selection of the next task, or previous if at end
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
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        Ok(task)
    }

    /// Update the title of the selected task.
    pub fn update_title(&mut self, new_title: String, now_rfc3339: &str) -> Result<()> {
        let task_id = self
            .selected_task()
            .map(|t| t.id.clone())
            .ok_or_else(|| anyhow!("No task selected"))?;

        let task = self
            .selected_task_mut()
            .ok_or_else(|| anyhow!("No task selected"))?;

        if new_title.trim().is_empty() {
            bail!("Title cannot be empty");
        }

        task.title = new_title;
        task.updated_at = Some(now_rfc3339.to_string());
        self.dirty = true;
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        Ok(())
    }

    /// Create a new task with default fields and the provided title.
    pub fn create_task_from_title(&mut self, title: &str, now_rfc3339: &str) -> Result<()> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            bail!("Title cannot be empty");
        }

        let done_queue = match self.done_path.as_ref() {
            Some(path) if path.exists() => Some(queue::load_queue_or_default(path)?),
            _ => None,
        };
        let next_id = queue::next_id_across(
            &self.queue,
            done_queue.as_ref(),
            &self.id_prefix,
            self.id_width,
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        };

        self.queue.tasks.push(task);
        let new_index = self.queue.tasks.len().saturating_sub(1);
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
        Ok(())
    }

    /// Cycle the active status filter (All -> Todo -> Doing -> Done -> Draft -> Rejected -> All).
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

    /// Set the search query (empty to clear).
    pub fn set_search_query(&mut self, query: String) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.query = query;
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    /// Clear all active filters (query, tags, status).
    pub fn clear_filters(&mut self) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters = FilterState::default();
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
    }

    /// Return true if any filters are active.
    pub fn has_active_filters(&self) -> bool {
        !self.filters.query.trim().is_empty()
            || !self.filters.tags.is_empty()
            || !self.filters.statuses.is_empty()
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
        Some(format!("filters: {}", parts.join(" ")))
    }

    /// Parse comma or whitespace-separated tags from input string.
    pub fn parse_tags(input: &str) -> Vec<String> {
        input
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
    }

    fn log_visible_lines(&self) -> usize {
        self.log_visible_lines.max(1)
    }

    fn set_log_visible_lines(&mut self, visible_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.log_visible_lines = visible_lines;
        let max_scroll = self.max_log_scroll(visible_lines);
        if self.autoscroll || self.log_scroll > max_scroll {
            self.log_scroll = max_scroll;
        }
    }

    fn max_log_scroll(&self, visible_lines: usize) -> usize {
        self.logs.len().saturating_sub(visible_lines)
    }

    fn scroll_logs_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        self.log_scroll = self.log_scroll.saturating_sub(lines);
    }

    fn scroll_logs_down(&mut self, lines: usize, visible_lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        let max_scroll = self.max_log_scroll(visible_lines);
        self.log_scroll = (self.log_scroll + lines).min(max_scroll);
    }

    fn enable_autoscroll(&mut self, visible_lines: usize) {
        self.autoscroll = true;
        self.log_scroll = self.max_log_scroll(visible_lines);
    }

    /// Rebuild the filtered view, optionally preserving selection of a preferred task ID.
    pub(crate) fn rebuild_filtered_view(&mut self) {
        self.rebuild_filtered_view_with_preferred(None);
    }

    /// Rebuild the filtered view, optionally preserving selection of a preferred task ID.
    fn rebuild_filtered_view_with_preferred(&mut self, preferred_id: Option<&str>) {
        let mut filtered = queue::filter_tasks(
            &self.queue,
            &self.filters.statuses,
            &self.filters.tags,
            &[],
            None,
        );

        if !self.filters.query.trim().is_empty() {
            match queue::search_tasks(filtered, &self.filters.query, false, false) {
                Ok(results) => {
                    filtered = results;
                }
                Err(err) => {
                    self.logs.push(format!("Error: {}", err));
                    filtered = Vec::new();
                }
            }
        }

        let mut index_by_id = std::collections::HashMap::new();
        for (idx, task) in self.queue.tasks.iter().enumerate() {
            index_by_id.insert(task.id.as_str(), idx);
        }

        self.filtered_indices = filtered
            .iter()
            .filter_map(|task| index_by_id.get(task.id.as_str()).copied())
            .collect();

        // Try to preserve selection of preferred_id if provided
        if let Some(preferred_id) = preferred_id {
            if let Some(new_pos) =
                self.filtered_indices
                    .iter()
                    .enumerate()
                    .find_map(|(pos, &idx)| {
                        self.queue.tasks.get(idx).and_then(|task| {
                            if task.id == preferred_id {
                                Some(pos)
                            } else {
                                None
                            }
                        })
                    })
            {
                self.selected = new_pos;
                self.clamp_selection_and_scroll();
                return;
            }
            // Preferred task was filtered out, reset selection to 0
            self.selected = 0;
        }

        self.clamp_selection_and_scroll();
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

    /// Reload the queue from disk, clamping selection and recording errors.
    fn reload_queue_from_disk(&mut self, queue_path: &Path) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        match queue::load_queue(queue_path) {
            Ok(new_queue) => {
                self.queue = new_queue;
                self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
                self.dirty = false;
                self.save_error = None;
            }
            Err(e) => {
                self.logs.push(format!("ERROR reloading queue: {}", e));
            }
        }
    }
}

fn auto_save_if_dirty(app: &mut App, queue_path: &std::path::Path) {
    if !app.dirty {
        return;
    }

    match queue::save_queue(queue_path, &app.queue) {
        Ok(()) => {
            app.dirty = false;
            app.save_error = None;
        }
        Err(e) => {
            let message = format!("ERROR saving queue: {}", e);
            let should_log = app.save_error.as_deref() != Some(message.as_str());
            app.save_error = Some(message.clone());
            if should_log {
                app.logs.push(message);
            }
        }
    }
}

/// Event sent from the runner thread to the TUI.
enum RunnerEvent {
    /// Output chunk received
    Output(String),
    /// Task finished (success or failure)
    Finished,
    /// Task failed with error
    Error(String),
}

/// Run the TUI application with an active queue lock.
///
/// This function:
/// 1. Sets up the terminal for TUI mode
/// 2. Runs the interactive event loop
/// 3. Cleans up terminal state on exit
/// 4. Returns None (tasks are executed within TUI in Executing mode)
///
/// The `runner_factory` creates a closure that executes a task when called.
/// It receives a task ID and an output handler callback.
pub fn run_tui<F, E>(
    resolved: &crate::config::Resolved,
    force_lock: bool,
    runner_factory: F,
) -> Result<Option<String>>
where
    F: Fn(String, crate::runner::OutputHandler) -> E + Send + Sync + 'static,
    E: FnOnce() -> Result<()> + Send + 'static,
{
    let (app, _queue_lock) = prepare_tui_session(resolved, force_lock)?;
    let queue_path = &resolved.queue_path;

    // Setup terminal
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    // Create channels for runner events
    let (tx, rx) = mpsc::channel();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // We use a RefCell for interior mutability within the closure
        use std::cell::RefCell;
        let app = RefCell::new(app);

        // Main event loop
        loop {
            // Draw the UI
            terminal
                .draw(|f| {
                    let mut app_ref = app.borrow_mut();
                    // Update detail width from current terminal size
                    app_ref.detail_width = f.area().width.saturating_sub(4);
                    draw_ui(f, &mut app_ref)
                })
                .context("draw UI")?;

            // Check for runner events
            while let Ok(event) = rx.try_recv() {
                let mut app_ref = app.borrow_mut();
                match event {
                    RunnerEvent::Output(text) => {
                        // Split text into lines and append to logs
                        for line in text.lines() {
                            app_ref.logs.push(line.to_string());
                        }
                        // Keep logs bounded (max 10k lines)
                        if app_ref.logs.len() > 10000 {
                            let excess = app_ref.logs.len() - 10000;
                            app_ref.logs.drain(0..excess);
                            app_ref.log_scroll = app_ref.log_scroll.saturating_sub(excess);
                        }
                        // Auto-scroll if enabled
                        if app_ref.autoscroll {
                            // Scroll to show latest logs
                            let visible_lines = app_ref.log_visible_lines();
                            app_ref.log_scroll = app_ref.max_log_scroll(visible_lines);
                        }
                    }
                    RunnerEvent::Finished => {
                        app_ref.runner_active = false;
                        // Reload queue to capture any changes made by the runner (or agents)
                        app_ref.reload_queue_from_disk(queue_path);

                        // Restore normal mode if we were in a runner-related view
                        if matches!(
                            &app_ref.mode,
                            AppMode::Executing { .. } | AppMode::ConfirmQuit
                        ) {
                            app_ref.mode = AppMode::Normal;
                        }
                    }
                    RunnerEvent::Error(msg) => {
                        app_ref.runner_active = false;
                        app_ref.logs.push(format!("ERROR: {}", msg));
                        if matches!(
                            &app_ref.mode,
                            AppMode::Executing { .. } | AppMode::ConfirmQuit
                        ) {
                            app_ref.mode = AppMode::Normal;
                        }
                    }
                }
            }

            // Auto-save if dirty
            if app.borrow().dirty {
                let mut app_ref = app.borrow_mut();
                auto_save_if_dirty(&mut app_ref, queue_path);
            }

            // Handle events with timeout (for polling runner events)
            if event::poll(Duration::from_millis(100)).context("poll event")? {
                if let Event::Key(key) = event::read().context("read event")? {
                    // Ignore key release events
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }

                    let mut app_ref = app.borrow_mut();

                    // Use of extracted handle_key_event function
                    let now = timeutil::now_utc_rfc3339()?;
                    match handle_key_event(&mut app_ref, key.code, &now)? {
                        TuiAction::Quit => break,
                        TuiAction::Continue => {}
                        TuiAction::ReloadQueue => {
                            app_ref.reload_queue_from_disk(queue_path);
                        }
                        TuiAction::RunTask(task_id) => {
                            // Spawn runner thread
                            let tx_clone = tx.clone();
                            let tx_clone_for_handler = tx.clone();
                            let handler: crate::runner::OutputHandler =
                                Arc::new(Box::new(move |text: &str| {
                                    let _ = tx_clone_for_handler
                                        .send(RunnerEvent::Output(text.to_string()));
                                }));

                            let runner_fn = runner_factory(task_id.clone(), handler);
                            thread::spawn(move || {
                                let result = runner_fn();
                                match result {
                                    Ok(()) => {
                                        let _ = tx_clone.send(RunnerEvent::Finished);
                                    }
                                    Err(e) => {
                                        let _ = tx_clone.send(RunnerEvent::Error(e.to_string()));
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>(None)
    }));

    // Cleanup terminal
    disable_raw_mode().context("disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("leave alternate screen")?;
    terminal.show_cursor().context("show cursor")?;

    match result {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(e)) => Err(e),
        Err(_) => bail!("TUI panicked"),
    }
}

/// Acquire the queue lock and load the queue for TUI usage.
fn prepare_tui_session(
    resolved: &crate::config::Resolved,
    force_lock: bool,
) -> Result<(App, fsutil::DirLock)> {
    let lock = queue::acquire_queue_lock(&resolved.repo_root, "tui", force_lock)?;
    let (queue, _done) = queue::load_and_validate_queues(resolved, true)?;
    let mut app = App::new(queue);
    app.id_prefix = resolved.id_prefix.clone();
    app.id_width = resolved.id_width;
    app.done_path = Some(resolved.done_path.clone());
    Ok((app, lock))
}

// Rendering (draw/layout/color helpers) lives in `crate::tui::render`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority};
    use tempfile::TempDir;

    fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            title: title.to_string(),
            status,
            priority: TaskPriority::Medium,
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["test evidence".to_string()],
            plan: vec!["test plan".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        }
    }

    fn make_test_task_with_tags(id: &str, title: &str, tags: Vec<&str>) -> Task {
        let mut task = make_test_task(id, title, TaskStatus::Todo);
        task.tags = tags.into_iter().map(|tag| tag.to_string()).collect();
        task
    }

    #[test]
    fn app_new_with_empty_queue() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![],
        };
        let app = App::new(queue);
        assert_eq!(app.selected, 0);
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.scroll, 0);
        assert!(!app.dirty);
        assert!(!app.runner_active);
    }

    #[test]
    fn app_new_with_tasks() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Doing),
            ],
        };
        let app = App::new(queue);
        assert_eq!(app.selected, 0);
        assert_eq!(app.queue.tasks.len(), 2);
        assert!(!app.runner_active);
    }

    #[test]
    fn app_move_up_does_not_go_negative() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.selected = 0;
        app.move_up();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn app_move_down_stays_within_bounds() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.selected = 0;
        app.move_down(10);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn app_cycle_status_cycles_correctly() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Draft)],
        };
        let mut app = App::new(queue);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Todo);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Done);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Draft);
    }

    #[test]
    fn app_cycle_priority_cycles_correctly_and_updates_timestamp() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.cycle_priority("2026-01-20T12:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].priority, TaskPriority::High);
        assert_eq!(
            app.queue.tasks[0].updated_at,
            Some("2026-01-20T12:00:00Z".to_string())
        );
        assert!(app.dirty);

        app.cycle_priority("2026-01-20T12:00:01Z").unwrap();
        assert_eq!(app.queue.tasks[0].priority, TaskPriority::Critical);

        app.cycle_priority("2026-01-20T12:00:02Z").unwrap();
        assert_eq!(app.queue.tasks[0].priority, TaskPriority::Low);

        app.cycle_priority("2026-01-20T12:00:03Z").unwrap();
        assert_eq!(app.queue.tasks[0].priority, TaskPriority::Medium);
    }

    #[test]
    fn app_cycle_priority_errors_when_no_task_selected() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![],
        };
        let mut app = App::new(queue);

        let err = app
            .cycle_priority("2026-01-20T12:00:00Z")
            .expect_err("expected no task selected error");
        assert!(err.to_string().contains("No task selected"));
    }

    #[test]
    fn app_delete_selected_task_removes_task() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Doing),
                make_test_task("RQ-0003", "Task 3", TaskStatus::Done),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 1;

        let deleted = app.delete_selected_task().unwrap();
        assert_eq!(deleted.id, "RQ-0002");
        assert_eq!(app.queue.tasks.len(), 2);
        assert_eq!(app.queue.tasks[0].id, "RQ-0001");
        assert_eq!(app.queue.tasks[1].id, "RQ-0003");
        assert!(app.dirty);
    }

    #[test]
    fn app_delete_selected_task_adjusts_selection() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Doing),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 1;

        app.delete_selected_task().unwrap();
        assert_eq!(app.selected, 0);
        assert_eq!(app.queue.tasks.len(), 1);
    }

    #[test]
    fn app_update_title_changes_title() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.update_title("New Title".to_string(), "2026-01-20T12:00:00Z")
            .unwrap();
        assert_eq!(app.queue.tasks[0].title, "New Title");
        assert_eq!(
            app.queue.tasks[0].updated_at,
            Some("2026-01-20T12:00:00Z".to_string())
        );
        assert!(app.dirty);
    }

    #[test]
    fn app_update_title_rejects_empty_title() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        assert!(app
            .update_title("".to_string(), "2026-01-20T12:00:00Z")
            .is_err());
        assert!(app
            .update_title("   ".to_string(), "2026-01-20T12:00:00Z")
            .is_err());
    }

    #[test]
    fn app_create_task_from_title_appends_with_defaults() -> Result<()> {
        let temp = TempDir::new()?;
        let done_path = temp.path().join("done.json");
        let mut done_task = make_test_task("RQ-0005", "Done Task", TaskStatus::Done);
        done_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());
        let done_queue = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };
        queue::save_queue(&done_path, &done_queue)?;

        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0003", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.id_prefix = "RQ".to_string();
        app.id_width = 4;
        app.done_path = Some(done_path);
        app.mode = AppMode::CreatingTask("New Task".to_string());

        app.create_task_from_title("New Task", "2026-01-20T12:00:00Z")?;

        assert_eq!(app.queue.tasks.len(), 2);
        let task = &app.queue.tasks[1];
        assert_eq!(task.id, "RQ-0006");
        assert_eq!(task.title, "New Task");
        assert_eq!(task.status, TaskStatus::Todo);
        assert_eq!(task.priority, TaskPriority::Medium);
        assert_eq!(task.created_at, Some("2026-01-20T12:00:00Z".to_string()));
        assert_eq!(task.updated_at, Some("2026-01-20T12:00:00Z".to_string()));
        assert!(task.completed_at.is_none());
        assert!(task.tags.is_empty());
        assert!(task.scope.is_empty());
        assert!(task.evidence.is_empty());
        assert!(task.plan.is_empty());
        assert!(task.notes.is_empty());
        assert!(task.depends_on.is_empty());
        assert!(task.custom_fields.is_empty());
        assert!(app.dirty);
        assert_eq!(app.mode, AppMode::Normal);
        Ok(())
    }

    #[test]
    fn app_create_task_from_title_rejects_blank_title() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::CreatingTask("   ".to_string());

        let err = app
            .create_task_from_title("   ", "2026-01-20T12:00:00Z")
            .expect_err("expected blank title error");

        assert!(err.to_string().contains("Title cannot be empty"));
        assert_eq!(app.queue.tasks.len(), 1);
        assert_eq!(app.mode, AppMode::CreatingTask("   ".to_string()));
    }

    #[test]
    fn reload_queue_clamps_selection_and_clears_dirty() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let initial_queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Doing),
            ],
        };
        queue::save_queue(&queue_path, &initial_queue)?;
        let mut app = App::new(initial_queue);
        app.selected = 1;
        app.scroll = 1;
        app.dirty = true;

        let updated_queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0003", "Task 3", TaskStatus::Todo)],
        };
        queue::save_queue(&queue_path, &updated_queue)?;

        app.reload_queue_from_disk(&queue_path);

        assert_eq!(app.queue.tasks.len(), 1);
        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll, 0);
        assert!(!app.dirty);
        assert!(app.save_error.is_none());
        Ok(())
    }

    #[test]
    fn reload_queue_logs_errors_on_failure() -> Result<()> {
        let temp = TempDir::new()?;
        let bad_path = temp.path().join("queue_dir");
        std::fs::create_dir_all(&bad_path)?;
        let mut app = App::new(QueueFile::default());

        app.reload_queue_from_disk(&bad_path);

        assert_eq!(app.logs.len(), 1);
        assert!(app.logs[0].contains("ERROR reloading queue"));
        Ok(())
    }

    #[test]
    fn prepare_tui_session_acquires_queue_lock() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        queue::save_queue(&queue_path, &QueueFile::default())?;
        let done_path = ralph_dir.join("done.json");

        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path: queue_path.clone(),
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let (_app, _lock) = prepare_tui_session(&resolved, false)?;
        let lock_dir = fsutil::queue_lock_dir(repo_root);
        assert!(lock_dir.exists());

        let err = queue::acquire_queue_lock(repo_root, "tui second", false)
            .expect_err("expected lock to be held");
        assert!(err.to_string().contains("Queue lock already held"));
        Ok(())
    }

    #[test]
    fn prepare_tui_session_rejects_invalid_queue() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        let mut queue = QueueFile::default();
        queue
            .tasks
            .push(make_test_task("BAD-1", "Bad task", TaskStatus::Todo));
        queue::save_queue(&queue_path, &queue)?;
        let done_path = ralph_dir.join("done.json");

        let resolved = crate::config::Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = prepare_tui_session(&resolved, false)
            .err()
            .expect("expected validation error");
        assert!(err.to_string().contains("Mismatched task ID prefix"));
        Ok(())
    }

    #[test]
    fn auto_save_clears_dirty_on_success() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let queue = QueueFile::default();
        let mut app = App::new(queue);
        app.dirty = true;

        auto_save_if_dirty(&mut app, &queue_path);

        assert!(!app.dirty);
        assert!(app.save_error.is_none());
        assert!(queue_path.exists());
        Ok(())
    }

    #[test]
    fn auto_save_keeps_dirty_on_failure_and_dedupes_logs() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue_dir");
        std::fs::create_dir_all(&queue_path)?;
        let queue = QueueFile::default();
        let mut app = App::new(queue);
        app.dirty = true;

        auto_save_if_dirty(&mut app, &queue_path);
        assert!(app.dirty);
        assert!(app.save_error.is_some());
        assert_eq!(app.logs.len(), 1);

        auto_save_if_dirty(&mut app, &queue_path);
        assert!(app.dirty);
        assert_eq!(app.logs.len(), 1);
        Ok(())
    }

    #[test]
    fn auto_save_clears_error_after_recovery() -> Result<()> {
        let temp = TempDir::new()?;
        let bad_path = temp.path().join("queue_dir");
        std::fs::create_dir_all(&bad_path)?;
        let good_path = temp.path().join("queue.json");
        let queue = QueueFile::default();
        let mut app = App::new(queue);
        app.dirty = true;

        auto_save_if_dirty(&mut app, &bad_path);
        assert!(app.dirty);
        assert!(app.save_error.is_some());

        auto_save_if_dirty(&mut app, &good_path);
        assert!(!app.dirty);
        assert!(app.save_error.is_none());
        Ok(())
    }

    #[test]
    fn app_filters_by_search_query() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Alpha task", TaskStatus::Todo),
                make_test_task("RQ-0002", "Beta task", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.set_search_query("alpha".to_string());

        assert_eq!(app.filtered_len(), 1);
        assert_eq!(
            app.selected_task().map(|task| task.id.as_str()),
            Some("RQ-0001")
        );
    }

    #[test]
    fn app_filters_by_tags() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task_with_tags("RQ-0001", "UI polish", vec!["ux", "tui"]),
                make_test_task_with_tags("RQ-0002", "Docs", vec!["docs"]),
            ],
        };
        let mut app = App::new(queue);
        app.set_tag_filters(vec!["tui".to_string()]);

        assert_eq!(app.filtered_len(), 1);
        assert_eq!(
            app.selected_task().map(|task| task.id.as_str()),
            Some("RQ-0001")
        );
    }

    #[test]
    fn selection_clamps_when_filtering() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Alpha", TaskStatus::Todo),
                make_test_task("RQ-0002", "Beta", TaskStatus::Todo),
                make_test_task("RQ-0003", "Gamma", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 2;
        app.scroll = 2;

        app.set_search_query("Alpha".to_string());

        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll, 0);
        assert_eq!(app.filtered_len(), 1);
        assert_eq!(
            app.selected_task().map(|task| task.id.as_str()),
            Some("RQ-0001")
        );
    }

    #[test]
    fn set_log_visible_lines_autoscrolls_to_bottom() {
        let mut app = App::new(QueueFile::default());
        app.logs = (0..50).map(|i| format!("line {}", i)).collect();
        app.autoscroll = true;
        app.log_scroll = 0;

        app.set_log_visible_lines(5);

        assert_eq!(app.log_visible_lines, 5);
        assert_eq!(app.log_scroll, 45);
    }

    #[test]
    fn set_log_visible_lines_clamps_scroll_when_out_of_bounds() {
        let mut app = App::new(QueueFile::default());
        app.logs = (0..50).map(|i| format!("line {}", i)).collect();
        app.autoscroll = false;
        app.log_scroll = 40;

        app.set_log_visible_lines(20);

        assert_eq!(app.log_visible_lines, 20);
        assert_eq!(app.log_scroll, 30);
    }

    #[test]
    fn parse_tags_splits_comma_separated() {
        let tags = App::parse_tags("tui,ux,docs");
        assert_eq!(tags, vec!["tui", "ux", "docs"]);
    }

    #[test]
    fn parse_tags_splits_whitespace_separated() {
        let tags = App::parse_tags("tui ux docs");
        assert_eq!(tags, vec!["tui", "ux", "docs"]);
    }

    #[test]
    fn parse_tags_handles_mixed_separators() {
        let tags = App::parse_tags("tui, ux docs");
        assert_eq!(tags, vec!["tui", "ux", "docs"]);
    }

    #[test]
    fn parse_tags_trims_whitespace() {
        let tags = App::parse_tags("  tui  ,  ux  ");
        assert_eq!(tags, vec!["tui", "ux"]);
    }

    #[test]
    fn parse_tags_filters_empty_tags() {
        let tags = App::parse_tags("tui,,ux, ,docs");
        assert_eq!(tags, vec!["tui", "ux", "docs"]);
    }

    #[test]
    fn rebuild_filtered_view_preserves_selection_with_preferred_id() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Alpha", TaskStatus::Todo),
                make_test_task("RQ-0002", "Beta", TaskStatus::Doing),
                make_test_task("RQ-0003", "Gamma", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 1; // Select RQ-0002

        // Filter to only Todo tasks - RQ-0002 should be filtered out
        app.filters.statuses = vec![TaskStatus::Todo];
        app.rebuild_filtered_view_with_preferred(Some("RQ-0002"));

        // RQ-0002 is filtered out, so selection should clamp to first available
        assert_eq!(app.filtered_len(), 2);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn rebuild_filtered_view_preserves_selection_when_task_still_visible() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Alpha", TaskStatus::Todo),
                make_test_task("RQ-0002", "Beta", TaskStatus::Doing),
                make_test_task("RQ-0003", "Gamma", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 1; // Select RQ-0002

        // Change filter but keep RQ-0002 visible
        app.filters.statuses = vec![TaskStatus::Doing];
        app.rebuild_filtered_view_with_preferred(Some("RQ-0002"));

        // RQ-0002 should still be selected
        assert_eq!(app.filtered_len(), 1);
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("RQ-0002"));
    }

    #[test]
    fn cycle_status_preserves_selection() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.selected = 0;

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();

        // Selection should be preserved
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("RQ-0001"));
    }

    #[test]
    fn delete_task_preserves_selection_of_next_task() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Todo),
                make_test_task("RQ-0003", "Task 3", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 0; // Select RQ-0001

        app.delete_selected_task().unwrap();

        // Should select RQ-0002 (next task)
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("RQ-0002"));
    }

    #[test]
    fn delete_task_preserves_selection_of_previous_when_at_end() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);
        app.selected = 1; // Select RQ-0002 (last task)

        app.delete_selected_task().unwrap();

        // Should select RQ-0001 (previous task)
        assert_eq!(app.selected, 0);
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("RQ-0001"));
    }

    #[test]
    fn clear_filters_restores_full_list() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Alpha", TaskStatus::Todo),
                make_test_task("RQ-0002", "Beta", TaskStatus::Doing),
                make_test_task("RQ-0003", "Gamma", TaskStatus::Done),
            ],
        };
        let mut app = App::new(queue);
        app.set_search_query("Alpha".to_string());
        app.set_tag_filters(vec!["test".to_string()]);
        app.cycle_status_filter();

        assert_eq!(app.filtered_len(), 1);

        app.clear_filters();

        assert_eq!(app.filtered_len(), 3);
        assert_eq!(app.queue.tasks.len(), 3);
    }
}
