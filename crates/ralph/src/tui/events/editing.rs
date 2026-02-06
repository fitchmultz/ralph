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
//! - Text input uses cursor-aware `TextInput` edits.
//! - Editing modes remain consistent with the selected entry index.

use super::super::config_edit::{ConfigKey, RiskLevel};
use super::super::{AppMode, MultiLineInput};
use super::types::TuiAction;
use super::{App, is_plain_char, text_char};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Check if cycling the config value would enable a risky behavior.
/// Returns Some(warning_message) if confirmation is required, None otherwise.
fn check_risky_config_change(app: &App, key: ConfigKey) -> Option<String> {
    let entries = app.config_entries();
    let entry = entries.iter().find(|e| e.key == key)?;

    // Only check Danger level - Warning level just shows inline text
    if entry.risk_level != RiskLevel::Danger {
        return None;
    }

    match key {
        ConfigKey::AgentGitCommitPushEnabled => {
            // Check if we're about to enable auto-push (currently false or None)
            let current = app.project_config.agent.git_commit_push_enabled;
            if current != Some(true) {
                Some(format!(
                    "Enable automatic git commit and push?\n\n⚠ WARNING: {}",
                    entry.description
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Result of handling a text-edit key.
enum TextEditKeyResult {
    Commit(MultiLineInput),
    Cancel,
    Update(MultiLineInput),
    Noop,
}

fn handle_textarea_edit_key(key: KeyEvent, mut textarea: MultiLineInput) -> TextEditKeyResult {
    match key.code {
        KeyCode::Enter => {
            // Check if Alt is pressed - if so, insert newline
            if key.modifiers.contains(KeyModifiers::ALT) {
                if textarea.input(key) {
                    TextEditKeyResult::Update(textarea)
                } else {
                    TextEditKeyResult::Noop
                }
            } else {
                // Plain Enter commits
                TextEditKeyResult::Commit(textarea)
            }
        }
        KeyCode::Esc => TextEditKeyResult::Cancel,
        _ => {
            if textarea.input(key) {
                TextEditKeyResult::Update(textarea)
            } else {
                TextEditKeyResult::Noop
            }
        }
    }
}

/// Handle key events in EditingTask mode.
pub(super) fn handle_editing_task_key(
    app: &mut App,
    key: KeyEvent,
    selected: usize,
    editing_value: Option<MultiLineInput>,
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
        match handle_textarea_edit_key(key, value) {
            TextEditKeyResult::Commit(value) => {
                // For list fields, get the lines as a comma-separated string
                let edit_value = if entry.key.is_list_field() {
                    value.lines().join(", ")
                } else {
                    value.value()
                };
                match app.apply_task_edit(entry.key, &edit_value, now_rfc3339) {
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
                        let is_list = entry.key.is_list_field();
                        app.mode = AppMode::EditingTask {
                            selected,
                            editing_value: Some(MultiLineInput::new(current, is_list)),
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
                        let is_list = entry.key.is_list_field();
                        app.mode = AppMode::EditingTask {
                            selected,
                            editing_value: Some(MultiLineInput::new(current, is_list)),
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
                            let current = app.task_value_for_edit(entry.key);
                            let is_list = entry.key.is_list_field();
                            let mut input = MultiLineInput::new(current, is_list);
                            // Simulate typing the character
                            input.textarea_mut().insert_char(ch);
                            app.mode = AppMode::EditingTask {
                                selected,
                                editing_value: Some(input),
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
    editing_value: Option<MultiLineInput>,
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
        match handle_textarea_edit_key(key, value) {
            TextEditKeyResult::Commit(value) => {
                match app.apply_config_text_value(entry.key, &value.value()) {
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
                        editing_value: Some(MultiLineInput::new(current, false)),
                    };
                } else {
                    // Check if this is a risky config change
                    if let Some(warning) = check_risky_config_change(app, entry.key) {
                        app.mode = AppMode::ConfirmRiskyConfig {
                            key: entry.key,
                            new_value: entry.value.clone(),
                            warning,
                            previous_mode: Box::new(AppMode::EditingConfig {
                                selected,
                                editing_value: None,
                            }),
                        };
                    } else {
                        app.cycle_config_value(entry.key);
                        app.set_status_message("Config updated");
                        app.mode = AppMode::EditingConfig {
                            selected,
                            editing_value: None,
                        };
                    }
                }
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(' ') if is_plain_char(&key, ' ') => {
                if entry.kind == crate::tui::ConfigFieldKind::Text {
                    let current = app.config_value_for_edit(entry.key);
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: Some(MultiLineInput::new(current, false)),
                    };
                } else {
                    // Check if this is a risky config change
                    if let Some(warning) = check_risky_config_change(app, entry.key) {
                        app.mode = AppMode::ConfirmRiskyConfig {
                            key: entry.key,
                            new_value: entry.value.clone(),
                            warning,
                            previous_mode: Box::new(AppMode::EditingConfig {
                                selected,
                                editing_value: None,
                            }),
                        };
                    } else {
                        app.cycle_config_value(entry.key);
                        app.set_status_message("Config updated");
                        app.mode = AppMode::EditingConfig {
                            selected,
                            editing_value: None,
                        };
                    }
                }
                Ok(TuiAction::Continue)
            }
            KeyCode::Char('x') if is_plain_char(&key, 'x') => {
                app.clear_config_value(entry.key);
                app.set_status_message("Config cleared");
                Ok(TuiAction::Continue)
            }
            KeyCode::Char(_) => {
                if entry.kind == crate::tui::ConfigFieldKind::Text
                    && let Some(ch) = text_char(&key)
                {
                    let current = app.config_value_for_edit(entry.key);
                    let mut input = MultiLineInput::new(current, false);
                    // Simulate typing the character
                    input.textarea_mut().insert_char(ch);
                    app.mode = AppMode::EditingConfig {
                        selected,
                        editing_value: Some(input),
                    };
                }
                Ok(TuiAction::Continue)
            }
            _ => Ok(TuiAction::Continue),
        }
    }
}
