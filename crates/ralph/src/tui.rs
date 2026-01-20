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
//! - `s`: Cycle status (Todo → Doing → Done → Rejected → Todo)

use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;
use crate::timeutil;

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
}

/// Actions that can result from handling a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    /// Continue running the TUI
    Continue,
    /// Exit the TUI
    Quit,
    /// Run a specific task (transitions to Executing mode)
    RunTask(String),
}

/// Interaction modes for the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    /// Normal navigation mode
    Normal,
    /// Editing task title
    EditingTitle(String),
    /// Confirming task deletion
    ConfirmDelete,
    /// Executing a task (live output view)
    Executing { task_id: String },
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
            TaskStatus::Todo => TaskStatus::Doing,
            TaskStatus::Doing => TaskStatus::Done,
            TaskStatus::Done => TaskStatus::Rejected,
            TaskStatus::Rejected => TaskStatus::Todo,
        };

        task.status = new_status;
        task.updated_at = Some(now_rfc3339.to_string());

        match new_status {
            TaskStatus::Done | TaskStatus::Rejected => {
                task.completed_at = Some(now_rfc3339.to_string());
            }
            TaskStatus::Todo | TaskStatus::Doing => {
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

/// Handle a key event and return the resulting action.
///
/// This function is the core of TUI interaction handling and is public
/// to allow testing without a full terminal setup.
pub fn handle_key_event(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::EditingTitle(ref current) => {
            handle_editing_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::ConfirmDelete => handle_confirm_delete_key(app, key),
        AppMode::Executing { .. } => handle_executing_mode_key(app, key),
    }
}

/// Handle key events in Normal mode.
fn handle_normal_mode_key(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => Ok(TuiAction::Quit),
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let list_height = 20;
            app.move_down(list_height);
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if let Some(task) = app.selected_task() {
                let task_id = task.id.clone();
                app.mode = AppMode::Executing {
                    task_id: task_id.clone(),
                };
                app.logs.clear();
                app.log_scroll = 0;
                Ok(TuiAction::RunTask(task_id))
            } else {
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Char('d') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('e') => {
            if let Some(task) = app.selected_task() {
                app.mode = AppMode::EditingTitle(task.title.clone());
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('s') => {
            let _ = app.cycle_status(now_rfc3339);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in EditingTitle mode.
fn handle_editing_mode_key(
    app: &mut App,
    key: KeyCode,
    current: &str,
    _now_rfc3339: &str,
) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            let new_title = current.to_string();
            let _ = app.update_title(new_title);
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut new_title = current.to_string();
            new_title.push(c);
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut new_title = current.to_string();
            new_title.pop();
            app.mode = AppMode::EditingTitle(new_title);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmDelete mode.
fn handle_confirm_delete_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let _ = app.delete_selected_task();
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in Executing mode.
fn handle_executing_mode_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
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
/// It receives the task ID and an output handler callback.
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

                    // Use the extracted handle_key_event function
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

/// Wrap text to fit within a given width.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    textwrap::wrap(text, width)
        .into_iter()
        .map(|s| s.into_owned())
        .collect()
}

/// Draw the main UI.
///
/// Public to allow testing with TestBackend.
pub fn draw_ui(f: &mut Frame<'_>, app: &mut App) {
    let size = f.area();

    // Handle Executing mode separately (full-screen output view)
    if matches!(app.mode, AppMode::Executing { .. }) {
        draw_execution_view(f, app, size);
        return;
    }

    // Main layout: split into left (task list) and right (details)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(size);

    // Left panel: task list
    draw_task_list(f, app, chunks[0]);

    // Right panel: task details
    draw_task_details(f, app, chunks[1]);

    // Draw confirmation dialog if in ConfirmDelete mode
    if app.mode == AppMode::ConfirmDelete {
        draw_confirm_dialog(f, size);
    }
}

/// Draw the execution view (full-screen output during task execution).
fn draw_execution_view(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let task_id = match &app.mode {
        AppMode::Executing { task_id } => task_id.clone(),
        _ => "Unknown".to_string(),
    };

    // Create a block with title
    let title = Line::from(vec![
        Span::styled("Executing: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(&task_id, Style::default().fg(Color::Cyan)),
        Span::raw(" "),
        Span::styled("(Esc to return)", Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Calculate visible log lines
    let visible_height = inner.height.saturating_sub(2) as usize; // Leave room for borders
    let log_count = app.logs.len();
    let start_idx = if app.log_scroll + visible_height > log_count {
        log_count.saturating_sub(visible_height)
    } else {
        app.log_scroll
    };

    // Get visible log lines
    let visible_logs: Vec<&String> = app
        .logs
        .iter()
        .skip(start_idx)
        .take(visible_height)
        .collect();

    // Render logs
    let log_text = Text::from(
        visible_logs
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>(),
    );

    let paragraph = Paragraph::new(log_text)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, inner);

    // Draw status indicator at bottom
    let status_line = if log_count > 0 {
        Line::from(vec![
            Span::raw("Lines: "),
            Span::styled(format!("{}", log_count), Style::default().fg(Color::Cyan)),
            Span::raw(" | Scroll: "),
            Span::styled(
                format!("{}/{}", app.log_scroll, log_count),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" | "),
            Span::styled("Auto-scroll: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if app.autoscroll { "ON" } else { "OFF" },
                Style::default().fg(if app.autoscroll {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ])
    } else {
        Line::from(vec![Span::styled(
            "Waiting for output...",
            Style::default().fg(Color::DarkGray),
        )])
    };

    let status_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };

    let status_paragraph = Paragraph::new(status_line);
    f.render_widget(status_paragraph, status_area);
}

/// Draw the task list panel.
fn draw_task_list(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let title = Line::from(vec![
        Span::styled("Tasks", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ("),
        Span::styled(
            format!("{}", app.queue.tasks.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(")"),
    ]);

    let items: Vec<ListItem> = app
        .queue
        .tasks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let is_selected = i == app.selected;
            let status_style = Style::default().fg(status_color(task.status));

            let line = if is_selected {
                Line::from(vec![
                    Span::styled(
                        "» ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&task.id, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(
                        task.status.as_str(),
                        status_style.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(task.priority.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&task.title, Style::default().add_modifier(Modifier::BOLD)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(&task.id, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(task.status.as_str(), status_style),
                    Span::raw(" "),
                    Span::styled(task.priority.as_str(), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(&task.title, Style::default()),
                ])
            };

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(list, area);

    // Draw selection indicator manually
    if !app.queue.tasks.is_empty() {
        let list_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let visible_count = list_height.min(app.queue.tasks.len());
        let selected_offset = app.selected.saturating_sub(app.scroll);

        if selected_offset < visible_count {
            let y = area.y + 1 + selected_offset as u16;
            let highlight_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            };
            f.render_widget(
                Paragraph::new("").block(Block::default().style(Style::default().bg(Color::Blue))),
                highlight_area,
            );
        }
    }
}

/// Draw the task details panel.
fn draw_task_details(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.detail_width = area.width.saturating_sub(4); // Account for borders

    let title = if let AppMode::EditingTitle(ref title) = &app.mode {
        Line::from(vec![
            Span::styled(
                "Edit Title: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("_", Style::default().fg(Color::Yellow)), // Cursor
        ])
    } else {
        Line::from(Span::styled(
            "Task Details",
            Style::default().add_modifier(Modifier::BOLD),
        ))
    };

    let block = Block::default().title(title).borders(Borders::ALL);
    f.render_widget(block, area);

    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });

    if let Some(task) = app.selected_task() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(&task.id, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    task.status.as_str(),
                    Style::default().fg(status_color(task.status)),
                ),
            ]),
            Line::from(vec![
                Span::styled("Priority: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    task.priority.as_str(),
                    Style::default().fg(priority_color(task.priority)),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Title", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]),
        ];

        // Title with word wrap
        for line in wrap_text(&task.title, app.detail_width as usize) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }

        // Tags
        if !task.tags.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Tags", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.tags.join(", "), Style::default().fg(Color::Cyan)),
            ]));
        }

        // Scope
        if !task.scope.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Scope", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(": ", Style::default()),
                Span::styled(task.scope.join(", "), Style::default().fg(Color::Green)),
            ]));
        }

        // Evidence
        if !task.evidence.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Evidence",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]));
            for item in &task.evidence {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  • {}", line),
                        Style::default(),
                    )));
                }
            }
        }

        // Plan
        if !task.plan.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Plan", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for (i, item) in task.plan.iter().enumerate() {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  {}. {}", i + 1, line),
                        Style::default(),
                    )));
                }
            }
        }

        // Notes
        if !task.notes.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Notes", Style::default().add_modifier(Modifier::UNDERLINED)),
                Span::styled(":", Style::default()),
            ]));
            for item in &task.notes {
                for line in wrap_text(item, app.detail_width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(
                        format!("  - {}", line),
                        Style::default().fg(Color::Yellow),
                    )));
                }
            }
        }

        // Dependencies
        if !task.depends_on.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Depends On",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(": ", Style::default()),
                Span::styled(
                    task.depends_on.join(", "),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }

        // Custom Fields
        if !task.custom_fields.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Custom Fields",
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ),
                Span::styled(":", Style::default()),
            ]));
            let mut sorted_fields: Vec<_> = task.custom_fields.iter().collect();
            sorted_fields.sort_by_key(|&(k, _)| k);
            for (key, value) in sorted_fields {
                for line in wrap_text(
                    &format!("  • {}: {}", key, value),
                    app.detail_width.saturating_sub(4) as usize,
                ) {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(Color::LightCyan),
                    )));
                }
            }
        }

        // Timestamps
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "Created",
                Style::default().add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(": ", Style::default()),
            Span::styled(
                task.created_at.as_deref().unwrap_or("N/A"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Updated",
                Style::default().add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled(": ", Style::default()),
            Span::styled(
                task.updated_at.as_deref().unwrap_or("N/A"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    } else {
        let text = Text::from(vec![
            Line::from(""),
            Line::from("No tasks in queue."),
            Line::from(""),
            Line::from("Create a task with:"),
            Line::from(Span::styled(
                "  ralph task build \"your request\"",
                Style::default().fg(Color::Cyan),
            )),
        ]);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    // Draw help footer at bottom of screen
    let help_text = match &app.mode {
        AppMode::Normal => vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":quit "),
            Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":nav "),
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":run "),
            Span::styled("d", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":del "),
            Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":edit "),
            Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":status"),
        ],
        AppMode::EditingTitle(_) => vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":save "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::ConfirmDelete => vec![
            Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":yes "),
            Span::styled("n", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":no "),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":cancel"),
        ],
        AppMode::Executing { .. } => vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(":return to list (task continues)"),
        ],
    };

    let help_paragraph = Paragraph::new(Line::from(help_text))
        .alignment(Alignment::Center)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let help_area = Rect {
        x: 0,
        y: f.area().height.saturating_sub(1),
        width: f.area().width,
        height: 1,
    };
    f.render_widget(help_paragraph, help_area);
}

/// Draw the confirmation dialog for task deletion.
fn draw_confirm_dialog(f: &mut Frame<'_>, area: Rect) {
    let popup_width = 40.min(area.width.saturating_sub(4));
    let popup_height = 6;

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Delete this task? ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("(y/n)", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray)),
    )
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false });

    f.render_widget(popup, popup_area);
}

/// Get the color for a task status.
fn status_color(status: TaskStatus) -> Color {
    match status {
        TaskStatus::Todo => Color::Blue,
        TaskStatus::Doing => Color::Yellow,
        TaskStatus::Done => Color::Green,
        TaskStatus::Rejected => Color::Red,
    }
}

/// Get the color for a task priority.
fn priority_color(priority: crate::contracts::TaskPriority) -> Color {
    use crate::contracts::TaskPriority;
    match priority {
        TaskPriority::Critical => Color::Red,
        TaskPriority::High => Color::Yellow,
        TaskPriority::Medium => Color::Blue,
        TaskPriority::Low => Color::DarkGray,
    }
}

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
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Doing);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Done);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);

        app.cycle_status("2026-01-19T00:00:00Z").unwrap();
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Todo);
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
