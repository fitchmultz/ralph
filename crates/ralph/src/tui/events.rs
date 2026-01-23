//! TUI event handling extracted from `crate::tui`.
//!
//! This module contains all key-event dispatch and per-mode handlers.
//! Public API is preserved via `crate::tui` re-exporting:
//! - `AppMode`
//! - `TuiAction`
//! - `handle_key_event`
//!
//! The interaction model is intentionally user-centric:
//! - `:` opens a command palette (discoverability)
//! - `l` toggles loop mode (auto-run tasks)
//! - `a` archives terminal tasks (done/rejected) with confirmation
//! - `?`/`h` shows the help overlay

use anyhow::Result;
use crossterm::event::KeyCode;
use std::sync::mpsc;

use super::App;
use crate::runutil::RevertDecision;

/// Actions that can result from handling a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    /// Continue running the TUI
    Continue,
    /// Exit the TUI
    Quit,
    /// Reload the queue from disk
    ReloadQueue,
    /// Run a scan with the provided focus string.
    RunScan(String),
    /// Run a specific task (transitions to Executing mode)
    RunTask(String),
}

/// Interaction modes for the TUI.
#[derive(Debug, Clone)]
pub enum AppMode {
    /// Normal navigation mode
    Normal,
    /// Full-screen help overlay
    Help,
    /// Editing task fields
    EditingTask {
        selected: usize,
        editing_value: Option<String>,
    },
    /// Creating a new task (title input)
    CreatingTask(String),
    /// Searching tasks (query input)
    Searching(String),
    /// Filtering tasks by tag list (comma-separated input)
    FilteringTags(String),
    /// Editing project configuration
    EditingConfig {
        selected: usize,
        editing_value: Option<String>,
    },
    /// Running a scan (focus input)
    Scanning(String),
    /// Command palette (":" style)
    CommandPalette { query: String, selected: usize },
    /// Confirming task deletion
    ConfirmDelete,
    /// Confirming archive of done/rejected tasks
    ConfirmArchive,
    /// Confirming quit while a task is running
    ConfirmQuit,
    /// Confirming revert of uncommitted changes.
    ConfirmRevert {
        label: String,
        reply_sender: mpsc::Sender<RevertDecision>,
        previous_mode: Box<AppMode>,
    },
    /// Executing a task (live output view)
    Executing { task_id: String },
}

impl PartialEq for AppMode {
    fn eq(&self, other: &Self) -> bool {
        use AppMode::*;
        match (self, other) {
            (Normal, Normal) => true,
            (Help, Help) => true,
            (
                EditingTask {
                    selected: left_selected,
                    editing_value: left_value,
                },
                EditingTask {
                    selected: right_selected,
                    editing_value: right_value,
                },
            ) => left_selected == right_selected && left_value == right_value,
            (CreatingTask(left), CreatingTask(right)) => left == right,
            (Searching(left), Searching(right)) => left == right,
            (FilteringTags(left), FilteringTags(right)) => left == right,
            (
                EditingConfig {
                    selected: left_selected,
                    editing_value: left_value,
                },
                EditingConfig {
                    selected: right_selected,
                    editing_value: right_value,
                },
            ) => left_selected == right_selected && left_value == right_value,
            (Scanning(left), Scanning(right)) => left == right,
            (
                CommandPalette {
                    query: left_query,
                    selected: left_selected,
                },
                CommandPalette {
                    query: right_query,
                    selected: right_selected,
                },
            ) => left_query == right_query && left_selected == right_selected,
            (ConfirmDelete, ConfirmDelete) => true,
            (ConfirmArchive, ConfirmArchive) => true,
            (ConfirmQuit, ConfirmQuit) => true,
            (
                ConfirmRevert {
                    label: left_label,
                    previous_mode: left_previous,
                    ..
                },
                ConfirmRevert {
                    label: right_label,
                    previous_mode: right_previous,
                    ..
                },
            ) => left_label == right_label && left_previous == right_previous,
            (Executing { task_id: left_id }, Executing { task_id: right_id }) => {
                left_id == right_id
            }
            _ => false,
        }
    }
}

impl Eq for AppMode {}

/// High-level commands available in the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCommand {
    RunSelected,
    RunNextRunnable,
    ToggleLoop,
    ArchiveTerminal,
    NewTask,
    EditTask,
    EditConfig,
    ScanRepo,
    Search,
    FilterTags,
    ClearFilters,
    CycleStatus,
    CyclePriority,
    ReloadQueue,
    Quit,
}

/// A single palette entry, already filtered and ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub cmd: PaletteCommand,
    pub title: String,
}

/// Handle a key event and return the resulting action.
///
/// This function is the core of TUI interaction handling and is public
/// to allow testing without a full terminal setup.
pub fn handle_key_event(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mode_key(app, key, now_rfc3339),
        AppMode::Help => handle_help_mode_key(app, key),
        AppMode::EditingTask {
            selected,
            editing_value,
        } => handle_editing_task_key(app, key, selected, editing_value, now_rfc3339),
        AppMode::CreatingTask(ref current) => {
            handle_creating_mode_key(app, key, current, now_rfc3339)
        }
        AppMode::Searching(ref current) => handle_searching_mode_key(app, key, current),
        AppMode::FilteringTags(ref current) => handle_filtering_tags_key(app, key, current),
        AppMode::EditingConfig {
            selected,
            editing_value,
        } => handle_editing_config_key(app, key, selected, editing_value),
        AppMode::Scanning(ref current) => handle_scanning_mode_key(app, key, current),
        AppMode::CommandPalette { query, selected } => {
            handle_command_palette_key(app, key, &query, selected, now_rfc3339)
        }
        AppMode::ConfirmDelete => handle_confirm_delete_key(app, key),
        AppMode::ConfirmArchive => handle_confirm_archive_key(app, key, now_rfc3339),
        AppMode::ConfirmQuit => handle_confirm_quit_key(app, key),
        AppMode::ConfirmRevert {
            label,
            reply_sender,
            previous_mode,
        } => handle_confirm_revert_key(app, key, &label, reply_sender, *previous_mode),
        AppMode::Executing { .. } => handle_executing_mode_key(app, key),
    }
}

/// Handle key events in Normal mode.
fn handle_normal_mode_key(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Char('?') | KeyCode::Char('h') => {
            app.mode = AppMode::Help;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(':') => {
            app.mode = AppMode::CommandPalette {
                query: String::new(),
                selected: 0,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('q') | KeyCode::Esc => {
            app.execute_palette_command(PaletteCommand::Quit, now_rfc3339)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_up();
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let list_height = app.list_height;
            app.move_down(list_height);
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => app.execute_palette_command(PaletteCommand::RunSelected, now_rfc3339),
        KeyCode::Char('l') => app.execute_palette_command(PaletteCommand::ToggleLoop, now_rfc3339),
        KeyCode::Char('a') => {
            app.execute_palette_command(PaletteCommand::ArchiveTerminal, now_rfc3339)
        }
        KeyCode::Char('d') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::ConfirmDelete;
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('e') => {
            if app.selected_task().is_some() {
                app.mode = AppMode::EditingTask {
                    selected: 0,
                    editing_value: None,
                };
            } else {
                app.set_status_message("No task selected");
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('c') => {
            app.mode = AppMode::EditingConfig {
                selected: 0,
                editing_value: None,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('g') => {
            if app.runner_active {
                app.set_status_message("Runner already active");
            } else {
                app.mode = AppMode::Scanning(String::new());
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') => {
            app.mode = AppMode::CreatingTask(String::new());
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('/') => {
            app.mode = AppMode::Searching(app.filters.query.clone());
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('t') => {
            app.mode = AppMode::FilteringTags(app.filters.tags.join(","));
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('f') => {
            app.cycle_status_filter();
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('x') => {
            app.clear_filters();
            app.set_status_message("Filters cleared");
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('s') => app.execute_palette_command(PaletteCommand::CycleStatus, now_rfc3339),
        KeyCode::Char('p') => {
            app.execute_palette_command(PaletteCommand::CyclePriority, now_rfc3339)
        }
        KeyCode::Char('r') => Ok(TuiAction::ReloadQueue),
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in Help mode.
fn handle_help_mode_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in EditingTask mode.
fn handle_editing_task_key(
    app: &mut App,
    key: KeyCode,
    selected: usize,
    editing_value: Option<String>,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    let entries = app.task_edit_entries();
    if entries.is_empty() {
        app.mode = AppMode::Normal;
        app.set_status_message("No task fields available");
        return Ok(TuiAction::Continue);
    }
    let max_index = entries.len().saturating_sub(1);
    let selected = selected.min(max_index);
    let entry = entries[selected].clone();

    if let Some(mut value) = editing_value {
        match key {
            KeyCode::Enter => match app.apply_task_edit(entry.key, &value, now_rfc3339) {
                Ok(()) => {
                    app.mode = AppMode::EditingTask {
                        selected,
                        editing_value: None,
                    };
                    Ok(TuiAction::Continue)
                }
                Err(e) => {
                    app.set_status_message(format!("Error: {}", e));
                    app.mode = AppMode::EditingTask {
                        selected,
                        editing_value: Some(value),
                    };
                    Ok(TuiAction::Continue)
                }
            },
            KeyCode::Esc => {
                app.mode = AppMode::EditingTask {
                    selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(c) => {
                value.push(c);
                app.mode = AppMode::EditingTask {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Backspace => {
                value.pop();
                app.mode = AppMode::EditingTask {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    } else {
        match key {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
                Ok(TuiAction::Continue)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                match entry.kind {
                    crate::tui::TaskEditKind::Cycle => {
                        if let Err(e) = app.apply_task_edit(entry.key, "", now_rfc3339) {
                            app.set_status_message(format!("Error: {}", e));
                        }
                        app.mode = AppMode::EditingTask {
                            selected,
                            editing_value: None,
                        };
                    }
                    crate::tui::TaskEditKind::Text
                    | crate::tui::TaskEditKind::List
                    | crate::tui::TaskEditKind::Map
                    | crate::tui::TaskEditKind::OptionalText => {
                        let current = app.task_value_for_edit(entry.key);
                        app.mode = AppMode::EditingTask {
                            selected,
                            editing_value: Some(current),
                        };
                    }
                }
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('x') => {
                match entry.kind {
                    crate::tui::TaskEditKind::Cycle => {}
                    crate::tui::TaskEditKind::Text
                    | crate::tui::TaskEditKind::List
                    | crate::tui::TaskEditKind::Map
                    | crate::tui::TaskEditKind::OptionalText => {
                        if let Err(e) = app.apply_task_edit(entry.key, "", now_rfc3339) {
                            app.set_status_message(format!("Error: {}", e));
                        }
                    }
                }
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(c) => {
                match entry.kind {
                    crate::tui::TaskEditKind::Text
                    | crate::tui::TaskEditKind::List
                    | crate::tui::TaskEditKind::Map
                    | crate::tui::TaskEditKind::OptionalText => {
                        let mut current = app.task_value_for_edit(entry.key);
                        current.push(c);
                        app.mode = AppMode::EditingTask {
                            selected,
                            editing_value: Some(current),
                        };
                    }
                    crate::tui::TaskEditKind::Cycle => {}
                }
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    }
}

/// Handle key events in CreatingTask mode.
fn handle_creating_mode_key(
    app: &mut App,
    key: KeyCode,
    current: &str,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            if let Err(e) = app.create_task_from_title(current, now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut new_title = current.to_string();
            new_title.push(c);
            app.mode = AppMode::CreatingTask(new_title);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut new_title = current.to_string();
            new_title.pop();
            app.mode = AppMode::CreatingTask(new_title);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in Searching mode.
fn handle_searching_mode_key(app: &mut App, key: KeyCode, current: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            app.set_search_query(current.to_string());
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut next = current.to_string();
            next.push(c);
            app.mode = AppMode::Searching(next);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = current.to_string();
            next.pop();
            app.mode = AppMode::Searching(next);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in FilteringTags mode.
fn handle_filtering_tags_key(app: &mut App, key: KeyCode, current: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            let tags = App::parse_tags(current);
            app.set_tag_filters(tags);
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut next = current.to_string();
            next.push(c);
            app.mode = AppMode::FilteringTags(next);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = current.to_string();
            next.pop();
            app.mode = AppMode::FilteringTags(next);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

fn handle_editing_config_key(
    app: &mut App,
    key: KeyCode,
    selected: usize,
    editing_value: Option<String>,
) -> Result<TuiAction> {
    let entries = app.config_entries();
    if entries.is_empty() {
        app.mode = AppMode::Normal;
        app.set_status_message("No config fields available");
        return Ok(TuiAction::Continue);
    }
    let max_index = entries.len().saturating_sub(1);
    let selected = selected.min(max_index);
    let entry = entries[selected].clone();

    if let Some(mut value) = editing_value {
        match key {
            KeyCode::Enter => match app.apply_config_text_value(entry.key, &value) {
                Ok(()) => {
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: None,
                    };
                    app.set_status_message("Config updated");
                    Ok(TuiAction::Continue)
                }
                Err(e) => {
                    app.set_status_message(format!("Error: {}", e));
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: Some(value),
                    };
                    Ok(TuiAction::Continue)
                }
            },
            KeyCode::Esc => {
                app.mode = AppMode::EditingConfig {
                    selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(c) => {
                value.push(c);
                app.mode = AppMode::EditingConfig {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Backspace => {
                value.pop();
                app.mode = AppMode::EditingConfig {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    } else {
        match key {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
                Ok(TuiAction::Continue)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if entry.kind == crate::tui::ConfigFieldKind::Text {
                    let current = app.config_value_for_edit(entry.key);
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: Some(current),
                    };
                } else {
                    app.cycle_config_value(entry.key);
                    app.set_status_message("Config updated");
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: None,
                    };
                }
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('x') => {
                app.clear_config_value(entry.key);
                app.set_status_message("Config cleared");
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(c) => {
                if entry.kind == crate::tui::ConfigFieldKind::Text {
                    let mut current = app.config_value_for_edit(entry.key);
                    current.push(c);
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: Some(current),
                    };
                }
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    }
}

/// Handle key events in Scanning mode.
fn handle_scanning_mode_key(app: &mut App, key: KeyCode, current: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Enter => {
            if app.runner_active {
                app.set_status_message("Runner already active");
                return Ok(TuiAction::Continue);
            }
            let focus = current.trim().to_string();
            app.mode = AppMode::Normal;
            Ok(TuiAction::RunScan(focus))
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut next = current.to_string();
            next.push(c);
            app.mode = AppMode::Scanning(next);
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = current.to_string();
            next.pop();
            app.mode = AppMode::Scanning(next);
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in CommandPalette mode.
fn handle_command_palette_key(
    app: &mut App,
    key: KeyCode,
    query: &str,
    selected: usize,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    let entries = app.palette_entries(query);
    let max_index = entries.len().saturating_sub(1);
    let selected = selected.min(max_index);

    match key {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if let Some(entry) = entries.get(selected) {
                app.mode = AppMode::Normal;
                app.execute_palette_command(entry.cmd, now_rfc3339)
            } else {
                app.mode = AppMode::Normal;
                app.set_status_message("No matching command");
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let next_selected = selected.saturating_sub(1);
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let next_selected = if entries.is_empty() {
                0
            } else {
                (selected + 1).min(max_index)
            };
            app.mode = AppMode::CommandPalette {
                query: query.to_string(),
                selected: next_selected,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Char(c) => {
            let mut next = query.to_string();
            next.push(c);
            app.mode = AppMode::CommandPalette {
                query: next,
                selected: 0,
            };
            Ok(TuiAction::Continue)
        }
        KeyCode::Backspace => {
            let mut next = query.to_string();
            next.pop();
            app.mode = AppMode::CommandPalette {
                query: next,
                selected: 0,
            };
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmDelete mode.
fn handle_confirm_delete_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Err(e) = app.delete_selected_task() {
                app.set_status_message(format!("Error: {}", e));
            }
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

/// Handle key events in ConfirmArchive mode.
fn handle_confirm_archive_key(app: &mut App, key: KeyCode, now_rfc3339: &str) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            if let Err(e) = app.archive_terminal_tasks(now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
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

/// Handle key events in ConfirmQuit mode.
fn handle_confirm_quit_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    match key {
        KeyCode::Char('y') | KeyCode::Char('Y') => Ok(TuiAction::Quit),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmRevert mode.
fn handle_confirm_revert_key(
    app: &mut App,
    key: KeyCode,
    label: &str,
    reply_sender: mpsc::Sender<RevertDecision>,
    previous_mode: AppMode,
) -> Result<TuiAction> {
    let decision = match key {
        KeyCode::Enter | KeyCode::Char('1') | KeyCode::Char('y') | KeyCode::Char('Y') => {
            RevertDecision::Revert
        }
        KeyCode::Char('2') | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            RevertDecision::Keep
        }
        _ => return Ok(TuiAction::Continue),
    };

    if reply_sender.send(decision).is_err() {
        app.set_status_message(format!("{label}: revert prompt expired"));
    } else if decision == RevertDecision::Revert {
        app.set_status_message(format!("{label}: reverting uncommitted changes"));
    } else {
        app.set_status_message(format!("{label}: keeping uncommitted changes"));
    }

    app.mode = previous_mode;
    Ok(TuiAction::Continue)
}

/// Handle key events in Executing mode.
fn handle_executing_mode_key(app: &mut App, key: KeyCode) -> Result<TuiAction> {
    let visible_lines = app.log_visible_lines();
    let page_lines = visible_lines.saturating_sub(1).max(1);
    match key {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_logs_up(1);
            Ok(TuiAction::Continue)
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_logs_down(1, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageUp => {
            app.scroll_logs_up(page_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::PageDown => {
            app.scroll_logs_down(page_lines, visible_lines);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('a') => {
            if app.autoscroll {
                app.autoscroll = false;
            } else {
                app.enable_autoscroll(visible_lines);
            }
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('l') => {
            if app.loop_active {
                app.loop_active = false;
                app.loop_arm_after_current = false;
                app.set_status_message("Loop stopped");
            }
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};

    fn make_test_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            title: "Test task".to_string(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: Some("2026-01-19T00:00:00Z".to_string()),
            updated_at: Some("2026-01-19T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn quit_when_not_running_exits_immediately() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action = handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Quit);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn quit_when_running_requires_confirmation() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.runner_active = true;

        let action = handle_key_event(&mut app, KeyCode::Char('q'), "2026-01-19T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::ConfirmQuit);
    }

    #[test]
    fn confirm_quit_accepts_yes() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::ConfirmQuit;

        let action = handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-19T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Quit);
    }

    #[test]
    fn loop_key_starts_loop_and_runs_next_runnable() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action = handle_key_event(&mut app, KeyCode::Char('l'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
        assert!(app.loop_active);
        assert!(app.runner_active);
    }

    #[test]
    fn archive_flow_enters_confirm_mode_then_moves_tasks() {
        let mut done_task = make_test_task("RQ-0001");
        done_task.status = TaskStatus::Done;
        done_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());

        let queue = QueueFile {
            version: 1,
            tasks: vec![done_task, make_test_task("RQ-0002")],
        };
        let mut app = App::new(queue);

        // Enter confirm archive.
        let action = handle_key_event(&mut app, KeyCode::Char('a'), "2026-01-20T00:00:00Z")
            .expect("handle key");
        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::ConfirmArchive);

        // Confirm.
        let action = handle_key_event(&mut app, KeyCode::Char('y'), "2026-01-20T00:00:00Z")
            .expect("handle key");
        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);

        assert_eq!(app.queue.tasks.len(), 1);
        assert_eq!(app.queue.tasks[0].id, "RQ-0002");
        assert_eq!(app.done.tasks.len(), 1);
        assert_eq!(app.done.tasks[0].id, "RQ-0001");
        assert!(app.dirty);
        assert!(app.dirty_done);
    }

    #[test]
    fn colon_enters_command_palette() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action = handle_key_event(&mut app, KeyCode::Char(':'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        match app.mode {
            AppMode::CommandPalette { .. } => {}
            other => panic!("expected command palette, got {:?}", other),
        }
    }

    #[test]
    fn help_key_enters_help_mode() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action = handle_key_event(&mut app, KeyCode::Char('?'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Help);
    }

    #[test]
    fn help_key_enters_help_mode_with_h() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action = handle_key_event(&mut app, KeyCode::Char('h'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Help);
    }

    #[test]
    fn help_mode_closes_on_escape() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Help;

        let action =
            handle_key_event(&mut app, KeyCode::Esc, "2026-01-20T00:00:00Z").expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn help_mode_closes_on_h() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Help;

        let action = handle_key_event(&mut app, KeyCode::Char('h'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn help_mode_closes_on_question_mark() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Help;

        let action = handle_key_event(&mut app, KeyCode::Char('?'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn help_mode_ignores_unrelated_keys() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Help;

        let action = handle_key_event(&mut app, KeyCode::Char('x'), "2026-01-20T00:00:00Z")
            .expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Help);
    }

    #[test]
    fn command_palette_runs_selected_command() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::CommandPalette {
            query: "run selected".to_string(),
            selected: 0,
        };

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

        assert_eq!(action, TuiAction::RunTask("RQ-0001".to_string()));
        assert!(app.runner_active);
    }

    #[test]
    fn command_palette_with_no_matches_sets_status_message() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::CommandPalette {
            query: "nope".to_string(),
            selected: 0,
        };

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("handle key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
        assert_eq!(app.status_message.as_deref(), Some("No matching command"));
    }

    #[test]
    fn c_enters_config_mode() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action =
            handle_key_event(&mut app, KeyCode::Char('c'), "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        match app.mode {
            AppMode::EditingConfig { .. } => {}
            other => panic!("expected config mode, got {:?}", other),
        }
    }

    #[test]
    fn g_enters_scan_mode() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);

        let action =
            handle_key_event(&mut app, KeyCode::Char('g'), "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        match app.mode {
            AppMode::Scanning(_) => {}
            other => panic!("expected scan mode, got {:?}", other),
        }
    }

    #[test]
    fn scan_mode_enter_runs_scan() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Scanning("focus".to_string());

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::RunScan("focus".to_string()));
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn scan_mode_escape_cancels() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::Scanning("focus".to_string());

        let action = handle_key_event(&mut app, KeyCode::Esc, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn scan_palette_command_enters_scan_mode() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::CommandPalette {
            query: "scan".to_string(),
            selected: 0,
        };

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        match app.mode {
            AppMode::Scanning(_) => {}
            other => panic!("expected scan mode, got {:?}", other),
        }
    }

    #[test]
    fn scan_rejected_when_runner_active() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.runner_active = true;
        app.mode = AppMode::Scanning("focus".to_string());

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(app.status_message.as_deref(), Some("Runner already active"));
    }

    #[test]
    fn config_mode_cycles_project_type() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        app.mode = AppMode::EditingConfig {
            selected: 0,
            editing_value: None,
        };

        let action =
            handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(action, TuiAction::Continue);
        assert_eq!(
            app.project_config.project_type,
            Some(crate::contracts::ProjectType::Code)
        );
        assert!(app.dirty_config);
    }

    #[test]
    fn config_text_entry_updates_value() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001")],
        };
        let mut app = App::new(queue);
        let idx = app
            .config_entries()
            .iter()
            .position(|entry| entry.key == crate::tui::ConfigKey::QueueIdPrefix)
            .expect("queue.id_prefix entry");
        app.mode = AppMode::EditingConfig {
            selected: idx,
            editing_value: None,
        };

        let _ =
            handle_key_event(&mut app, KeyCode::Char('X'), "2026-01-20T00:00:00Z").expect("key");
        let _ = handle_key_event(&mut app, KeyCode::Enter, "2026-01-20T00:00:00Z").expect("key");

        assert_eq!(app.project_config.queue.id_prefix.as_deref(), Some("X"));
    }
}
