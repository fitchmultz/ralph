//! Field editing key handling for the TUI.
//!
//! Responsibilities:
//! - Handle navigation and text edits for task and config editing modes.
//! - Apply edits or cancel based on user input.
//!
//! Not handled here:
//! - Rendering edit UIs or validating field schemas.
//! - Persistence of edits beyond updating `App` state.
//!
//! Invariants/assumptions:
//! - Text input ignores Ctrl/Alt modified characters.
//! - Editing modes remain consistent with the selected entry index.

use super::super::AppMode;
use super::types::TuiAction;
use super::{is_plain_char, text_char, App};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Result of handling a text-edit key.
enum TextEditKeyResult {
    Commit(String),
    Cancel,
    Update(String),
    Noop,
}

fn handle_text_edit_key(key: KeyEvent, value: String) -> TextEditKeyResult {
    match key.code {
        KeyCode::Enter => TextEditKeyResult::Commit(value),
        KeyCode::Esc => TextEditKeyResult::Cancel,
        KeyCode::Backspace => {
            let mut updated = value;
            updated.pop();
            TextEditKeyResult::Update(updated)
        }
        _ => match text_char(&key) {
            Some(ch) => {
                let mut updated = value;
                updated.push(ch);
                TextEditKeyResult::Update(updated)
            }
            None => TextEditKeyResult::Noop,
        },
    }
}

/// Handle key events in EditingTask mode.
pub(super) fn handle_editing_task_key(
    app: &mut App,
    key: KeyEvent,
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

    if let Some(value) = editing_value {
        match handle_text_edit_key(key, value) {
            TextEditKeyResult::Commit(value) => {
                match app.apply_task_edit(entry.key, &value, now_rfc3339) {
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
                }
            }
            TextEditKeyResult::Cancel => {
                app.mode = AppMode::EditingTask {
                    selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            TextEditKeyResult::Update(value) => {
                app.mode = AppMode::EditingTask {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            TextEditKeyResult::Noop => Ok(TuiAction::Continue),
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
                Ok(TuiAction::Continue)
            }
            KeyCode::Up => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('k') if is_plain_char(&key, 'k') => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Down => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('j') if is_plain_char(&key, 'j') => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingTask {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Enter => {
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
            KeyCode::Char(' ') if is_plain_char(&key, ' ') => {
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
            KeyCode::Char('x') if is_plain_char(&key, 'x') => {
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
            KeyCode::Char(_) => {
                match entry.kind {
                    crate::tui::TaskEditKind::Text
                    | crate::tui::TaskEditKind::List
                    | crate::tui::TaskEditKind::Map
                    | crate::tui::TaskEditKind::OptionalText => {
                        if let Some(ch) = text_char(&key) {
                            let mut current = app.task_value_for_edit(entry.key);
                            current.push(ch);
                            app.mode = AppMode::EditingTask {
                                selected,
                                editing_value: Some(current),
                            };
                        }
                    }
                    crate::tui::TaskEditKind::Cycle => {}
                }
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    }
}

pub(super) fn handle_editing_config_key(
    app: &mut App,
    key: KeyEvent,
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

    if let Some(value) = editing_value {
        match handle_text_edit_key(key, value) {
            TextEditKeyResult::Commit(value) => {
                match app.apply_config_text_value(entry.key, &value) {
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
                }
            }
            TextEditKeyResult::Cancel => {
                app.mode = AppMode::EditingConfig {
                    selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            TextEditKeyResult::Update(value) => {
                app.mode = AppMode::EditingConfig {
                    selected,
                    editing_value: Some(value),
                };
                Ok(TuiAction::Continue)
            }
            TextEditKeyResult::Noop => Ok(TuiAction::Continue),
        }
    } else {
        match key.code {
            KeyCode::Esc => {
                app.mode = AppMode::Normal;
                Ok(TuiAction::Continue)
            }
            KeyCode::Up => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('k') if is_plain_char(&key, 'k') => {
                let next_selected = selected.saturating_sub(1);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Down => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('j') if is_plain_char(&key, 'j') => {
                let next_selected = (selected + 1).min(max_index);
                app.mode = AppMode::EditingConfig {
                    selected: next_selected,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            KeyCode::Enter => {
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
            KeyCode::Char(' ') if is_plain_char(&key, ' ') => {
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
            KeyCode::Char('x') if is_plain_char(&key, 'x') => {
                app.clear_config_value(entry.key);
                app.set_status_message("Config cleared");
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(_) => {
                if entry.kind == crate::tui::ConfigFieldKind::Text {
                    if let Some(ch) = text_char(&key) {
                        let mut current = app.config_value_for_edit(entry.key);
                        current.push(ch);
                        app.mode = AppMode::EditingConfig {
                            selected,
                            editing_value: Some(current),
                        };
                    }
                }
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    }
}
