use crate::config::ConfigLayer;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::{config as crate_config, fsutil, queue, runutil, timeutil};
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

use super::events::{handle_key_event, AppMode, PaletteCommand, PaletteEntry, TuiAction};
use super::render::draw_ui;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunningKind {
    Task,
    Scan { focus: String },
    TaskBuilder,
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

    pub(crate) fn append_log_lines<I>(&mut self, lines: I)
    where
        I: IntoIterator<Item = String>,
    {
        for line in lines {
            self.logs.push(line);
        }
        if self.logs.len() > 10000 {
            let excess = self.logs.len() - 10000;
            self.logs.drain(0..excess);
            self.log_scroll = self.log_scroll.saturating_sub(excess);
        }
        if self.autoscroll {
            let visible_lines = self.log_visible_lines();
            self.log_scroll = self.max_log_scroll(visible_lines);
        }
    }

    pub(crate) fn set_runner_error(&mut self, msg: &str) {
        let summary_line = msg
            .lines()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
            .unwrap_or("Runner error");
        let status = if summary_line == "Runner error" {
            "Runner error (see logs)".to_string()
        } else {
            format!("Runner error: {} (see logs)", summary_line)
        };
        self.set_status_message(status);

        let mut lines = Vec::new();
        lines.push("Runner error details:".to_string());
        if msg.trim().is_empty() {
            lines.push("(no details provided)".to_string());
        } else {
            for line in msg.lines() {
                lines.push(line.to_string());
            }
        }
        self.append_log_lines(lines);
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

    /// Move the selected task up in the queue.
    pub fn move_task_up(&mut self, now_rfc3339: &str) -> Result<()> {
        if self.selected == 0 || self.filtered_indices.is_empty() {
            return Ok(());
        }

        let current_idx = self.filtered_indices[self.selected];
        let prev_idx = self.filtered_indices[self.selected - 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[prev_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, prev_idx);
        self.dirty = true;

        let task_id = self.queue.tasks[prev_idx].id.clone();
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        self.set_status_message(format!("Moved {} up", task_id));

        Ok(())
    }

    /// Move the selected task down in the queue.
    pub fn move_task_down(&mut self, now_rfc3339: &str) -> Result<()> {
        if self.selected + 1 >= self.filtered_indices.len() || self.filtered_indices.is_empty() {
            return Ok(());
        }

        let current_idx = self.filtered_indices[self.selected];
        let next_idx = self.filtered_indices[self.selected + 1];

        self.queue.tasks[current_idx].updated_at = Some(now_rfc3339.to_string());
        self.queue.tasks[next_idx].updated_at = Some(now_rfc3339.to_string());

        self.queue.tasks.swap(current_idx, next_idx);
        self.dirty = true;

        let task_id = self.queue.tasks[next_idx].id.clone();
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        self.set_status_message(format!("Moved {} down", task_id));

        Ok(())
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

    pub(crate) fn parse_list(input: &str) -> Vec<String> {
        input
            .split([',', '\n'])
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }

    /// Parse comma or whitespace-separated tags from input string.
    pub(crate) fn parse_tags(input: &str) -> Vec<String> {
        input
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect()
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
                cmd: PaletteCommand::BuildTaskAgent,
                title: "Build task with agent".to_string(),
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
                cmd: PaletteCommand::MoveTaskUp,
                title: "Move selected task up".to_string(),
            },
            PaletteEntry {
                cmd: PaletteCommand::MoveTaskDown,
                title: "Move selected task down".to_string(),
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
            PaletteCommand::BuildTaskAgent => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                } else {
                    self.mode = AppMode::CreatingTaskDescription(String::new());
                }
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
            PaletteCommand::MoveTaskUp => {
                if let Err(e) = self.move_task_up(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::MoveTaskDown => {
                if let Err(e) = self.move_task_down(now_rfc3339) {
                    self.set_status_message(format!("Error: {}", e));
                }
                Ok(TuiAction::Continue)
            }
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
    pub(crate) fn start_task_execution(
        &mut self,
        task_id: String,
        focus_logs: bool,
        append_logs: bool,
    ) {
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
    pub(crate) fn start_scan_execution(
        &mut self,
        focus: String,
        focus_logs: bool,
        append_logs: bool,
    ) {
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

    /// Start execution of the task builder agent.
    pub(crate) fn start_task_builder_execution(&mut self, request: String) {
        self.logs.clear();
        self.logs
            .push(format!("=== Building task from: {} ===", request));
        self.log_scroll = 0;
        self.autoscroll = true;
        self.set_status_message("Starting task builder...");

        self.runner_active = true;
        self.running_task_id = Some("Task Builder".to_string());
        self.running_kind = Some(RunningKind::TaskBuilder);
        self.mode = AppMode::Executing {
            task_id: "Task Builder".to_string(),
        };
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

    pub(crate) fn rebuild_filtered_view_with_preferred(&mut self, preferred_id: Option<&str>) {
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
    pub(crate) fn reload_queues_from_disk(&mut self, queue_path: &Path, done_path: &Path) {
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

    /// Handle scan completion: reload queue, set status, and return to normal mode.
    pub(crate) fn on_scan_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Scan completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle task builder completion: reload queue, set status, and return to normal mode.
    pub(crate) fn on_task_builder_finished(&mut self, queue_path: &Path, done_path: &Path) {
        self.reload_queues_from_disk(queue_path, done_path);
        self.set_status_message("Task builder completed");
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle scan error: set error message and return to normal mode.
    pub(crate) fn on_scan_error(&mut self, msg: &str) {
        self.set_status_message(format!("Scan error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }

    /// Handle task builder error: set error message and return to normal mode.
    pub(crate) fn on_task_builder_error(&mut self, msg: &str) {
        self.set_status_message(format!("Task builder error: {}", msg));
        if matches!(self.mode, AppMode::Executing { .. } | AppMode::ConfirmQuit) {
            self.mode = AppMode::Normal;
        }
    }
}

pub(crate) fn auto_save_if_dirty(
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
            Some(path) => match crate_config::save_layer(path, &app.project_config) {
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
        allow_proceed: bool,
        reply: mpsc::Sender<runutil::RevertDecision>,
    },
}

/// Run the TUI application with an active queue lock.
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

        let revert_prompt: runutil::RevertPromptHandler =
            Arc::new(move |context: &runutil::RevertPromptContext| {
                let (reply_tx, reply_rx) = mpsc::channel();
                if tx_clone_for_prompt
                    .send(RunnerEvent::RevertPrompt {
                        label: context.label.clone(),
                        allow_proceed: context.allow_proceed,
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

    // Helper to spawn task builder work.
    let spawn_task_builder = |request: String, tx: mpsc::Sender<RunnerEvent>| {
        let tx_clone = tx.clone();
        thread::spawn(move || {
            let result = || -> Result<()> {
                let resolved = crate_config::resolve_from_cwd()?;
                let runner = resolved.config.agent.runner.unwrap_or_default();
                let model = resolved
                    .config
                    .agent
                    .model
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();
                let reasoning_effort = resolved.config.agent.reasoning_effort;
                let repoprompt_tool_injection =
                    crate::agent::resolve_repoprompt_flags(false, false, &resolved).tool_injection;
                let opts = crate::task_cmd::TaskBuildOptions {
                    request,
                    hint_tags: String::new(),
                    hint_scope: String::new(),
                    runner,
                    model,
                    reasoning_effort,
                    force: false,
                    repoprompt_tool_injection,
                };
                crate::task_cmd::build_task_without_lock(&resolved, opts)?;
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
                        app_ref.append_log_lines(text.lines().map(|line| line.to_string()));
                    }
                    RunnerEvent::Finished => {
                        app_ref.runner_active = false;
                        app_ref.running_task_id = None;
                        let running_kind = app_ref.running_kind.take();

                        match running_kind {
                            Some(RunningKind::Scan { .. }) => {
                                app_ref.on_scan_finished(queue_path, done_path);
                            }
                            Some(RunningKind::TaskBuilder) => {
                                app_ref.on_task_builder_finished(queue_path, done_path);
                            }
                            Some(RunningKind::Task) | None => {
                                app_ref.reload_queues_from_disk(queue_path, done_path);

                                if app_ref.mode == AppMode::ConfirmQuit {
                                    app_ref.mode = AppMode::Normal;
                                }

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
                                app_ref.on_scan_error(&msg);
                            }
                            Some(RunningKind::TaskBuilder) => {
                                app_ref.on_task_builder_error(&msg);
                            }
                            Some(RunningKind::Task) | None => {
                                app_ref.set_runner_error(&msg);
                                if matches!(
                                    app_ref.mode,
                                    AppMode::Executing { .. } | AppMode::ConfirmQuit
                                ) {
                                    app_ref.mode = AppMode::Normal;
                                }
                            }
                        }
                    }
                    RunnerEvent::RevertPrompt {
                        label,
                        allow_proceed,
                        reply,
                    } => {
                        let previous_mode = app_ref.mode.clone();
                        app_ref.mode = AppMode::ConfirmRevert {
                            label,
                            allow_proceed,
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
                        TuiAction::BuildTask(request) => {
                            if app_ref.runner_active {
                                app_ref.set_status_message("Runner already active");
                            } else {
                                app_ref.start_task_builder_execution(request.clone());
                                let tx_clone = tx.clone();
                                spawn_task_builder(request, tx_clone);
                            }
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
pub fn prepare_tui_session(
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
            project_config = crate_config::load_layer(path)
                .with_context(|| format!("load project config {}", path.display()))?;
        }
    }
    app.project_config = project_config;
    app.project_config_path = project_config_path;

    Ok((app, lock))
}

pub(crate) fn scan_label(focus: &str) -> String {
    let trimmed = focus.trim();
    if trimmed.is_empty() {
        "scan: (all)".to_string()
    } else {
        format!("scan: {}", trimmed)
    }
}
