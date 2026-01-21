//! Interactive Terminal UI for browsing and managing the task queue.
//!
//! The TUI provides a split-pane interface:
//! - Left panel: list of tasks with ID, status, priority, and title
//! - Right panel: detailed view of the selected task
//!
//! Key bindings:
//! - `q` / `Esc`: Quit
//! - `Up` / `Down` / `j` / `k`: Navigate task list
//! - `Enter`: Execute task (suspends TUI, runs task, restores)
//! - `d`: Delete task (with confirmation)
//! - `e`: Edit task title
//! - `s`: Cycle status (Draft → Todo → Doing → Done → Rejected → Draft)

use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;
use crate::timeutil;

pub mod events;
pub mod render;

pub use events::{handle_key_event, AppMode, TuiAction};
pub use render::draw_ui;

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
    /// Execution logs (when in Executing mode)
    pub logs: Vec<String>,
    /// Scroll offset for execution logs
    pub log_scroll: usize,
    /// Whether to auto-scroll execution logs
    pub autoscroll: bool,
    /// Height of the task list (for scrolling calculation)
    pub list_height: usize,
}

impl App {
    /// Create a new TUI app from a queue file.
    pub fn new(queue: QueueFile) -> Self {
        Self {
            queue,
            selected: 0,
            mode: AppMode::Normal,
            scroll: 0,
            detail_width: 60,
            dirty: false,
            logs: Vec::new(),
            log_scroll: 0,
            autoscroll: true,
            list_height: 20,
        }
    }

    /// Get the currently selected task, if any.
    pub fn selected_task(&self) -> Option<&Task> {
        self.queue.tasks.get(self.selected)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if !self.queue.tasks.is_empty() && self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll {
                self.scroll = self.selected;
            }
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self, list_height: usize) {
        if self.selected + 1 < self.queue.tasks.len() {
            self.selected += 1;
            // Scroll down if selection is below visible area
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected - list_height + 1;
            }
        }
    }

    /// Cycle the status of the selected task.
    pub fn cycle_status(&mut self, now_rfc3339: &str) -> Result<()> {
        let task = self
            .queue
            .tasks
            .get_mut(self.selected)
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
        Ok(())
    }

    /// Delete the selected task (returns the deleted task for confirmation).
    pub fn delete_selected_task(&mut self) -> Result<Task> {
        let task = self
            .queue
            .tasks
            .get(self.selected)
            .ok_or_else(|| anyhow!("No task selected"))?
            .clone();

        self.queue.tasks.remove(self.selected);

        // Adjust selection if needed
        if self.selected >= self.queue.tasks.len() && !self.queue.tasks.is_empty() {
            self.selected = self.queue.tasks.len() - 1;
        }

        self.dirty = true;
        Ok(task)
    }

    /// Update the title of the selected task.
    pub fn update_title(&mut self, new_title: String) -> Result<()> {
        let task = self
            .queue
            .tasks
            .get_mut(self.selected)
            .ok_or_else(|| anyhow!("No task selected"))?;

        if new_title.trim().is_empty() {
            bail!("Title cannot be empty");
        }

        task.title = new_title;
        self.dirty = true;
        Ok(())
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

/// Run the TUI application.
///
/// This function:
/// 1. Sets up the terminal for TUI mode
/// 2. Runs the interactive event loop
/// 3. Cleans up terminal state on exit
/// 4. Returns None (tasks are executed within TUI in Executing mode)
///
/// The `runner_factory` creates a closure that executes a task when called.
/// It receives a task ID and an output handler callback.
pub fn run_tui<F, E>(queue_path: &Path, runner_factory: F) -> Result<Option<String>>
where
    F: Fn(String, crate::runner::OutputHandler) -> E + Send + Sync + 'static,
    E: FnOnce() -> Result<()> + Send + 'static,
{
    // Load the queue
    let queue = queue::load_queue(queue_path)?;
    let app = App::new(queue.clone());

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
                .context("draw UI")
                .unwrap();

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
                            let visible_lines = 20; // Approximate
                            if app_ref.logs.len() > visible_lines {
                                app_ref.log_scroll = app_ref.logs.len() - visible_lines;
                            }
                        }
                    }
                    RunnerEvent::Finished => {
                        // Reload queue to capture any changes made by the runner (or agents)
                        match queue::load_queue(queue_path) {
                            Ok(new_queue) => {
                                app_ref.queue = new_queue;
                                // Clamp selection to new bounds
                                if app_ref.queue.tasks.is_empty() {
                                    app_ref.selected = 0;
                                } else if app_ref.selected >= app_ref.queue.tasks.len() {
                                    app_ref.selected = app_ref.queue.tasks.len() - 1;
                                }
                                app_ref.dirty = false;
                            }
                            Err(e) => {
                                app_ref.logs.push(format!("ERROR reloading queue: {}", e));
                            }
                        }

                        // Restore normal mode
                        if let AppMode::Executing { .. } = &app_ref.mode {
                            app_ref.mode = AppMode::Normal;
                        }
                    }
                    RunnerEvent::Error(msg) => {
                        app_ref.logs.push(format!("ERROR: {}", msg));
                        if let AppMode::Executing { .. } = &app_ref.mode {
                            app_ref.mode = AppMode::Normal;
                        }
                    }
                }
            }

            // Auto-save if dirty
            if app.borrow().dirty {
                let mut app_ref = app.borrow_mut();
                if let Err(e) = queue::save_queue(queue_path, &app_ref.queue) {
                    app_ref.logs.push(format!("ERROR saving queue: {}", e));
                    // Don't clear dirty flag so we retry? Or clear to avoid spam?
                    // Let's clear it to avoid infinite error loops in the UI
                }
                app_ref.dirty = false;
            }

            // Handle events with timeout (for polling runner events)
            if event::poll(Duration::from_millis(100))
                .context("poll event")
                .unwrap()
            {
                if let Event::Key(key) = event::read().context("read event").unwrap() {
                    // Ignore key release events
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }

                    let mut app_ref = app.borrow_mut();

                    // Use of extracted handle_key_event function
                    let now = timeutil::now_utc_rfc3339().unwrap();
                    match handle_key_event(&mut app_ref, key.code, &now).unwrap() {
                        TuiAction::Quit => break,
                        TuiAction::Continue => {}
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

// Rendering (draw/layout/color helpers) lives in `crate::tui::render`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority};

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

        app.update_title("New Title".to_string()).unwrap();
        assert_eq!(app.queue.tasks[0].title, "New Title");
        assert!(app.dirty);
    }

    #[test]
    fn app_update_title_rejects_empty_title() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        assert!(app.update_title("".to_string()).is_err());
        assert!(app.update_title("   ".to_string()).is_err());
    }
}
