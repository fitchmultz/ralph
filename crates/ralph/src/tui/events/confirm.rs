//! Confirmation-mode key handling for the TUI.
//!
//! Responsibilities:
//! - Handle key input for destructive/confirm dialogs.
//! - Translate confirm/cancel actions into `TuiAction` values.
//!
//! Not handled here:
//! - Rendering of confirmation dialogs.
//! - Non-confirmation input handling.
//!
//! Invariants/assumptions:
//! - Confirmation modes always return to `AppMode::Normal` on cancel.
//! - Confirm actions are idempotent at the `TuiAction` level.

use super::super::AppMode;
use super::types::{ConfirmDiscardAction, TuiAction};
use super::{is_plain_char, text_char, App};
use crate::runutil::RevertDecision;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::mpsc;

/// Handle key events in ConfirmDelete mode.
pub(super) fn handle_confirm_delete_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        KeyCode::Char('y') if is_plain_char(&key, 'y') => {
            if let Err(e) = app.delete_selected_task() {
                app.set_status_message(format!("Error: {}", e));
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('Y') if is_plain_char(&key, 'Y') => {
            if let Err(e) = app.delete_selected_task() {
                app.set_status_message(format!("Error: {}", e));
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') if is_plain_char(&key, 'n') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('N') if is_plain_char(&key, 'N') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmArchive mode.
pub(super) fn handle_confirm_archive_key(
    app: &mut App,
    key: KeyEvent,
    now_rfc3339: &str,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Char('y') if is_plain_char(&key, 'y') => {
            if let Err(e) = app.archive_terminal_tasks(now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('Y') if is_plain_char(&key, 'Y') => {
            if let Err(e) = app.archive_terminal_tasks(now_rfc3339) {
                app.set_status_message(format!("Error: {}", e));
            }
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('n') if is_plain_char(&key, 'n') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('N') if is_plain_char(&key, 'N') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmQuit mode.
pub(super) fn handle_confirm_quit_key(app: &mut App, key: KeyEvent) -> Result<TuiAction> {
    match key.code {
        KeyCode::Char('y') if is_plain_char(&key, 'y') => Ok(TuiAction::Quit),
        KeyCode::Char('Y') if is_plain_char(&key, 'Y') => Ok(TuiAction::Quit),
        KeyCode::Char('n') if is_plain_char(&key, 'n') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('N') if is_plain_char(&key, 'N') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Handle key events in ConfirmDiscard mode.
pub(super) fn handle_confirm_discard_key(
    app: &mut App,
    key: KeyEvent,
    action: ConfirmDiscardAction,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Char('y') if is_plain_char(&key, 'y') => {
            app.mode = AppMode::Normal;
            Ok(match action {
                ConfirmDiscardAction::ReloadQueue => TuiAction::ReloadQueue,
                ConfirmDiscardAction::Quit => TuiAction::Quit,
            })
        }
        KeyCode::Char('Y') if is_plain_char(&key, 'Y') => {
            app.mode = AppMode::Normal;
            Ok(match action {
                ConfirmDiscardAction::ReloadQueue => TuiAction::ReloadQueue,
                ConfirmDiscardAction::Quit => TuiAction::Quit,
            })
        }
        KeyCode::Char('n') if is_plain_char(&key, 'n') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('N') if is_plain_char(&key, 'N') => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => Ok(TuiAction::Continue),
    }
}

/// Private struct encapsulating ConfirmRevert mode state.
pub(super) struct ConfirmRevertState {
    label: String,
    allow_proceed: bool,
    selected: usize,
    input: String,
    reply_sender: mpsc::Sender<RevertDecision>,
    previous_mode: AppMode,
}

impl ConfirmRevertState {
    pub(super) fn new(
        label: String,
        allow_proceed: bool,
        selected: usize,
        input: String,
        reply_sender: mpsc::Sender<RevertDecision>,
        previous_mode: AppMode,
    ) -> Self {
        Self {
            label,
            allow_proceed,
            selected,
            input,
            reply_sender,
            previous_mode,
        }
    }

    fn max_index(&self) -> usize {
        if self.allow_proceed {
            3
        } else {
            2
        }
    }

    fn into_mode(self) -> AppMode {
        AppMode::ConfirmRevert {
            label: self.label,
            allow_proceed: self.allow_proceed,
            selected: self.selected,
            input: self.input,
            reply_sender: self.reply_sender,
            previous_mode: Box::new(self.previous_mode),
        }
    }
}

fn status_message_for_revert_decision(label: &str, decision: &RevertDecision) -> String {
    match decision {
        RevertDecision::Revert => format!("{label}: reverting uncommitted changes"),
        RevertDecision::Keep => format!("{label}: keeping uncommitted changes"),
        RevertDecision::Continue { .. } => format!("{label}: continuing session"),
        RevertDecision::Proceed => format!("{label}: keeping changes and proceeding"),
    }
}

/// Handle key events in ConfirmRevert mode.
pub(super) fn handle_confirm_revert_key(
    app: &mut App,
    key: KeyEvent,
    state: ConfirmRevertState,
) -> Result<TuiAction> {
    let mut state = state;

    match key.code {
        KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Down => {
            state.selected = (state.selected + 1).min(state.max_index());
        }
        KeyCode::Char(_) => {
            if let Some(ch) = text_char(&key) {
                match ch {
                    '1' => {
                        if state.selected == 2 {
                            state.input.push('1');
                        } else {
                            state.selected = 0;
                        }
                    }
                    '2' => {
                        if state.selected == 2 {
                            state.input.push('2');
                        } else {
                            state.selected = 1;
                        }
                    }
                    '3' => {
                        if state.selected == 2 {
                            state.input.push('3');
                        } else {
                            state.selected = 2;
                        }
                    }
                    '4' => {
                        if state.selected == 2 {
                            state.input.push('4');
                        } else if state.allow_proceed {
                            state.selected = 3;
                        }
                    }
                    _ => {
                        if state.selected == 2 {
                            state.input.push(ch);
                        }
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if state.selected == 2 {
                state.input.pop();
            }
        }
        KeyCode::Enter => {
            let decision = match state.selected {
                0 => RevertDecision::Keep,
                1 => RevertDecision::Revert,
                2 => {
                    if state.input.trim().is_empty() {
                        let hint = if state.allow_proceed {
                            "enter a message to continue or choose Keep/Revert/Proceed"
                        } else {
                            "enter a message to continue or choose Keep/Revert"
                        };
                        app.set_status_message(format!("{}: {}", state.label, hint));
                        app.mode = state.into_mode();
                        return Ok(TuiAction::Continue);
                    }
                    RevertDecision::Continue {
                        message: state.input,
                    }
                }
                3 if state.allow_proceed => RevertDecision::Proceed,
                _ => RevertDecision::Keep,
            };

            if state.reply_sender.send(decision.clone()).is_err() {
                app.set_status_message(format!("{}: revert prompt expired", state.label));
            } else {
                app.set_status_message(status_message_for_revert_decision(&state.label, &decision));
            }

            app.mode = state.previous_mode;
            return Ok(TuiAction::Continue);
        }
        KeyCode::Esc => {
            let decision = RevertDecision::Keep;
            if state.reply_sender.send(decision).is_err() {
                app.set_status_message(format!("{}: revert prompt expired", state.label));
            } else {
                app.set_status_message(format!("{}: keeping uncommitted changes", state.label));
            }
            app.mode = state.previous_mode;
            return Ok(TuiAction::Continue);
        }
        _ => {}
    }

    app.mode = state.into_mode();
    Ok(TuiAction::Continue)
}
