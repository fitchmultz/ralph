//! TUI runtime loop and runner orchestration.
//!
//! Responsibilities:
//! - Own the terminal lifecycle (raw mode, alternate screen, mouse capture).
//! - Run the main draw + input loop.
//! - Spawn runner/scanner/task-builder work and translate output into UI updates.
//!
//! Not handled here:
//! - App state definition (see `tui::app`).
//! - Persistence policies and queue reload primitives (see `tui::app_session` + `tui::app_reload`).
//!
//! Invariants/assumptions:
//! - Terminal is always restored on exit (even on early-return / error / panic).
//! - Runner events are processed serially on the UI thread.

use crate::config::Resolved;
use crate::progress::ExecutionPhase;
use crate::tui::app_reload::ReloadOperations;
use crate::tui::app_resize::ResizeOperations;
use crate::tui::app_session::{auto_save_app_if_dirty, prepare_tui_session};
use crate::tui::events::{handle_key_event, handle_mouse_event};
use crate::tui::external_tools;
use crate::tui::input::TextInput;
use crate::tui::render::draw_ui;
use crate::tui::revert_prompt::make_tui_revert_prompt_handler;
use crate::tui::terminal::{BorderStyle, TerminalCapabilities};
use crate::tui::{App, AppMode, RunningKind, TuiAction, TuiOptions};
use crate::{config as crate_config, runutil, timeutil};
use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

/// Event sent from the runner thread to the TUI.
#[derive(Debug)]
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

/// Terminal session guard for cleanup on drop.
struct TerminalSession {
    enable_mouse: bool,
}

impl TerminalSession {
    fn new(enable_mouse: bool) -> Self {
        Self { enable_mouse }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
        if self.enable_mouse {
            let _ = execute!(stdout, DisableMouseCapture);
        }
    }
}

/// Handle a runner event and return the next task ID to start if in loop mode.
///
/// This function processes runner events and updates the app state accordingly.
/// It returns `Some(task_id)` when loop mode wants to start another task.
pub(in crate::tui) fn handle_runner_event(
    app: &mut App,
    event: RunnerEvent,
    queue_path: &Path,
    done_path: &Path,
) -> Option<String> {
    match event {
        RunnerEvent::Output(text) => {
            let lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
            // Process each line for phase detection
            if app.running_kind == Some(RunningKind::Task) {
                for line in &lines {
                    app.process_log_line_for_phase(line);
                }
            }
            app.append_log_lines(lines);
            None
        }
        RunnerEvent::Finished => {
            app.runner_active = false;
            // Capture the task ID before clearing it (needed for execution history)
            let finished_task_id = app.running_task_id.clone();
            app.running_task_id = None;
            // Mark execution as complete for phase tracking
            if app.running_kind == Some(RunningKind::Task) {
                app.transition_to_phase(ExecutionPhase::Complete);
            }
            let running_kind = app.running_kind.take();

            match running_kind {
                Some(RunningKind::Scan { .. }) => {
                    app.on_scan_finished(queue_path, done_path);
                }
                Some(RunningKind::TaskBuilder) => {
                    app.on_task_builder_finished(queue_path, done_path);
                }
                Some(RunningKind::Task) | None => {
                    // Record execution history for completed task (only if Done)
                    if let Some(ref task_id) = finished_task_id {
                        app.record_execution_history_for_task(task_id, done_path);
                    }

                    app.reload_queues_from_disk(queue_path, done_path);

                    if app.mode == AppMode::ConfirmQuit {
                        app.mode = AppMode::Normal;
                    }

                    if app.loop_active {
                        if app.loop_arm_after_current {
                            app.loop_arm_after_current = false;
                        } else {
                            app.loop_ran = app.loop_ran.saturating_add(1);
                        }

                        if let Some(max) = app.loop_max_tasks
                            && app.loop_ran >= max
                        {
                            let loop_ran = app.loop_ran;
                            app.loop_active = false;
                            app.set_status_message(format!(
                                "Loop finished (ran {}/{})",
                                loop_ran, max
                            ));
                        }

                        if app.loop_active {
                            if let Some(next_id) = app.next_loop_task_id() {
                                let focus_logs = matches!(app.mode, AppMode::Executing { .. });
                                app.start_task_execution(next_id.clone(), focus_logs, true);
                                return Some(next_id);
                            } else {
                                let loop_ran = app.loop_ran;
                                app.loop_active = false;
                                app.set_status_message(format!("Loop complete (ran {})", loop_ran));
                            }
                        }
                    } else if matches!(app.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
                        app.mode = AppMode::Normal;
                    }
                }
            }
            None
        }
        RunnerEvent::Error(msg) => {
            app.runner_active = false;
            app.running_task_id = None;
            let running_kind = app.running_kind.take();

            app.loop_active = false;
            app.loop_arm_after_current = false;

            match running_kind {
                Some(RunningKind::Scan { .. }) => {
                    app.on_scan_error(&msg);
                }
                Some(RunningKind::TaskBuilder) => {
                    app.on_task_builder_error(&msg);
                }
                Some(RunningKind::Task) | None => {
                    app.set_runner_error(&msg);
                    if matches!(app.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
                        app.mode = AppMode::Normal;
                    }
                }
            }
            None
        }
        RunnerEvent::RevertPrompt {
            label,
            preface,
            allow_proceed,
            reply,
        } => {
            let previous_mode = app.mode.clone();
            app.mode = AppMode::ConfirmRevert {
                label,
                preface,
                allow_proceed,
                selected: 0,
                input: TextInput::new(""),
                reply_sender: reply,
                previous_mode: Box::new(previous_mode),
            };
            None
        }
    }
}

/// Run the TUI application with an active queue lock.
pub fn run_tui<F, E, S, SE>(
    resolved: &Resolved,
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

    // Create the cleanup guard - this ensures cleanup happens on panic/unwind
    let _session_guard = TerminalSession::new(enable_mouse);

    // Create channels for runner events.
    let (tx, rx) = mpsc::channel::<RunnerEvent>();

    let build_handlers = |tx: &mpsc::Sender<RunnerEvent>| {
        let tx_clone_for_handler = tx.clone();
        let handler: crate::runner::OutputHandler = Arc::new(Box::new(move |text: &str| {
            let _ = tx_clone_for_handler.send(RunnerEvent::Output(text.to_string()));
        }));

        let revert_prompt = make_tui_revert_prompt_handler(tx.clone());

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
                    let repo_root = external_tools::repo_root_from_queue_path(queue_path);
                    let paths = external_tools::resolve_scope_paths(repo_root.as_deref(), &scope);

                    match external_tools::open_paths_in_editor(&paths) {
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
                    match external_tools::copy_text_to_clipboard(&text) {
                        Ok(()) => {
                            app_ref.set_status_message("Copied file:line reference(s) to clipboard")
                        }
                        Err(e) => app_ref.set_status_message(format!("Copy failed: {}", e)),
                    }
                    Ok(false)
                }
                TuiAction::OpenUrlInBrowser(url) => {
                    match external_tools::open_url_in_browser(&url) {
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
                if let Some(id) = handle_runner_event(&mut app_ref, event, queue_path, done_path) {
                    next_to_start = Some(id);
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
                auto_save_app_if_dirty(&mut app_ref, queue_path, done_path, config_path.as_deref());
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

    // Cleanup is handled by TerminalSession Drop impl, but we also do it explicitly
    // here to ensure proper ordering with the terminal restoration
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use tempfile::TempDir;

    fn create_test_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Test task {}", id),
            status: TaskStatus::Todo,
            ..Default::default()
        }
    }

    fn create_test_app() -> App {
        let queue = QueueFile {
            tasks: vec![
                create_test_task("RQ-0001"),
                create_test_task("RQ-0002"),
                create_test_task("RQ-0003"),
            ],
            ..Default::default()
        };
        App::new(queue)
    }

    #[test]
    fn test_handle_runner_event_output_appends_logs() {
        let mut app = create_test_app();
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        // Initialize queue files
        std::fs::write(&queue_path, r#"{"tasks":[]}"#).unwrap();
        std::fs::write(&done_path, r#"{"tasks":[]}"#).unwrap();

        // Set up runner state
        app.runner_active = true;
        app.running_kind = Some(RunningKind::Task);

        let result = handle_runner_event(
            &mut app,
            RunnerEvent::Output("Test log line\nAnother line".to_string()),
            &queue_path,
            &done_path,
        );

        assert!(result.is_none());
        assert_eq!(app.logs.len(), 2);
        assert_eq!(app.logs[0], "Test log line");
        assert_eq!(app.logs[1], "Another line");
    }

    #[test]
    fn test_handle_runner_event_output_triggers_phase_detection() {
        let mut app = create_test_app();
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        std::fs::write(&queue_path, r#"{"tasks":[]}"#).unwrap();
        std::fs::write(&done_path, r#"{"tasks":[]}"#).unwrap();

        // Set up runner state for task execution
        app.runner_active = true;
        app.running_kind = Some(RunningKind::Task);
        app.reset_phase_tracking(3);

        assert_eq!(app.execution_phase, ExecutionPhase::Planning);

        handle_runner_event(
            &mut app,
            RunnerEvent::Output("# IMPLEMENTATION MODE\nStarting work".to_string()),
            &queue_path,
            &done_path,
        );

        assert_eq!(app.execution_phase, ExecutionPhase::Implementation);
    }

    #[test]
    fn test_handle_runner_event_finished_disables_runner() {
        let mut app = create_test_app();
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        std::fs::write(&queue_path, r#"{"tasks":[]}"#).unwrap();
        std::fs::write(&done_path, r#"{"tasks":[]}"#).unwrap();

        app.runner_active = true;
        app.running_task_id = Some("RQ-0001".to_string());
        app.running_kind = Some(RunningKind::Task);

        let result = handle_runner_event(&mut app, RunnerEvent::Finished, &queue_path, &done_path);

        assert!(result.is_none());
        assert!(!app.runner_active);
        assert!(app.running_task_id.is_none());
        assert!(app.running_kind.is_none());
    }

    #[test]
    fn test_handle_runner_event_error_disables_loop() {
        let mut app = create_test_app();
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        std::fs::write(&queue_path, r#"{"tasks":[]}"#).unwrap();
        std::fs::write(&done_path, r#"{"tasks":[]}"#).unwrap();

        app.runner_active = true;
        app.loop_active = true;
        app.running_kind = Some(RunningKind::Task);

        handle_runner_event(
            &mut app,
            RunnerEvent::Error("Test error".to_string()),
            &queue_path,
            &done_path,
        );

        assert!(!app.runner_active);
        assert!(!app.loop_active);
        assert!(!app.loop_arm_after_current);
    }

    #[test]
    fn test_handle_runner_event_revert_prompt_changes_mode() {
        let mut app = create_test_app();
        let temp_dir = TempDir::new().unwrap();
        let queue_path = temp_dir.path().join("queue.json");
        let done_path = temp_dir.path().join("done.json");

        std::fs::write(&queue_path, r#"{"tasks":[]}"#).unwrap();
        std::fs::write(&done_path, r#"{"tasks":[]}"#).unwrap();

        app.mode = AppMode::Normal;

        let (reply_tx, _reply_rx) = mpsc::channel();

        let result = handle_runner_event(
            &mut app,
            RunnerEvent::RevertPrompt {
                label: "test-label".to_string(),
                preface: Some("test preface".to_string()),
                allow_proceed: true,
                reply: reply_tx,
            },
            &queue_path,
            &done_path,
        );

        assert!(result.is_none());
        assert!(matches!(app.mode, AppMode::ConfirmRevert { .. }));
    }
}
