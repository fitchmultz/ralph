//! TUI event types and interaction modes.
//!
//! Responsibilities:
//! - Define `TuiAction` values returned by key handling.
//! - Define `AppMode` and related enums that model TUI state.
//!
//! Not handled here:
//! - Event dispatch logic (see `tui/events/mod.rs`).
//! - Rendering or side effects.
//!
//! Invariants/assumptions:
//! - `AppMode` variants fully describe UI state used by handlers and renderers.
//! - Public enums remain stable for callers constructing or matching on modes.

use std::sync::mpsc;

use crate::agent::RepoPromptMode;
use crate::contracts::{ReasoningEffort, Runner};
use crate::runutil::RevertDecision;
use crate::tui::config_edit::ConfigKey;
use crate::tui::{MultiLineInput, TextInput};

/// View mode for the TUI - list or kanban board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Traditional list view with task details panel
    #[default]
    List,
    /// Kanban board view with status columns
    Board,
}

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
    /// Trigger task builder agent with the given description
    BuildTask(String),
    /// Trigger task builder agent with full options
    BuildTaskWithOptions(TaskBuilderOptions),
    /// Spawn the user's editor for these (repo-relative or absolute) paths.
    OpenScopeInEditor(Vec<String>),
    /// Copy the provided text to the system clipboard.
    CopyToClipboard(String),
    /// Open the provided URL in the system browser.
    OpenUrlInBrowser(String),
}

/// Actions that can discard unsaved changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmDiscardAction {
    /// Reload queues from disk.
    ReloadQueue,
    /// Quit the TUI.
    Quit,
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
        editing_value: Option<MultiLineInput>,
    },
    /// Creating a new task (title input)
    CreatingTask(TextInput),
    /// Creating a new task via task builder agent (description input)
    CreatingTaskDescription(TextInput),
    /// Searching tasks (query input)
    Searching(TextInput),
    /// Filtering tasks by tag list (comma-separated input)
    FilteringTags(TextInput),
    /// Filtering tasks by scope list (comma-separated input)
    FilteringScopes(TextInput),
    /// Editing project configuration
    EditingConfig {
        selected: usize,
        editing_value: Option<MultiLineInput>,
    },
    /// Running a scan (focus input)
    Scanning(TextInput),
    /// Command palette (":" style)
    CommandPalette { query: TextInput, selected: usize },
    /// Confirming task deletion
    ConfirmDelete,
    /// Confirming archive of done/rejected tasks
    ConfirmArchive,
    /// Confirming queue repair (with dry-run flag)
    ConfirmRepair { dry_run: bool },
    /// Confirming queue unlock
    ConfirmUnlock,
    /// Confirming auto-archive of a single terminal task
    ConfirmAutoArchive(String),
    /// Confirm batch delete with count of affected tasks
    ConfirmBatchDelete { count: usize },
    /// Confirm batch archive with count of affected tasks
    ConfirmBatchArchive { count: usize },
    /// Confirming quit while a task is running
    ConfirmQuit,
    /// Confirming discard of unsaved changes before reload/quit.
    ConfirmDiscard { action: ConfirmDiscardAction },
    /// Confirming revert of uncommitted changes.
    ConfirmRevert {
        label: String,
        preface: Option<String>,
        allow_proceed: bool,
        selected: usize,
        input: TextInput,
        reply_sender: mpsc::Sender<RevertDecision>,
        previous_mode: Box<AppMode>,
    },
    /// Executing a task (live output view)
    Executing { task_id: String },
    /// Confirming a risky config toggle
    ConfirmRiskyConfig {
        key: ConfigKey,
        new_value: String,
        warning: String,
        previous_mode: Box<AppMode>,
    },
    /// Building a task with agent overrides (advanced task builder)
    BuildingTaskOptions(TaskBuilderState),
    /// Jumping to a task by ID (input mode)
    JumpingToTask(TextInput),
    /// Workflow flowchart overlay
    FlowchartOverlay {
        /// Previous mode to return to when closing
        previous_mode: Box<AppMode>,
    },
    /// Dependency graph overlay
    DependencyGraphOverlay {
        /// Previous mode to return to when closing
        previous_mode: Box<AppMode>,
        /// View mode: true = show what this task blocks (dependents), false = show dependencies
        show_dependents: bool,
        /// Whether to highlight critical path
        highlight_critical: bool,
    },
    /// Parallel run state overlay (read-only)
    ParallelStateOverlay {
        /// Previous mode to return to when closing
        previous_mode: Box<AppMode>,
    },
}

/// State for the advanced task builder flow with override options.
#[derive(Debug, Clone)]
pub struct TaskBuilderState {
    /// Current step in the builder flow
    pub step: TaskBuilderStep,
    /// The task description/request
    pub description: String,
    /// Text input for description (used in Description step)
    pub description_input: TextInput,
    /// Tags hint (comma-separated)
    pub tags_hint: String,
    /// Scope hint (comma-separated)
    pub scope_hint: String,
    /// Runner override (None = use config default)
    pub runner_override: Option<Runner>,
    /// Model override as raw input (validated on submit)
    pub model_override_input: String,
    /// Reasoning effort override
    pub effort_override: Option<ReasoningEffort>,
    /// RepoPrompt mode override
    pub repoprompt_mode: Option<RepoPromptMode>,
    /// Currently selected field index (for Advanced step)
    pub selected_field: usize,
    /// Error message for validation failures
    pub error_message: Option<String>,
}

/// Steps in the task builder flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskBuilderStep {
    /// Entering the task description
    Description,
    /// Configuring advanced options
    Advanced,
}

/// Options collected by the task builder for creating a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBuilderOptions {
    /// The task description/request
    pub request: String,
    /// Tags hint (comma-separated)
    pub hint_tags: String,
    /// Scope hint (comma-separated)
    pub hint_scope: String,
    /// Runner override (None = use config default)
    pub runner_override: Option<Runner>,
    /// Model override
    pub model_override: Option<crate::contracts::Model>,
    /// Reasoning effort override
    pub reasoning_effort_override: Option<ReasoningEffort>,
    /// RepoPrompt mode override
    pub repoprompt_mode: Option<RepoPromptMode>,
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
            (CreatingTaskDescription(left), CreatingTaskDescription(right)) => left == right,
            (Searching(left), Searching(right)) => left == right,
            (FilteringTags(left), FilteringTags(right)) => left == right,
            (FilteringScopes(left), FilteringScopes(right)) => left == right,
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
            (ConfirmRepair { dry_run: left }, ConfirmRepair { dry_run: right }) => left == right,
            (ConfirmUnlock, ConfirmUnlock) => true,
            (ConfirmAutoArchive(left), ConfirmAutoArchive(right)) => left == right,
            (ConfirmBatchDelete { count: left }, ConfirmBatchDelete { count: right }) => {
                left == right
            }
            (ConfirmBatchArchive { count: left }, ConfirmBatchArchive { count: right }) => {
                left == right
            }
            (ConfirmQuit, ConfirmQuit) => true,
            (ConfirmDiscard { action: left }, ConfirmDiscard { action: right }) => left == right,
            (
                ConfirmRevert {
                    label: left_label,
                    preface: left_preface,
                    allow_proceed: left_allow_proceed,
                    selected: left_selected,
                    input: left_input,
                    previous_mode: left_previous,
                    ..
                },
                ConfirmRevert {
                    label: right_label,
                    preface: right_preface,
                    allow_proceed: right_allow_proceed,
                    selected: right_selected,
                    input: right_input,
                    previous_mode: right_previous,
                    ..
                },
            ) => {
                left_label == right_label
                    && left_preface == right_preface
                    && left_allow_proceed == right_allow_proceed
                    && left_selected == right_selected
                    && left_input == right_input
                    && left_previous == right_previous
            }
            (Executing { task_id: left_id }, Executing { task_id: right_id }) => {
                left_id == right_id
            }
            (
                ConfirmRiskyConfig {
                    key: left_key,
                    new_value: left_new_value,
                    warning: left_warning,
                    previous_mode: left_previous,
                },
                ConfirmRiskyConfig {
                    key: right_key,
                    new_value: right_new_value,
                    warning: right_warning,
                    previous_mode: right_previous,
                },
            ) => {
                left_key == right_key
                    && left_new_value == right_new_value
                    && left_warning == right_warning
                    && left_previous == right_previous
            }
            (BuildingTaskOptions(left), BuildingTaskOptions(right)) => {
                left.step == right.step
                    && left.description == right.description
                    && left.tags_hint == right.tags_hint
                    && left.scope_hint == right.scope_hint
                    && left.runner_override == right.runner_override
                    && left.model_override_input == right.model_override_input
                    && left.effort_override == right.effort_override
                    && left.repoprompt_mode == right.repoprompt_mode
                    && left.selected_field == right.selected_field
                    && left.error_message == right.error_message
            }
            (JumpingToTask(left), JumpingToTask(right)) => left == right,
            (
                FlowchartOverlay {
                    previous_mode: left_previous,
                },
                FlowchartOverlay {
                    previous_mode: right_previous,
                },
            ) => left_previous == right_previous,
            (
                DependencyGraphOverlay {
                    previous_mode: left_previous,
                    show_dependents: left_show_deps,
                    highlight_critical: left_critical,
                },
                DependencyGraphOverlay {
                    previous_mode: right_previous,
                    show_dependents: right_show_deps,
                    highlight_critical: right_critical,
                },
            ) => {
                left_previous == right_previous
                    && left_show_deps == right_show_deps
                    && left_critical == right_critical
            }
            (
                ParallelStateOverlay {
                    previous_mode: left_previous,
                },
                ParallelStateOverlay {
                    previous_mode: right_previous,
                },
            ) => left_previous == right_previous,
            _ => false,
        }
    }
}

impl Eq for AppMode {}
