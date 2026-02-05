//! Advanced task builder key handling for the TUI.
//!
//! Responsibilities:
//! - Handle the two-step task builder flow: description -> advanced options.
//! - Capture overrides for runner, model, effort, tags, scope, and repo-prompt mode.
//! - Validate inputs and convert to TaskBuilderOptions on submission.
//!
//! Not handled here:
//! - Rendering the task builder UI (see `tui::render::overlays`).
//! - Actual task creation (handled by the caller via TuiAction).
//!
//! Invariants/assumptions:
//! - Uses TextInput for text fields following the same patterns as other modes.
//! - Enum fields cycle through variants with Space/Enter keys.
//! - Validation happens on submit, not during editing.

use super::super::input::{TextInputEdit, apply_text_input_key};
use super::super::{App, AppMode, TextInput};
use super::types::{TaskBuilderOptions, TaskBuilderState, TaskBuilderStep, TuiAction};
use super::{is_plain_char, text_char};
use crate::agent::RepoPromptMode;
use crate::constants::ui::TASK_BUILDER_FIELD_COUNT;
use crate::contracts::{Model, ReasoningEffort, Runner};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

/// Field indices for the advanced options step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskBuilderField {
    Tags = 0,
    Scope = 1,
    Runner = 2,
    Model = 3,
    Effort = 4,
    RepoPrompt = 5,
    Submit = 6,
}

impl TaskBuilderField {
    const COUNT: usize = TASK_BUILDER_FIELD_COUNT;

    fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Tags,
            1 => Self::Scope,
            2 => Self::Runner,
            3 => Self::Model,
            4 => Self::Effort,
            5 => Self::RepoPrompt,
            _ => Self::Submit,
        }
    }
}

/// Handle key events in BuildingTaskOptions mode.
pub(super) fn handle_building_task_options_key(
    app: &mut App,
    key: KeyEvent,
    state: TaskBuilderState,
) -> Result<TuiAction> {
    match state.step {
        TaskBuilderStep::Description => handle_description_step(app, key, state),
        TaskBuilderStep::Advanced => handle_advanced_step(app, key, state),
    }
}

/// Handle the description input step.
fn handle_description_step(
    app: &mut App,
    key: KeyEvent,
    mut state: TaskBuilderState,
) -> Result<TuiAction> {
    match key.code {
        KeyCode::Enter => {
            let description = state.description_input.value().trim().to_string();
            if description.is_empty() {
                state.error_message = Some("Description cannot be empty".to_string());
                app.mode = AppMode::BuildingTaskOptions(state);
                return Ok(TuiAction::Continue);
            }
            state.description = description;
            state.step = TaskBuilderStep::Advanced;
            state.selected_field = 0;
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        _ => {
            let mut input = state.description_input.clone();
            if apply_text_input_key(&mut input, &key) == TextInputEdit::Changed {
                state.description_input = input;
                state.error_message = None;
            }
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
    }
}

/// Handle the advanced options step.
fn handle_advanced_step(
    app: &mut App,
    key: KeyEvent,
    mut state: TaskBuilderState,
) -> Result<TuiAction> {
    let field = TaskBuilderField::from_index(state.selected_field);

    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            Ok(TuiAction::Continue)
        }
        KeyCode::Up => {
            state.selected_field = state.selected_field.saturating_sub(1);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('k') if is_plain_char(&key, 'k') => {
            state.selected_field = state.selected_field.saturating_sub(1);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Down => {
            state.selected_field = (state.selected_field + 1).min(TaskBuilderField::COUNT - 1);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('j') if is_plain_char(&key, 'j') => {
            state.selected_field = (state.selected_field + 1).min(TaskBuilderField::COUNT - 1);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Enter => {
            if field == TaskBuilderField::Submit {
                // Validate and submit
                match validate_and_build_options(&state) {
                    Ok(options) => {
                        app.mode = AppMode::Normal;
                        Ok(TuiAction::BuildTaskWithOptions(options))
                    }
                    Err(e) => {
                        state.error_message = Some(e.to_string());
                        app.mode = AppMode::BuildingTaskOptions(state);
                        Ok(TuiAction::Continue)
                    }
                }
            } else {
                // For text fields, Enter doesn't do anything special
                // For cycle fields, we cycle on Enter too
                cycle_field(&mut state, field);
                app.mode = AppMode::BuildingTaskOptions(state);
                Ok(TuiAction::Continue)
            }
        }
        KeyCode::Char(' ') if is_plain_char(&key, ' ') => {
            cycle_field(&mut state, field);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        KeyCode::Char('x') if is_plain_char(&key, 'x') => {
            clear_field(&mut state, field);
            state.error_message = None;
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
        _ => {
            // Handle text input for text fields
            if let Some(ch) = text_char(&key) {
                match field {
                    TaskBuilderField::Tags | TaskBuilderField::Scope | TaskBuilderField::Model => {
                        let current = get_field_value(&state, field);
                        let mut input = TextInput::new(current);
                        input.insert_char(ch);
                        set_field_value(&mut state, field, input.value());
                        state.error_message = None;
                    }
                    _ => {}
                }
            }
            app.mode = AppMode::BuildingTaskOptions(state);
            Ok(TuiAction::Continue)
        }
    }
}

/// Get the current value of a text field.
fn get_field_value(state: &TaskBuilderState, field: TaskBuilderField) -> String {
    match field {
        TaskBuilderField::Tags => state.tags_hint.clone(),
        TaskBuilderField::Scope => state.scope_hint.clone(),
        TaskBuilderField::Model => state.model_override_input.clone(),
        _ => String::new(),
    }
}

/// Set the value of a text field.
fn set_field_value(state: &mut TaskBuilderState, field: TaskBuilderField, value: &str) {
    match field {
        TaskBuilderField::Tags => state.tags_hint = value.to_string(),
        TaskBuilderField::Scope => state.scope_hint = value.to_string(),
        TaskBuilderField::Model => state.model_override_input = value.to_string(),
        _ => {}
    }
}

/// Clear a field's value.
fn clear_field(state: &mut TaskBuilderState, field: TaskBuilderField) {
    match field {
        TaskBuilderField::Tags => state.tags_hint.clear(),
        TaskBuilderField::Scope => state.scope_hint.clear(),
        TaskBuilderField::Runner => state.runner_override = None,
        TaskBuilderField::Model => state.model_override_input.clear(),
        TaskBuilderField::Effort => state.effort_override = None,
        TaskBuilderField::RepoPrompt => state.repoprompt_mode = None,
        TaskBuilderField::Submit => {}
    }
}

/// Cycle the value of a cycleable field.
fn cycle_field(state: &mut TaskBuilderState, field: TaskBuilderField) {
    match field {
        TaskBuilderField::Runner => {
            state.runner_override = cycle_runner(state.runner_override.clone());
        }
        TaskBuilderField::Effort => {
            state.effort_override = cycle_effort(state.effort_override);
        }
        TaskBuilderField::RepoPrompt => {
            state.repoprompt_mode = cycle_repoprompt(state.repoprompt_mode);
        }
        _ => {}
    }
}

/// Cycle through runner options.
fn cycle_runner(current: Option<Runner>) -> Option<Runner> {
    match current {
        None => Some(Runner::Claude),
        Some(Runner::Claude) => Some(Runner::Codex),
        Some(Runner::Codex) => Some(Runner::Opencode),
        Some(Runner::Opencode) => Some(Runner::Gemini),
        Some(Runner::Gemini) => Some(Runner::Cursor),
        Some(Runner::Cursor) => Some(Runner::Kimi),
        Some(Runner::Kimi) => Some(Runner::Pi),
        Some(Runner::Pi) => None,
        Some(Runner::Plugin(_)) => None,
    }
}

/// Cycle through reasoning effort options.
fn cycle_effort(current: Option<ReasoningEffort>) -> Option<ReasoningEffort> {
    match current {
        None => Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Low) => Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::Medium) => Some(ReasoningEffort::High),
        Some(ReasoningEffort::High) => Some(ReasoningEffort::XHigh),
        Some(ReasoningEffort::XHigh) => None,
    }
}

/// Cycle through RepoPrompt mode options.
fn cycle_repoprompt(current: Option<RepoPromptMode>) -> Option<RepoPromptMode> {
    match current {
        None => Some(RepoPromptMode::Tools),
        Some(RepoPromptMode::Tools) => Some(RepoPromptMode::Plan),
        Some(RepoPromptMode::Plan) => Some(RepoPromptMode::Off),
        Some(RepoPromptMode::Off) => None,
    }
}

/// Validate the state and build TaskBuilderOptions.
fn validate_and_build_options(state: &TaskBuilderState) -> anyhow::Result<TaskBuilderOptions> {
    // Validate model if provided
    let model_override = if state.model_override_input.trim().is_empty() {
        None
    } else {
        match parse_model(state.model_override_input.trim()) {
            Ok(model) => Some(model),
            Err(e) => anyhow::bail!("Invalid model: {}", e),
        }
    };

    Ok(TaskBuilderOptions {
        request: state.description.clone(),
        hint_tags: state.tags_hint.clone(),
        hint_scope: state.scope_hint.clone(),
        runner_override: state.runner_override.clone(),
        model_override,
        reasoning_effort_override: state.effort_override,
        repoprompt_mode: state.repoprompt_mode,
    })
}

/// Parse a model string into a Model.
fn parse_model(value: &str) -> anyhow::Result<Model> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("model cannot be empty");
    }
    Ok(match trimmed {
        "gpt-5.2-codex" => Model::Gpt52Codex,
        "gpt-5.2" => Model::Gpt52,
        "zai-coding-plan/glm-4.7" => Model::Glm47,
        other => Model::Custom(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_runner() {
        assert_eq!(cycle_runner(None), Some(Runner::Claude));
        assert_eq!(cycle_runner(Some(Runner::Claude)), Some(Runner::Codex));
        assert_eq!(cycle_runner(Some(Runner::Codex)), Some(Runner::Opencode));
        assert_eq!(cycle_runner(Some(Runner::Opencode)), Some(Runner::Gemini));
        assert_eq!(cycle_runner(Some(Runner::Gemini)), Some(Runner::Cursor));
        assert_eq!(cycle_runner(Some(Runner::Cursor)), Some(Runner::Kimi));
        assert_eq!(cycle_runner(Some(Runner::Kimi)), Some(Runner::Pi));
        assert_eq!(cycle_runner(Some(Runner::Pi)), None);
    }

    #[test]
    fn test_cycle_effort() {
        assert_eq!(cycle_effort(None), Some(ReasoningEffort::Low));
        assert_eq!(
            cycle_effort(Some(ReasoningEffort::Low)),
            Some(ReasoningEffort::Medium)
        );
        assert_eq!(
            cycle_effort(Some(ReasoningEffort::Medium)),
            Some(ReasoningEffort::High)
        );
        assert_eq!(
            cycle_effort(Some(ReasoningEffort::High)),
            Some(ReasoningEffort::XHigh)
        );
        assert_eq!(cycle_effort(Some(ReasoningEffort::XHigh)), None);
    }

    #[test]
    fn test_cycle_repoprompt() {
        assert_eq!(cycle_repoprompt(None), Some(RepoPromptMode::Tools));
        assert_eq!(
            cycle_repoprompt(Some(RepoPromptMode::Tools)),
            Some(RepoPromptMode::Plan)
        );
        assert_eq!(
            cycle_repoprompt(Some(RepoPromptMode::Plan)),
            Some(RepoPromptMode::Off)
        );
        assert_eq!(cycle_repoprompt(Some(RepoPromptMode::Off)), None);
    }

    #[test]
    fn test_parse_model() {
        assert!(matches!(
            parse_model("gpt-5.2-codex").unwrap(),
            Model::Gpt52Codex
        ));
        assert!(matches!(parse_model("gpt-5.2").unwrap(), Model::Gpt52));
        assert!(matches!(
            parse_model("zai-coding-plan/glm-4.7").unwrap(),
            Model::Glm47
        ));
        assert!(matches!(parse_model("sonnet").unwrap(), Model::Custom(s) if s == "sonnet"));
    }

    #[test]
    fn test_validate_and_build_options() {
        let state = TaskBuilderState {
            step: TaskBuilderStep::Advanced,
            description: "Test task".to_string(),
            description_input: TextInput::new("Test task".to_string()),
            tags_hint: "tag1, tag2".to_string(),
            scope_hint: "scope1".to_string(),
            runner_override: Some(Runner::Claude),
            model_override_input: "sonnet".to_string(),
            effort_override: Some(ReasoningEffort::High),
            repoprompt_mode: Some(RepoPromptMode::Tools),
            selected_field: 0,
            error_message: None,
        };

        let options = validate_and_build_options(&state).unwrap();
        assert_eq!(options.request, "Test task");
        assert_eq!(options.hint_tags, "tag1, tag2");
        assert_eq!(options.hint_scope, "scope1");
        assert_eq!(options.runner_override, Some(Runner::Claude));
        assert!(matches!(options.model_override, Some(Model::Custom(s)) if s == "sonnet"));
        assert_eq!(
            options.reasoning_effort_override,
            Some(ReasoningEffort::High)
        );
        assert_eq!(options.repoprompt_mode, Some(RepoPromptMode::Tools));
    }

    #[test]
    fn test_validate_and_build_options_empty_model() {
        let state = TaskBuilderState {
            step: TaskBuilderStep::Advanced,
            description: "Test task".to_string(),
            description_input: TextInput::new("Test task".to_string()),
            tags_hint: String::new(),
            scope_hint: String::new(),
            runner_override: None,
            model_override_input: String::new(),
            effort_override: None,
            repoprompt_mode: None,
            selected_field: 0,
            error_message: None,
        };

        let options = validate_and_build_options(&state).unwrap();
        assert_eq!(options.request, "Test task");
        assert!(options.model_override.is_none());
        assert!(options.runner_override.is_none());
    }
}
