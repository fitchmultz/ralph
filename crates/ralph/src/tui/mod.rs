//! Interactive Terminal UI for browsing and managing the task queue.
//!
//! User-centric entry point features:
//! - Queue browsing, filtering, search
//! - Task editing (full task fields) + create/delete
//! - Execution (run selected) with live logs
//! - Loop mode: auto-run next runnable tasks (`l` toggles)
//! - Archive mode: move Done/Rejected tasks to done archive (`a` then confirm)
//! - Command palette for discoverability (`:`)
//!
//! Key bindings (high-level):
//! - `:`: command palette (filter by tags/scopes, toggle regex/case-sensitive)
//! - `q` / `Esc`: Quit (prompts if a task is still running)
//! - `?` / `h`: Help overlay
//! - `Up` / `Down` / `j` / `k`: Navigate task list
//! - `Enter`: Execute selected task
//! - `l`: Toggle loop (auto-run next runnable tasks)
//! - `a`: Archive done/rejected tasks (confirmation)
//! - `d`: Delete task (with confirmation)
//! - `e`: Edit task fields
//! - `c`: Edit project config (.ralph/config.json)
//! - `s`: Cycle status (Draft → Todo → Doing → Done → Rejected → Draft)
//! - `f`: Cycle status filter (All → Todo → Doing → Done → Draft → Rejected → All)
//! - `p`: Cycle priority (Low → Medium → High → Critical → Low)
//! - `r`: Reload queue from disk
//! - `n`: Create a new task
//! - `/`: Search tasks by text
//! - `t`: Filter tasks by tags
//! - `x`: Clear active filters
//! - `g`: Scan repository for new tasks
//! - Executing view: `↑`/`↓`/`j`/`k` scroll, `PgUp`/`PgDn` page, `a` toggles auto-scroll, `l` stops loop

use anyhow::{anyhow, bail, Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::config::{self, ConfigLayer};
use crate::contracts::{
    ClaudePermissionMode, GitRevertMode, Model, ProjectType, QueueFile, ReasoningEffort, Runner,
    Task, TaskPriority, TaskStatus,
};
use crate::outpututil::format_custom_fields;
use crate::timeutil;
use crate::{fsutil, queue, runutil};

pub mod events;
pub mod render;

pub use events::{handle_key_event, AppMode, PaletteCommand, PaletteEntry, TuiAction};
pub use render::draw_ui;

/// Options that control how the TUI boots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TuiOptions {
    /// If true, start loop mode immediately after launch.
    pub start_loop: bool,
    /// Optional max tasks for loop mode (None = unlimited).
    pub loop_max_tasks: Option<u32>,
    /// If true, draft tasks are eligible for loop selection.
    pub loop_include_draft: bool,
}

/// Active filters applied to the task list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterState {
    /// Free-text search query (substring match across task fields).
    pub query: String,
    /// Status filters (empty means all statuses).
    pub statuses: Vec<TaskStatus>,
    /// Tag filters (empty means all tags).
    pub tags: Vec<String>,
    /// Search options (regex mode, case sensitivity).
    pub search_options: queue::SearchOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFieldKind {
    Cycle,
    Toggle,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEditKind {
    Cycle,
    Text,
    List,
    Map,
    OptionalText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEditKey {
    Title,
    Status,
    Priority,
    Tags,
    Scope,
    Evidence,
    Plan,
    Notes,
    Request,
    DependsOn,
    CustomFields,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
}

#[derive(Debug, Clone)]
pub struct TaskEditEntry {
    pub key: TaskEditKey,
    pub label: &'static str,
    pub value: String,
    pub kind: TaskEditKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    ProjectType,
    QueueFile,
    QueueDoneFile,
    QueueIdPrefix,
    QueueIdWidth,
    AgentRunner,
    AgentModel,
    AgentReasoningEffort,
    AgentIterations,
    AgentFollowupReasoningEffort,
    AgentCodexBin,
    AgentOpencodeBin,
    AgentGeminiBin,
    AgentClaudeBin,
    AgentClaudePermissionMode,
    AgentRequireRepoPrompt,
    AgentGitRevertMode,
    AgentGitCommitPushEnabled,
    AgentPhases,
}

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub key: ConfigKey,
    pub label: &'static str,
    pub value: String,
    pub kind: ConfigFieldKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunningKind {
    Task,
    Scan { focus: String },
}

/// Application state for the TUI.
pub struct App {
    /// The active task queue.
    pub queue: QueueFile,
    /// The done archive queue.
    pub done: QueueFile,
    /// Currently selected task index.
    pub selected: usize,
    /// Current interaction mode.
    pub mode: AppMode,
    /// Scroll offset for the task list.
    pub scroll: usize,
    /// Width of the right panel for text wrapping.
    pub detail_width: u16,
    /// Flag indicating if active queue was modified (needs save).
    pub dirty: bool,
    /// Flag indicating if done archive was modified (needs save).
    pub dirty_done: bool,
    /// Project-specific config overrides (.ralph/config.json).
    pub project_config: ConfigLayer,
    /// Optional path to the project config.
    pub project_config_path: Option<PathBuf>,
    /// Flag indicating if project config was modified (needs save).
    pub dirty_config: bool,
    /// Last auto-save error message, if any.
    pub save_error: Option<String>,
    /// Status message shown in the footer (user-visible feedback).
    pub status_message: Option<String>,
    /// Execution logs.
    pub logs: Vec<String>,
    /// Scroll offset for execution logs.
    pub log_scroll: usize,
    /// Whether to auto-scroll execution logs.
    pub autoscroll: bool,
    /// Last known visible log lines in Executing view (for paging/auto-scroll).
    pub log_visible_lines: usize,
    /// Height of the task list (for scrolling calculation).
    pub list_height: usize,
    /// Whether a runner thread is currently executing a task.
    pub runner_active: bool,
    /// Task ID currently running, if any.
    pub running_task_id: Option<String>,
    /// Kind of runner currently executing (task vs scan).
    pub running_kind: Option<RunningKind>,
    /// Whether loop mode is active.
    pub loop_active: bool,
    /// When loop is enabled while a task is already running, do not count that finishing task.
    pub loop_arm_after_current: bool,
    /// Count of tasks successfully completed in the current loop session.
    pub loop_ran: u32,
    /// Optional cap for loop tasks.
    pub loop_max_tasks: Option<u32>,
    /// Whether draft tasks are eligible for loop selection.
    pub loop_include_draft: bool,
    /// Task ID prefix used for new task creation.
    pub id_prefix: String,
    /// Task ID width used for new task creation.
    pub id_width: usize,
    /// Optional path to the done queue (kept for future integrations/UI).
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
            done: QueueFile::default(),
            selected: 0,
            mode: AppMode::Normal,
            scroll: 0,
            detail_width: 60,
            dirty: false,
            dirty_done: false,
            project_config: ConfigLayer::default(),
            project_config_path: None,
            dirty_config: false,
            save_error: None,
            status_message: None,
            logs: Vec::new(),
            log_scroll: 0,
            autoscroll: true,
            log_visible_lines: 20,
            list_height: 20,
            runner_active: false,
            running_task_id: None,
            running_kind: None,
            loop_active: false,
            loop_arm_after_current: false,
            loop_ran: 0,
            loop_max_tasks: None,
            loop_include_draft: false,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            done_path: None,
            filters: FilterState::default(),
            filtered_indices: Vec::new(),
        };
        app.rebuild_filtered_view();
        app
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    /// Return the number of tasks in the filtered view.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Return true if any filters are active.
    pub fn has_active_filters(&self) -> bool {
        !self.filters.query.trim().is_empty()
            || !self.filters.tags.is_empty()
            || !self.filters.statuses.is_empty()
            || !self.filters.search_options.scopes.is_empty()
            || self.filters.search_options.use_regex
            || self.filters.search_options.case_sensitive
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
        for scope in &self.filters.search_options.scopes {
            parts.push(format!("scope={}", scope));
        }
        if self.filters.search_options.use_regex {
            parts.push("regex".to_string());
        }
        if self.filters.search_options.case_sensitive {
            parts.push("case-sensitive".to_string());
        }
        Some(format!("filters: {}", parts.join(" ")))
    }

    /// Toggle case-sensitive search.
    pub fn toggle_case_sensitive(&mut self) {
        self.filters.search_options.case_sensitive = !self.filters.search_options.case_sensitive;
        let state = if self.filters.search_options.case_sensitive {
            "enabled"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Case-sensitive search {}", state));
        self.rebuild_filtered_view();
    }

    /// Toggle regex search.
    pub fn toggle_regex(&mut self) {
        self.filters.search_options.use_regex = !self.filters.search_options.use_regex;
        let state = if self.filters.search_options.use_regex {
            "enabled"
        } else {
            "disabled"
        };
        self.set_status_message(format!("Regex search {}", state));
        self.rebuild_filtered_view();
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
            if self.selected >= self.scroll + list_height {
                self.scroll = self.selected - list_height + 1;
            }
        }
    }

    /// Cycle the status of the selected task.
    pub fn cycle_status(&mut self, now_rfc3339: &str) -> Result<()> {
        self.apply_task_edit(TaskEditKey::Status, "", now_rfc3339)
    }

    /// Cycle the priority of the selected task.
    pub fn cycle_priority(&mut self, now_rfc3339: &str) -> Result<()> {
        self.apply_task_edit(TaskEditKey::Priority, "", now_rfc3339)
    }

    /// Delete the selected task.
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
        self.set_status_message(format!("Deleted {}", task.id));
        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        Ok(task)
    }

    pub(crate) fn task_edit_entries(&self) -> Vec<TaskEditEntry> {
        let Some(task) = self.selected_task() else {
            return Vec::new();
        };

        vec![
            TaskEditEntry {
                key: TaskEditKey::Title,
                label: "title",
                value: task.title.clone(),
                kind: TaskEditKind::Text,
            },
            TaskEditEntry {
                key: TaskEditKey::Status,
                label: "status",
                value: task.status.as_str().to_string(),
                kind: TaskEditKind::Cycle,
            },
            TaskEditEntry {
                key: TaskEditKey::Priority,
                label: "priority",
                value: task.priority.as_str().to_string(),
                kind: TaskEditKind::Cycle,
            },
            TaskEditEntry {
                key: TaskEditKey::Tags,
                label: "tags",
                value: display_list(&task.tags),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::Scope,
                label: "scope",
                value: display_list(&task.scope),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::Evidence,
                label: "evidence",
                value: display_list(&task.evidence),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::Plan,
                label: "plan",
                value: display_list(&task.plan),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::Notes,
                label: "notes",
                value: display_list(&task.notes),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::Request,
                label: "request",
                value: display_optional(task.request.as_deref()),
                kind: TaskEditKind::OptionalText,
            },
            TaskEditEntry {
                key: TaskEditKey::DependsOn,
                label: "depends_on",
                value: display_list(&task.depends_on),
                kind: TaskEditKind::List,
            },
            TaskEditEntry {
                key: TaskEditKey::CustomFields,
                label: "custom_fields",
                value: format_custom_fields(&task.custom_fields, "(empty)"),
                kind: TaskEditKind::Map,
            },
            TaskEditEntry {
                key: TaskEditKey::CreatedAt,
                label: "created_at",
                value: display_optional(task.created_at.as_deref()),
                kind: TaskEditKind::OptionalText,
            },
            TaskEditEntry {
                key: TaskEditKey::UpdatedAt,
                label: "updated_at",
                value: display_optional(task.updated_at.as_deref()),
                kind: TaskEditKind::OptionalText,
            },
            TaskEditEntry {
                key: TaskEditKey::CompletedAt,
                label: "completed_at",
                value: display_optional(task.completed_at.as_deref()),
                kind: TaskEditKind::OptionalText,
            },
        ]
    }

    pub(crate) fn task_value_for_edit(&self, key: TaskEditKey) -> String {
        let Some(task) = self.selected_task() else {
            return String::new();
        };
        match key {
            TaskEditKey::Title => task.title.clone(),
            TaskEditKey::Tags => task.tags.join(", "),
            TaskEditKey::Scope => task.scope.join(", "),
            TaskEditKey::Evidence => task.evidence.join(", "),
            TaskEditKey::Plan => task.plan.join(", "),
            TaskEditKey::Notes => task.notes.join(", "),
            TaskEditKey::Request => task.request.clone().unwrap_or_default(),
            TaskEditKey::DependsOn => task.depends_on.join(", "),
            TaskEditKey::CustomFields => format_custom_fields(&task.custom_fields, ""),
            TaskEditKey::CreatedAt => task.created_at.clone().unwrap_or_default(),
            TaskEditKey::UpdatedAt => task.updated_at.clone().unwrap_or_default(),
            TaskEditKey::CompletedAt => task.completed_at.clone().unwrap_or_default(),
            TaskEditKey::Status | TaskEditKey::Priority => String::new(),
        }
    }

    pub(crate) fn apply_task_edit(
        &mut self,
        key: TaskEditKey,
        input: &str,
        now_rfc3339: &str,
    ) -> Result<()> {
        let task_id = self
            .selected_task()
            .map(|t| t.id.clone())
            .ok_or_else(|| anyhow!("No task selected"))?;
        let index = self
            .selected_task_index()
            .ok_or_else(|| anyhow!("No task selected"))?;
        let previous = self
            .queue
            .tasks
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow!("No task selected"))?;

        let task = self
            .queue
            .tasks
            .get_mut(index)
            .ok_or_else(|| anyhow!("No task selected"))?;

        let trimmed = input.trim();

        match key {
            TaskEditKey::Title => {
                if trimmed.is_empty() {
                    bail!("Title cannot be empty");
                }
                task.title = trimmed.to_string();
            }
            TaskEditKey::Status => {
                let next_status = match task.status {
                    TaskStatus::Draft => TaskStatus::Todo,
                    TaskStatus::Todo => TaskStatus::Doing,
                    TaskStatus::Doing => TaskStatus::Done,
                    TaskStatus::Done => TaskStatus::Rejected,
                    TaskStatus::Rejected => TaskStatus::Draft,
                };
                queue::apply_status_policy(task, next_status, now_rfc3339, None)?;
            }
            TaskEditKey::Priority => {
                task.priority = task.priority.cycle();
            }
            TaskEditKey::Tags => {
                task.tags = Self::parse_list(trimmed);
            }
            TaskEditKey::Scope => {
                task.scope = Self::parse_list(trimmed);
            }
            TaskEditKey::Evidence => {
                task.evidence = Self::parse_list(trimmed);
            }
            TaskEditKey::Plan => {
                task.plan = Self::parse_list(trimmed);
            }
            TaskEditKey::Notes => {
                task.notes = Self::parse_list(trimmed);
            }
            TaskEditKey::Request => {
                task.request = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            TaskEditKey::DependsOn => {
                task.depends_on = Self::parse_list(trimmed);
            }
            TaskEditKey::CustomFields => {
                task.custom_fields = Self::parse_custom_fields(trimmed)?;
            }
            TaskEditKey::CreatedAt => {
                Self::validate_rfc3339_input("created_at", trimmed)?;
                task.created_at = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            TaskEditKey::UpdatedAt => {
                Self::validate_rfc3339_input("updated_at", trimmed)?;
                task.updated_at = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            TaskEditKey::CompletedAt => {
                Self::validate_rfc3339_input("completed_at", trimmed)?;
                task.completed_at = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
        }

        if !matches!(key, TaskEditKey::UpdatedAt) {
            task.updated_at = Some(now_rfc3339.to_string());
        }

        if let Err(err) = queue::validate_queue_set(
            &self.queue,
            Some(&self.done),
            &self.id_prefix,
            self.id_width,
        ) {
            self.queue.tasks[index] = previous;
            return Err(err);
        }

        self.dirty = true;
        self.set_status_message(format!("Updated {}", task_id));
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        Ok(())
    }

    /// Create a new task with default fields and the provided title.
    pub fn create_task_from_title(&mut self, title: &str, now_rfc3339: &str) -> Result<()> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            bail!("Title cannot be empty");
        }

        let next_id = queue::next_id_across(
            &self.queue,
            Some(&self.done),
            &self.id_prefix,
            self.id_width,
        )?;

        let task = Task {
            id: next_id.clone(),
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
            custom_fields: HashMap::new(),
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
        self.set_status_message(format!("Created {}", next_id));
        Ok(())
    }

    /// Archive terminal tasks (Done/Rejected) into the done queue.
    pub fn archive_terminal_tasks(&mut self, now_rfc3339: &str) -> Result<usize> {
        let report =
            queue::archive_terminal_tasks_in_memory(&mut self.queue, &mut self.done, now_rfc3339)?;

        if report.moved_ids.is_empty() {
            self.set_status_message("No done/rejected tasks to archive");
            return Ok(0);
        }

        let moved_count = report.moved_ids.len();

        self.dirty = true;
        self.dirty_done = true;

        self.rebuild_filtered_view();
        self.set_status_message(format!("Archived {} task(s)", moved_count));
        Ok(moved_count)
    }

    /// Cycle the active status filter.
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

    pub fn set_scope_filters(&mut self, scopes: Vec<String>) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());
        self.filters.search_options.scopes = scopes;
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

    fn parse_list(input: &str) -> Vec<String> {
        input
            .split([',', '\n'])
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }

    fn parse_custom_fields(input: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if input.trim().is_empty() {
            return Ok(map);
        }

        for raw in input.split([',', '\n']) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (key, value) = trimmed
                .split_once('=')
                .ok_or_else(|| anyhow!("Custom field entry must be key=value"))?;
            let key = key.trim();
            if key.is_empty() {
                bail!("Custom field key cannot be empty");
            }
            if key.chars().any(|c| c.is_whitespace()) {
                bail!("Custom field keys cannot contain whitespace");
            }
            let value = value.trim();
            map.insert(key.to_string(), value.to_string());
        }
        Ok(map)
    }

    fn validate_rfc3339_input(label: &str, value: &str) -> Result<()> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        OffsetDateTime::parse(trimmed, &Rfc3339)
            .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
        Ok(())
    }

    /// Parse comma or whitespace-separated tags from input string.
    pub fn parse_tags(input: &str) -> Vec<String> {
        input
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
    }

    pub(crate) fn config_entries(&self) -> Vec<ConfigEntry> {
        vec![
            ConfigEntry {
                key: ConfigKey::ProjectType,
                label: "project_type",
                value: display_project_type(self.project_config.project_type),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::QueueFile,
                label: "queue.file",
                value: display_path(self.project_config.queue.file.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueDoneFile,
                label: "queue.done_file",
                value: display_path(self.project_config.queue.done_file.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueIdPrefix,
                label: "queue.id_prefix",
                value: display_string(self.project_config.queue.id_prefix.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueIdWidth,
                label: "queue.id_width",
                value: display_u8(self.project_config.queue.id_width),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentRunner,
                label: "agent.runner",
                value: display_runner(self.project_config.agent.runner),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentModel,
                label: "agent.model",
                value: display_model(self.project_config.agent.model.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentReasoningEffort,
                label: "agent.reasoning_effort",
                value: display_reasoning_effort(self.project_config.agent.reasoning_effort),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentIterations,
                label: "agent.iterations",
                value: display_u8(self.project_config.agent.iterations),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentFollowupReasoningEffort,
                label: "agent.followup_reasoning_effort",
                value: display_reasoning_effort(
                    self.project_config.agent.followup_reasoning_effort,
                ),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentCodexBin,
                label: "agent.codex_bin",
                value: display_string(self.project_config.agent.codex_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentOpencodeBin,
                label: "agent.opencode_bin",
                value: display_string(self.project_config.agent.opencode_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentGeminiBin,
                label: "agent.gemini_bin",
                value: display_string(self.project_config.agent.gemini_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudeBin,
                label: "agent.claude_bin",
                value: display_string(self.project_config.agent.claude_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudePermissionMode,
                label: "agent.claude_permission_mode",
                value: display_claude_permission_mode(
                    self.project_config.agent.claude_permission_mode,
                ),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentRequireRepoPrompt,
                label: "agent.require_repoprompt",
                value: display_bool(self.project_config.agent.require_repoprompt),
                kind: ConfigFieldKind::Toggle,
            },
            ConfigEntry {
                key: ConfigKey::AgentGitRevertMode,
                label: "agent.git_revert_mode",
                value: display_git_revert_mode(self.project_config.agent.git_revert_mode),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentGitCommitPushEnabled,
                label: "agent.git_commit_push_enabled",
                value: display_bool(self.project_config.agent.git_commit_push_enabled),
                kind: ConfigFieldKind::Toggle,
            },
            ConfigEntry {
                key: ConfigKey::AgentPhases,
                label: "agent.phases",
                value: display_u8(self.project_config.agent.phases),
                kind: ConfigFieldKind::Cycle,
            },
        ]
    }

    pub(crate) fn config_value_for_edit(&self, key: ConfigKey) -> String {
        match key {
            ConfigKey::QueueFile => self
                .project_config
                .queue
                .file
                .as_ref()
                .map(|p: &PathBuf| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            ConfigKey::QueueDoneFile => self
                .project_config
                .queue
                .done_file
                .as_ref()
                .map(|p: &PathBuf| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            ConfigKey::QueueIdPrefix => self
                .project_config
                .queue
                .id_prefix
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::QueueIdWidth => self
                .project_config
                .queue
                .id_width
                .map(|v: u8| v.to_string())
                .unwrap_or_default(),
            ConfigKey::AgentModel => self
                .project_config
                .agent
                .model
                .as_ref()
                .map(|v: &Model| v.as_str().to_string())
                .unwrap_or_default(),
            ConfigKey::AgentIterations => self
                .project_config
                .agent
                .iterations
                .map(|value: u8| value.to_string())
                .unwrap_or_default(),
            ConfigKey::AgentCodexBin => self
                .project_config
                .agent
                .codex_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentOpencodeBin => self
                .project_config
                .agent
                .opencode_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentGeminiBin => self
                .project_config
                .agent
                .gemini_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentClaudeBin => self
                .project_config
                .agent
                .claude_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub(crate) fn apply_config_text_value(&mut self, key: ConfigKey, input: &str) -> Result<()> {
        let trimmed = input.trim();
        match key {
            ConfigKey::QueueFile => {
                self.project_config.queue.file = if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                };
            }
            ConfigKey::QueueDoneFile => {
                self.project_config.queue.done_file = if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                };
            }
            ConfigKey::QueueIdPrefix => {
                self.project_config.queue.id_prefix = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::QueueIdWidth => {
                self.project_config.queue.id_width = if trimmed.is_empty() {
                    None
                } else {
                    let value: u8 = trimmed
                        .parse()
                        .map_err(|_| anyhow!("queue.id_width must be a valid number (e.g., 4)"))?;
                    if value == 0 {
                        bail!("queue.id_width must be greater than 0");
                    }
                    Some(value)
                };
            }
            ConfigKey::AgentModel => {
                self.project_config.agent.model = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.parse::<Model>().map_err(|msg| anyhow!(msg))?)
                };
            }
            ConfigKey::AgentIterations => {
                self.project_config.agent.iterations = if trimmed.is_empty() {
                    None
                } else {
                    let value: u8 = trimmed.parse().map_err(|_| {
                        anyhow!("agent.iterations must be a valid number (e.g., 1)")
                    })?;
                    if value == 0 {
                        bail!("agent.iterations must be greater than 0");
                    }
                    Some(value)
                };
            }
            ConfigKey::AgentCodexBin => {
                self.project_config.agent.codex_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentOpencodeBin => {
                self.project_config.agent.opencode_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentGeminiBin => {
                self.project_config.agent.gemini_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentClaudeBin => {
                self.project_config.agent.claude_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            _ => {}
        }
        self.dirty_config = true;
        Ok(())
    }

    pub(crate) fn cycle_config_value(&mut self, key: ConfigKey) {
        match key {
            ConfigKey::ProjectType => {
                self.project_config.project_type =
                    cycle_project_type(self.project_config.project_type);
            }
            ConfigKey::AgentRunner => {
                self.project_config.agent.runner = cycle_runner(self.project_config.agent.runner);
            }
            ConfigKey::AgentReasoningEffort => {
                self.project_config.agent.reasoning_effort =
                    cycle_reasoning_effort(self.project_config.agent.reasoning_effort);
            }
            ConfigKey::AgentFollowupReasoningEffort => {
                self.project_config.agent.followup_reasoning_effort =
                    cycle_reasoning_effort(self.project_config.agent.followup_reasoning_effort);
            }
            ConfigKey::AgentClaudePermissionMode => {
                self.project_config.agent.claude_permission_mode =
                    cycle_claude_permission_mode(self.project_config.agent.claude_permission_mode);
            }
            ConfigKey::AgentRequireRepoPrompt => {
                self.project_config.agent.require_repoprompt =
                    cycle_bool(self.project_config.agent.require_repoprompt);
            }
            ConfigKey::AgentGitRevertMode => {
                self.project_config.agent.git_revert_mode =
                    cycle_git_revert_mode(self.project_config.agent.git_revert_mode);
            }
            ConfigKey::AgentGitCommitPushEnabled => {
                self.project_config.agent.git_commit_push_enabled =
                    cycle_bool(self.project_config.agent.git_commit_push_enabled);
            }
            ConfigKey::AgentPhases => {
                self.project_config.agent.phases = cycle_phases(self.project_config.agent.phases);
            }
            _ => {}
        }
        self.dirty_config = true;
    }

    pub(crate) fn clear_config_value(&mut self, key: ConfigKey) {
        match key {
            ConfigKey::ProjectType => self.project_config.project_type = None,
            ConfigKey::QueueFile => self.project_config.queue.file = None,
            ConfigKey::QueueDoneFile => self.project_config.queue.done_file = None,
            ConfigKey::QueueIdPrefix => self.project_config.queue.id_prefix = None,
            ConfigKey::QueueIdWidth => self.project_config.queue.id_width = None,
            ConfigKey::AgentRunner => self.project_config.agent.runner = None,
            ConfigKey::AgentModel => self.project_config.agent.model = None,
            ConfigKey::AgentReasoningEffort => self.project_config.agent.reasoning_effort = None,
            ConfigKey::AgentIterations => self.project_config.agent.iterations = None,
            ConfigKey::AgentFollowupReasoningEffort => {
                self.project_config.agent.followup_reasoning_effort = None;
            }
            ConfigKey::AgentCodexBin => self.project_config.agent.codex_bin = None,
            ConfigKey::AgentOpencodeBin => self.project_config.agent.opencode_bin = None,
            ConfigKey::AgentGeminiBin => self.project_config.agent.gemini_bin = None,
            ConfigKey::AgentClaudeBin => self.project_config.agent.claude_bin = None,
            ConfigKey::AgentClaudePermissionMode => {
                self.project_config.agent.claude_permission_mode = None;
            }
            ConfigKey::AgentRequireRepoPrompt => {
                self.project_config.agent.require_repoprompt = None
            }
            ConfigKey::AgentGitRevertMode => self.project_config.agent.git_revert_mode = None,
            ConfigKey::AgentGitCommitPushEnabled => {
                self.project_config.agent.git_commit_push_enabled = None
            }
            ConfigKey::AgentPhases => self.project_config.agent.phases = None,
        }
        self.dirty_config = true;
    }

    pub fn log_visible_lines(&self) -> usize {
        self.log_visible_lines.max(1)
    }

    pub fn set_log_visible_lines(&mut self, visible_lines: usize) {
        let visible_lines = visible_lines.max(1);
        self.log_visible_lines = visible_lines;
        let max_scroll = self.max_log_scroll(visible_lines);
        if self.autoscroll || self.log_scroll > max_scroll {
            self.log_scroll = max_scroll;
        }
    }

    pub fn max_log_scroll(&self, visible_lines: usize) -> usize {
        self.logs.len().saturating_sub(visible_lines)
    }

    pub fn scroll_logs_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        self.log_scroll = self.log_scroll.saturating_sub(lines);
    }

    pub fn scroll_logs_down(&mut self, lines: usize, visible_lines: usize) {
        if lines == 0 {
            return;
        }
        self.autoscroll = false;
        let max_scroll = self.max_log_scroll(visible_lines);
        self.log_scroll = (self.log_scroll + lines).min(max_scroll);
    }

    pub fn enable_autoscroll(&mut self, visible_lines: usize) {
        self.autoscroll = true;
        self.log_scroll = self.max_log_scroll(visible_lines);
    }

    /// Build the palette entries for a given query.
    pub fn palette_entries(&self, query: &str) -> Vec<PaletteEntry> {
        let toggle_label = if self.loop_active {
            "Stop loop"
        } else {
            "Start loop"
        };

        let mut entries = vec![
            PaletteEntry {
                cmd: PaletteCommand::RunSelected,
                title: "Run selected task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::RunNextRunnable,
                title: "Run next runnable task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleLoop,
                title: toggle_label.to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ArchiveTerminal,
                title: "Archive done/rejected tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::NewTask,
                title: "Create new task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::EditTask,
                title: "Edit selected task".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::EditConfig,
                title: "Edit project config".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ScanRepo,
                title: "Scan repository for tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::Search,
                title: "Search tasks".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::FilterTags,
                title: "Filter by tags".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::FilterScopes,
                title: "Filter by scope".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ClearFilters,
                title: "Clear filters".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::CycleStatus,
                title: "Cycle selected task status".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::CyclePriority,
                title: "Cycle selected task priority".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleCaseSensitive,
                title: "Toggle case-sensitive search".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ToggleRegex,
                title: "Toggle regex search".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::ReloadQueue,
                title: "Reload queue from disk".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::Quit,
                title: "Quit".to_string(),
            },
        ];

        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return entries;
        }

        entries.retain(|e| e.title.to_lowercase().contains(&q));
        entries
    }

    /// Execute a palette command (also used by direct keybinds for consistency).
    pub fn execute_palette_command(
        &mut self,
        cmd: PaletteCommand,
        now_rfc3339: &str,
    ) -> Result<TuiAction> {
        match cmd {
            PaletteCommand::RunSelected => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                    return Ok(TuiAction::Continue);
                }
                if self.loop_active {
                    self.loop_active = false;
                    self.loop_arm_after_current = false;
                    self.set_status_message("Loop stopped (manual run)");
                }
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };
                let task_id = task.id.clone();
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::RunNextRunnable => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                    return Ok(TuiAction::Continue);
                }
                let Some(task_id) = self.next_loop_task_id() else {
                    self.set_status_message("No runnable tasks");
                    return Ok(TuiAction::Continue);
                };
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::ToggleLoop => {
                if self.loop_active {
                    self.loop_active = false;
                    self.loop_arm_after_current = false;
                    self.set_status_message(format!("Loop stopped (ran {})", self.loop_ran));
                    return Ok(TuiAction::Continue);
                }

                self.loop_active = true;
                self.loop_ran = 0;

                if self.runner_active {
                    self.loop_arm_after_current = true;
                    self.set_status_message("Loop armed (will start after current task)");
                    return Ok(TuiAction::Continue);
                }

                let Some(task_id) = self.next_loop_task_id() else {
                    self.loop_active = false;
                    self.set_status_message("No runnable tasks");
                    return Ok(TuiAction::Continue);
                };

                self.set_status_message("Loop started");
                self.start_task_execution(task_id.clone(), true, false);
                Ok(TuiAction::RunTask(task_id))
            }
            PaletteCommand::ArchiveTerminal => {
                if self
                    .queue
                    .tasks
                    .iter()
                    .any(|t| matches!(t.status, TaskStatus::Done | TaskStatus::Rejected))
                {
                    self.mode = AppMode::ConfirmArchive;
                } else {
                    self.set_status_message("No done/rejected tasks to archive");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::NewTask => {
                self.mode = AppMode::CreatingTask(String::new());
                Ok(TuiAction::Continue)
            }
            PaletteCommand::EditTask => {
                if self.selected_task().is_some() {
                    self.mode = AppMode::EditingTask {
                        selected: 0,
                        editing_value: None,
                    };
                } else {
                    self.set_status_message("No task selected");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::EditConfig => {
                self.mode = AppMode::EditingConfig {
                    selected: 0,
                    editing_value: None,
                };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ScanRepo => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                } else {
                    self.mode = AppMode::Scanning(String::new());
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::Search => {
                self.mode = AppMode::Searching(self.filters.query.clone());
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterTags => {
                self.mode = AppMode::FilteringTags(self.filters.tags.join(","));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterScopes => {
                self.mode = AppMode::FilteringScopes(self.filters.search_options.scopes.join(","));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ClearFilters => {
                self.clear_filters();
                self.set_status_message("Filters cleared");
                Ok(TuiAction::Continue)
            }
            PaletteCommand::CycleStatus => {
                if let Err(e) = self.cycle_status(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                } else {
                    self.set_status_message("Status updated");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::CyclePriority => {
                if let Err(e) = self.cycle_priority(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                } else {
                    self.set_status_message("Priority updated");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleCaseSensitive => {
                self.toggle_case_sensitive();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleRegex => {
                self.toggle_regex();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ReloadQueue => Ok(TuiAction::ReloadQueue),
            PaletteCommand::Quit => {
                if self.runner_active {
                    self.mode = AppMode::ConfirmQuit;
                    Ok(TuiAction::Continue)
                } else {
                    Ok(TuiAction::Quit)
                }
            }
        }
    }

    /// Start execution of a specific task.
    fn start_task_execution(&mut self, task_id: String, focus_logs: bool, append_logs: bool) {
        if append_logs && !self.logs.is_empty() {
            self.logs.push(String::new());
            self.logs.push(format!("=== Executing {} ===", task_id));
        } else {
            self.logs.clear();
        }

        self.log_scroll = 0;
        self.autoscroll = true;

        self.runner_active = true;
        self.running_task_id = Some(task_id.clone());
        self.running_kind = Some(RunningKind::Task);

        if focus_logs {
            self.mode = AppMode::Executing { task_id };
        }
    }

    /// Start execution of a scan.
    fn start_scan_execution(&mut self, focus: String, focus_logs: bool, append_logs: bool) {
        let label = scan_label(&focus);
        if append_logs && !self.logs.is_empty() {
            self.logs.push(String::new());
            self.logs.push(format!("=== {} ===", label));
        } else {
            self.logs.clear();
        }

        self.log_scroll = 0;
        self.autoscroll = true;

        self.runner_active = true;
        self.running_task_id = Some(label);
        self.running_kind = Some(RunningKind::Scan {
            focus: focus.clone(),
        });

        if self.loop_active {
            self.loop_active = false;
            self.loop_arm_after_current = false;
            self.set_status_message("Loop stopped (scan run)");
        }

        if focus_logs {
            self.mode = AppMode::Executing { task_id: focus };
        }
    }

    /// Select the next runnable task for loop mode.
    ///
    /// This prefers resuming `doing` tasks, then the first runnable `todo`, then `draft` (when
    /// enabled), while skipping tasks whose dependencies are not met.
    pub fn next_loop_task_id(&self) -> Option<String> {
        let options =
            queue::operations::RunnableSelectionOptions::new(self.loop_include_draft, true);
        queue::operations::select_runnable_task_index(&self.queue, Some(&self.done), options)
            .and_then(|idx| self.queue.tasks.get(idx).map(|task| task.id.clone()))
    }

    /// Rebuild the filtered view.
    pub(crate) fn rebuild_filtered_view(&mut self) {
        self.rebuild_filtered_view_with_preferred(None);
    }

    fn rebuild_filtered_view_with_preferred(&mut self, preferred_id: Option<&str>) {
        let mut filtered = queue::filter_tasks(
            &self.queue,
            &self.filters.statuses,
            &self.filters.tags,
            &self.filters.search_options.scopes,
            None,
        );

        if !self.filters.query.trim().is_empty() {
            match queue::search_tasks(
                filtered,
                &self.filters.query,
                self.filters.search_options.use_regex,
                self.filters.search_options.case_sensitive,
            ) {
                Ok(results) => {
                    filtered = results;
                }
                Err(err) => {
                    self.set_status_message(format!("Search error: {}", err));
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

    /// Reload the queue + done archive from disk.
    fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path) {
        let preferred_id = self.selected_task().map(|t| t.id.clone());

        match queue::load_queue(queue_path) {
            Ok(new_queue) => {
                self.queue = new_queue;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error: {}", e));
                return;
            }
        }

        match queue::load_queue_or_default(done_path) {
            Ok(new_done) => {
                self.done = new_done;
            }
            Err(e) => {
                self.set_status_message(format!("Reload error (done): {}", e));
                return;
            }
        }

        self.rebuild_filtered_view_with_preferred(preferred_id.as_deref());
        self.dirty = false;
        self.dirty_done = false;
        self.save_error = None;
    }
}

fn auto_save_if_dirty(
    app: &mut App,
    queue_path: &Path,
    done_path: &Path,
    project_config_path: Option<&Path>,
) {
    let mut errors: Vec<String> = Vec::new();

    if app.dirty {
        match queue::save_queue(queue_path, &app.queue) {
            Ok(()) => {
                app.dirty = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving queue: {}", e));
            }
        }
    }

    if app.dirty_done {
        match queue::save_queue(done_path, &app.done) {
            Ok(()) => {
                app.dirty_done = false;
            }
            Err(e) => {
                errors.push(format!("ERROR saving done: {}", e));
            }
        }
    }

    if app.dirty_config {
        match project_config_path {
            Some(path) => match config::save_layer(path, &app.project_config) {
                Ok(()) => {
                    app.dirty_config = false;
                }
                Err(e) => {
                    errors.push(format!("ERROR saving config: {}", e));
                }
            },
            None => {
                errors.push("ERROR saving config: missing project config_path".to_string());
            }
        }
    }

    if errors.is_empty() {
        app.save_error = None;
        return;
    }

    let message = errors.join(" | ");
    let should_log = app.save_error.as_deref() != Some(message.as_str());
    app.save_error = Some(message.clone());
    if should_log {
        app.set_status_message(message);
    }
}

/// Event sent from the runner thread to the TUI.
enum RunnerEvent {
    /// Output chunk received
    Output(String),
    /// Task finished (success)
    Finished,
    /// Task failed with error
    Error(String),
    /// Revert prompt requested by the runner.
    RevertPrompt {
        label: String,
        reply: mpsc::Sender<runutil::RevertDecision>,
    },
}

/// Run the TUI application with an active queue lock.
///
/// The `runner_factory` creates a closure that executes a task when called.
/// It receives a task ID and an output handler callback.
/// The `scan_factory` creates a closure that runs a scan when called.
/// It receives a scan focus string and an output handler callback.
pub fn run_tui<F, E, S, SE>(
    resolved: &crate::config::Resolved,
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

    // Setup terminal.
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    // Create channels for runner events.
    let (tx, rx) = mpsc::channel::<RunnerEvent>();

    let build_handlers = |tx: &mpsc::Sender<RunnerEvent>| {
        let tx_clone_for_handler = tx.clone();
        let tx_clone_for_prompt = tx.clone();
        let handler: crate::runner::OutputHandler = Arc::new(Box::new(move |text: &str| {
            let _ = tx_clone_for_handler.send(RunnerEvent::Output(text.to_string()));
        }));

        let revert_prompt: runutil::RevertPromptHandler = Arc::new(move |label: &str| {
            let (reply_tx, reply_rx) = mpsc::channel();
            if tx_clone_for_prompt
                .send(RunnerEvent::RevertPrompt {
                    label: label.to_string(),
                    reply: reply_tx,
                })
                .is_err()
            {
                return runutil::RevertDecision::Keep;
            }
            reply_rx.recv().unwrap_or(runutil::RevertDecision::Keep)
        });

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

        // Main event loop.
        loop {
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
                match event {
                    RunnerEvent::Output(text) => {
                        for line in text.lines() {
                            app_ref.logs.push(line.to_string());
                        }
                        if app_ref.logs.len() > 10000 {
                            let excess = app_ref.logs.len() - 10000;
                            app_ref.logs.drain(0..excess);
                            app_ref.log_scroll = app_ref.log_scroll.saturating_sub(excess);
                        }
                        if app_ref.autoscroll {
                            let visible_lines = app_ref.log_visible_lines();
                            app_ref.log_scroll = app_ref.max_log_scroll(visible_lines);
                        }
                    }
                    RunnerEvent::Finished => {
                        app_ref.runner_active = false;
                        app_ref.running_task_id = None;
                        let running_kind = app_ref.running_kind.take();

                        match running_kind {
                            Some(RunningKind::Scan { .. }) => {
                                app_ref.reload_queues_from_disk(queue_path, done_path);
                                app_ref.set_status_message("Scan completed");
                                if matches!(
                                    app_ref.mode,
                                    AppMode::Executing { .. } | AppMode::ConfirmQuit
                                ) {
                                    app_ref.mode = AppMode::Normal;
                                }
                            }
                            Some(RunningKind::Task) | None => {
                                // Reload both queues to capture changes made by the runner.
                                app_ref.reload_queues_from_disk(queue_path, done_path);

                                // If we were in a quit confirmation, leave it.
                                if app_ref.mode == AppMode::ConfirmQuit {
                                    app_ref.mode = AppMode::Normal;
                                }

                                // Loop continuation logic.
                                if app_ref.loop_active {
                                    if app_ref.loop_arm_after_current {
                                        app_ref.loop_arm_after_current = false;
                                    } else {
                                        app_ref.loop_ran = app_ref.loop_ran.saturating_add(1);
                                    }

                                    if let Some(max) = app_ref.loop_max_tasks {
                                        if app_ref.loop_ran >= max {
                                            let loop_ran = app_ref.loop_ran;
                                            app_ref.loop_active = false;
                                            app_ref.set_status_message(format!(
                                                "Loop finished (ran {}/{})",
                                                loop_ran, max
                                            ));
                                        }
                                    }

                                    if app_ref.loop_active {
                                        if let Some(next_id) = app_ref.next_loop_task_id() {
                                            let focus_logs =
                                                matches!(app_ref.mode, AppMode::Executing { .. });
                                            app_ref.start_task_execution(
                                                next_id.clone(),
                                                focus_logs,
                                                true,
                                            );
                                            next_to_start = Some(next_id);
                                        } else {
                                            let loop_ran = app_ref.loop_ran;
                                            app_ref.loop_active = false;
                                            app_ref.set_status_message(format!(
                                                "Loop complete (ran {})",
                                                loop_ran
                                            ));
                                        }
                                    }
                                } else if matches!(
                                    app_ref.mode,
                                    AppMode::Executing { .. } | AppMode::ConfirmQuit
                                ) {
                                    app_ref.mode = AppMode::Normal;
                                }
                            }
                        }
                    }
                    RunnerEvent::Error(msg) => {
                        app_ref.runner_active = false;
                        app_ref.running_task_id = None;
                        let running_kind = app_ref.running_kind.take();

                        app_ref.loop_active = false;
                        app_ref.loop_arm_after_current = false;

                        match running_kind {
                            Some(RunningKind::Scan { .. }) => {
                                app_ref.set_status_message(format!("Scan error: {}", msg));
                            }
                            Some(RunningKind::Task) | None => {
                                app_ref.set_status_message(format!("Runner error: {}", msg));
                            }
                        }
                        if matches!(
                            app_ref.mode,
                            AppMode::Executing { .. } | AppMode::ConfirmQuit
                        ) {
                            app_ref.mode = AppMode::Normal;
                        }
                    }
                    RunnerEvent::RevertPrompt { label, reply } => {
                        let previous_mode = app_ref.mode.clone();
                        app_ref.mode = AppMode::ConfirmRevert {
                            label,
                            selected: 0,
                            input: String::new(),
                            reply_sender: reply,
                            previous_mode: Box::new(previous_mode),
                        };
                    }
                }
            }

            if let Some(id) = next_to_start {
                spawn_task(id, tx.clone());
            }

            // Auto-save if dirty.
            if app.borrow().dirty || app.borrow().dirty_done || app.borrow().dirty_config {
                let mut app_ref = app.borrow_mut();
                let config_path = app_ref.project_config_path.clone();
                auto_save_if_dirty(&mut app_ref, queue_path, done_path, config_path.as_deref());
            }

            // Handle input events.
            if event::poll(Duration::from_millis(100)).context("poll event")? {
                if let Event::Key(key) = event::read().context("read event")? {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }

                    let mut app_ref = app.borrow_mut();
                    let now = timeutil::now_utc_rfc3339()?;
                    match handle_key_event(&mut app_ref, key.code, &now)? {
                        TuiAction::Quit => break,
                        TuiAction::Continue => {}
                        TuiAction::ReloadQueue => {
                            app_ref.reload_queues_from_disk(queue_path, done_path);
                        }
                        TuiAction::RunTask(task_id) => {
                            let tx_clone = tx.clone();
                            spawn_task(task_id, tx_clone);
                        }
                        TuiAction::RunScan(focus) => {
                            app_ref.start_scan_execution(focus.clone(), true, false);
                            let tx_clone = tx.clone();
                            spawn_scan(focus, tx_clone);
                        }
                    }
                }
            }
        }

        Ok::<_, anyhow::Error>(None)
    }));

    // Cleanup terminal.
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
    let (queue, done) = queue::load_and_validate_queues(resolved, true)?;
    let mut app = App::new(queue);
    app.done = done.unwrap_or_default();
    app.id_prefix = resolved.id_prefix.clone();
    app.id_width = resolved.id_width;
    app.done_path = Some(resolved.done_path.clone());

    let mut project_config = ConfigLayer::default();
    let mut project_config_path = None;
    if let Some(path) = resolved.project_config_path.as_ref() {
        project_config_path = Some(path.clone());
        if path.exists() {
            project_config = config::load_layer(path)
                .with_context(|| format!("load project config {}", path.display()))?;
        }
    }
    app.project_config = project_config;
    app.project_config_path = project_config_path;

    Ok((app, lock))
}

fn default_config_value() -> String {
    "(global default)".to_string()
}

fn display_project_type(value: Option<ProjectType>) -> String {
    match value {
        Some(ProjectType::Code) => "code".to_string(),
        Some(ProjectType::Docs) => "docs".to_string(),
        None => default_config_value(),
    }
}

fn display_runner(value: Option<Runner>) -> String {
    match value {
        Some(Runner::Codex) => "codex".to_string(),
        Some(Runner::Opencode) => "opencode".to_string(),
        Some(Runner::Gemini) => "gemini".to_string(),
        Some(Runner::Claude) => "claude".to_string(),
        None => default_config_value(),
    }
}

fn display_reasoning_effort(value: Option<ReasoningEffort>) -> String {
    match value {
        Some(ReasoningEffort::Low) => "low".to_string(),
        Some(ReasoningEffort::Medium) => "medium".to_string(),
        Some(ReasoningEffort::High) => "high".to_string(),
        Some(ReasoningEffort::XHigh) => "xhigh".to_string(),
        None => default_config_value(),
    }
}

fn display_claude_permission_mode(value: Option<ClaudePermissionMode>) -> String {
    match value {
        Some(ClaudePermissionMode::AcceptEdits) => "accept_edits".to_string(),
        Some(ClaudePermissionMode::BypassPermissions) => "bypass_permissions".to_string(),
        None => default_config_value(),
    }
}

fn display_git_revert_mode(value: Option<GitRevertMode>) -> String {
    match value {
        Some(GitRevertMode::Ask) => "ask".to_string(),
        Some(GitRevertMode::Enabled) => "enabled".to_string(),
        Some(GitRevertMode::Disabled) => "disabled".to_string(),
        None => default_config_value(),
    }
}

fn display_model(value: Option<&Model>) -> String {
    match value {
        Some(model) => model.as_str().to_string(),
        None => default_config_value(),
    }
}

fn display_string(value: Option<&String>) -> String {
    match value {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => default_config_value(),
    }
}

fn display_path(value: Option<&PathBuf>) -> String {
    match value {
        Some(path) => path.to_string_lossy().to_string(),
        None => default_config_value(),
    }
}

fn display_u8(value: Option<u8>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => default_config_value(),
    }
}

fn display_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => default_config_value(),
    }
}

fn display_optional(value: Option<&str>) -> String {
    match value {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => "(empty)".to_string(),
    }
}

fn scan_label(focus: &str) -> String {
    let trimmed = focus.trim();
    if trimmed.is_empty() {
        "scan: (all)".to_string()
    } else {
        format!("scan: {}", trimmed)
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "(empty)".to_string()
    } else {
        values.join(", ")
    }
}

fn cycle_project_type(value: Option<ProjectType>) -> Option<ProjectType> {
    match value {
        None => Some(ProjectType::Code),
        Some(ProjectType::Code) => Some(ProjectType::Docs),
        Some(ProjectType::Docs) => None,
    }
}

fn cycle_runner(value: Option<Runner>) -> Option<Runner> {
    match value {
        None => Some(Runner::Codex),
        Some(Runner::Codex) => Some(Runner::Opencode),
        Some(Runner::Opencode) => Some(Runner::Gemini),
        Some(Runner::Gemini) => Some(Runner::Claude),
        Some(Runner::Claude) => None,
    }
}

fn cycle_reasoning_effort(value: Option<ReasoningEffort>) -> Option<ReasoningEffort> {
    match value {
        None => Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Low) => Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::Medium) => Some(ReasoningEffort::High),
        Some(ReasoningEffort::High) => Some(ReasoningEffort::XHigh),
        Some(ReasoningEffort::XHigh) => None,
    }
}

fn cycle_claude_permission_mode(
    value: Option<ClaudePermissionMode>,
) -> Option<ClaudePermissionMode> {
    match value {
        None => Some(ClaudePermissionMode::AcceptEdits),
        Some(ClaudePermissionMode::AcceptEdits) => Some(ClaudePermissionMode::BypassPermissions),
        Some(ClaudePermissionMode::BypassPermissions) => None,
    }
}

fn cycle_git_revert_mode(value: Option<GitRevertMode>) -> Option<GitRevertMode> {
    match value {
        None => Some(GitRevertMode::Ask),
        Some(GitRevertMode::Ask) => Some(GitRevertMode::Enabled),
        Some(GitRevertMode::Enabled) => Some(GitRevertMode::Disabled),
        Some(GitRevertMode::Disabled) => None,
    }
}

fn cycle_bool(value: Option<bool>) -> Option<bool> {
    match value {
        None => Some(true),
        Some(true) => Some(false),
        Some(false) => None,
    }
}

fn cycle_phases(value: Option<u8>) -> Option<u8> {
    match value {
        None => Some(1),
        Some(1) => Some(2),
        Some(2) => Some(3),
        Some(3) => None,
        Some(_) => None,
    }
}

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
    fn app_create_task_from_title_appends_with_defaults() -> Result<()> {
        let mut done_queue = QueueFile::default();
        let mut done_task = make_test_task("RQ-0005", "Done Task", TaskStatus::Done);
        done_task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
        done_queue.tasks.push(done_task);

        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0003", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);
        app.id_prefix = "RQ".to_string();
        app.id_width = 4;
        app.done = done_queue;

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
        assert!(app.dirty);
        assert_eq!(app.mode, AppMode::Normal);
        Ok(())
    }

    #[test]
    fn apply_task_edit_parses_list_fields() -> Result<()> {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.apply_task_edit(
            TaskEditKey::Tags,
            "alpha, beta,, gamma \n delta",
            "2026-01-20T12:00:00Z",
        )?;

        assert_eq!(
            app.queue.tasks[0].tags,
            vec![
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
                "delta".to_string()
            ]
        );
        Ok(())
    }

    #[test]
    fn apply_task_edit_cycles_status_with_policy() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Done)],
        };
        queue.tasks[0].completed_at = Some("2026-01-19T00:00:00Z".to_string());

        let mut app = App::new(queue);

        app.apply_task_edit(TaskEditKey::Status, "", "2026-01-20T12:00:00Z")?;
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Rejected);
        assert_eq!(
            app.queue.tasks[0].completed_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );
        assert_eq!(
            app.queue.tasks[0].updated_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );

        app.apply_task_edit(TaskEditKey::Status, "", "2026-01-21T12:00:00Z")?;
        assert_eq!(app.queue.tasks[0].status, TaskStatus::Draft);
        assert!(app.queue.tasks[0].completed_at.is_none());
        assert_eq!(
            app.queue.tasks[0].updated_at.as_deref(),
            Some("2026-01-21T12:00:00Z")
        );
        Ok(())
    }

    #[test]
    fn apply_task_edit_custom_fields_parses_and_validates() -> Result<()> {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.apply_task_edit(
            TaskEditKey::CustomFields,
            "foo=bar, baz=qux",
            "2026-01-20T12:00:00Z",
        )?;
        assert_eq!(
            app.queue.tasks[0]
                .custom_fields
                .get("foo")
                .map(String::as_str),
            Some("bar")
        );
        assert_eq!(
            app.queue.tasks[0]
                .custom_fields
                .get("baz")
                .map(String::as_str),
            Some("qux")
        );

        let err = app
            .apply_task_edit(
                TaskEditKey::CustomFields,
                "bad key=value",
                "2026-01-20T12:10:00Z",
            )
            .expect_err("expected invalid custom field key");
        assert!(err.to_string().contains("whitespace"));
        assert_eq!(
            app.queue.tasks[0]
                .custom_fields
                .get("foo")
                .map(String::as_str),
            Some("bar")
        );
        Ok(())
    }

    #[test]
    fn apply_task_edit_clears_optional_field() -> Result<()> {
        let mut task = make_test_task("RQ-0001", "Task 1", TaskStatus::Todo);
        task.completed_at = Some("2026-01-20T00:00:00Z".to_string());
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let mut app = App::new(queue);

        app.apply_task_edit(TaskEditKey::CompletedAt, "", "2026-01-20T12:00:00Z")?;
        assert!(app.queue.tasks[0].completed_at.is_none());
        Ok(())
    }

    #[test]
    fn apply_task_edit_rejects_invalid_updated_at() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        let err = app
            .apply_task_edit(
                TaskEditKey::UpdatedAt,
                "not-a-timestamp",
                "2026-01-20T12:00:00Z",
            )
            .expect_err("expected invalid updated_at");
        assert!(err.to_string().contains("updated_at"));
    }

    #[test]
    fn apply_task_edit_preserves_manual_updated_at() -> Result<()> {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        app.apply_task_edit(
            TaskEditKey::UpdatedAt,
            "2026-01-20T12:00:00Z",
            "2026-01-22T12:00:00Z",
        )?;

        assert_eq!(
            app.queue.tasks[0].updated_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );
        Ok(())
    }

    #[test]
    fn apply_task_edit_rejects_invalid_dependency() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                make_test_task("RQ-0001", "Task 1", TaskStatus::Todo),
                make_test_task("RQ-0002", "Task 2", TaskStatus::Todo),
            ],
        };
        let mut app = App::new(queue);

        let err = app
            .apply_task_edit(TaskEditKey::DependsOn, "RQ-9999", "2026-01-20T12:00:00Z")
            .expect_err("expected invalid dependency");
        assert!(err.to_string().contains("Invalid dependency"));
        assert!(app.queue.tasks[0].depends_on.is_empty());
    }

    #[test]
    fn auto_save_clears_dirty_on_success() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let done_path = temp.path().join("done.json");
        let config_path = temp.path().join("config.json");

        let queue = QueueFile::default();
        let mut app = App::new(queue);
        app.dirty = true;
        app.dirty_done = true;
        app.dirty_config = true;
        app.project_config_path = Some(config_path.clone());

        auto_save_if_dirty(&mut app, &queue_path, &done_path, Some(&config_path));

        assert!(!app.dirty);
        assert!(!app.dirty_done);
        assert!(!app.dirty_config);
        assert!(app.save_error.is_none());
        assert!(queue_path.exists());
        assert!(done_path.exists());
        assert!(config_path.exists());
        Ok(())
    }

    #[test]
    fn config_text_entry_rejects_invalid_id_width() {
        let mut app = App::new(QueueFile::default());
        let err = app
            .apply_config_text_value(ConfigKey::QueueIdWidth, "0")
            .expect_err("invalid id_width");
        assert!(err.to_string().contains("id_width"));
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
    fn archive_terminal_tasks_noop_when_none_terminal() -> Result<()> {
        let queue = QueueFile {
            version: 1,
            tasks: vec![make_test_task("RQ-0001", "Task 1", TaskStatus::Todo)],
        };
        let mut app = App::new(queue);

        let moved = app.archive_terminal_tasks("2026-01-20T12:00:00Z")?;

        assert_eq!(moved, 0);
        assert_eq!(app.queue.tasks.len(), 1);
        assert_eq!(app.done.tasks.len(), 0);
        assert_eq!(
            app.status_message.as_deref(),
            Some("No done/rejected tasks to archive")
        );
        Ok(())
    }

    #[test]
    fn archive_terminal_tasks_stamps_timestamps() -> Result<()> {
        let mut done_task = make_test_task("RQ-0001", "Task 1", TaskStatus::Done);
        done_task.updated_at = None;
        done_task.completed_at = None;

        let mut rejected_task = make_test_task("RQ-0002", "Task 2", TaskStatus::Rejected);
        rejected_task.updated_at = Some("2026-01-19T00:00:00Z".to_string());
        rejected_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());

        let queue = QueueFile {
            version: 1,
            tasks: vec![done_task, rejected_task],
        };
        let mut app = App::new(queue);

        let moved = app.archive_terminal_tasks("2026-01-20T12:00:00Z")?;

        assert_eq!(moved, 2);
        assert!(app.dirty);
        assert!(app.dirty_done);
        assert_eq!(app.queue.tasks.len(), 0);
        assert_eq!(app.done.tasks.len(), 2);

        let done_archived = app
            .done
            .tasks
            .iter()
            .find(|t| t.id == "RQ-0001")
            .expect("RQ-0001 archived");
        assert_eq!(
            done_archived.updated_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );
        assert_eq!(
            done_archived.completed_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );

        let rejected_archived = app
            .done
            .tasks
            .iter()
            .find(|t| t.id == "RQ-0002")
            .expect("RQ-0002 archived");
        assert_eq!(
            rejected_archived.updated_at.as_deref(),
            Some("2026-01-20T12:00:00Z")
        );
        assert_eq!(
            rejected_archived.completed_at.as_deref(),
            Some("2026-01-19T00:00:00Z")
        );
        Ok(())
    }

    #[test]
    fn palette_entries_include_scan_command() {
        let app = App::new(QueueFile::default());
        let entries = app.palette_entries("");
        assert!(entries
            .iter()
            .any(|entry| matches!(entry.cmd, PaletteCommand::ScanRepo)));
    }

    #[test]
    fn scan_label_formats_focus() {
        assert_eq!(scan_label(""), "scan: (all)");
        assert_eq!(scan_label("  security "), "scan: security");
    }

    #[test]
    fn start_scan_execution_sets_running_label() {
        let mut app = App::new(QueueFile::default());
        app.start_scan_execution("focus".to_string(), false, false);
        assert_eq!(app.running_task_id.as_deref(), Some("scan: focus"));
        assert!(matches!(app.running_kind, Some(RunningKind::Scan { .. })));
    }

    #[test]
    fn filter_summary_includes_case_sensitive() {
        let mut app = App::new(QueueFile::default());
        app.filters.search_options.case_sensitive = true;
        app.filters.query = "test".to_string();

        let summary = app.filter_summary();
        assert!(summary.is_some());
        assert!(summary.as_ref().unwrap().contains("case-sensitive"));
        assert!(summary.as_ref().unwrap().contains("query=test"));
    }

    #[test]
    fn filter_summary_includes_regex() {
        let mut app = App::new(QueueFile::default());
        app.filters.search_options.use_regex = true;
        app.filters.query = "RQ-\\d{4}".to_string();

        let summary = app.filter_summary();
        assert!(summary.is_some());
        assert!(summary.as_ref().unwrap().contains("regex"));
        assert!(summary.as_ref().unwrap().contains("query=RQ-\\d{4}"));
    }

    #[test]
    fn filter_summary_includes_both_search_options() {
        let mut app = App::new(QueueFile::default());
        app.filters.search_options.use_regex = true;
        app.filters.search_options.case_sensitive = true;
        app.filters.query = "test".to_string();

        let summary = app.filter_summary();
        assert!(summary.is_some());
        assert!(summary.as_ref().unwrap().contains("regex"));
        assert!(summary.as_ref().unwrap().contains("case-sensitive"));
    }

    #[test]
    fn has_active_filters_detects_search_options() {
        let mut app = App::new(QueueFile::default());

        assert!(!app.has_active_filters(), "no filters active by default");

        app.filters.search_options.use_regex = true;
        assert!(
            app.has_active_filters(),
            "regex option makes filters active"
        );

        app.filters.search_options.use_regex = false;
        assert!(!app.has_active_filters(), "regex option disabled");

        app.filters.search_options.case_sensitive = true;
        assert!(
            app.has_active_filters(),
            "case-sensitive option makes filters active"
        );

        app.filters.search_options.case_sensitive = false;
        assert!(!app.has_active_filters(), "case-sensitive option disabled");
    }

    #[test]
    fn filter_summary_includes_scopes() {
        let mut app = App::new(QueueFile::default());
        app.filters.search_options.scopes = vec!["crates/ralph".to_string()];
        app.filters.query = "test".to_string();

        let summary = app.filter_summary();
        assert!(summary.is_some());
        assert!(summary.as_ref().unwrap().contains("scope=crates/ralph"));
        assert!(summary.as_ref().unwrap().contains("query=test"));
    }

    #[test]
    fn has_active_filters_detects_scopes() {
        let mut app = App::new(QueueFile::default());

        assert!(!app.has_active_filters(), "no filters active by default");

        app.filters.search_options.scopes = vec!["frontend".to_string()];
        assert!(
            app.has_active_filters(),
            "scope filter makes filters active"
        );

        app.filters.search_options.scopes.clear();
        assert!(!app.has_active_filters(), "scope filter disabled");
    }
}
