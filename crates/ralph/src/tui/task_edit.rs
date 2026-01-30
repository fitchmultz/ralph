//! TUI task editing helpers and entry definitions.
//!
//! Responsibilities:
//! - Define the editable task fields and how they are presented in the TUI.
//! - Apply edits to the in-memory queue and refresh selection state.
//!
//! Not handled here:
//! - Queue persistence, locking, or filesystem IO.
//! - Input handling or terminal rendering.
//!
//! Invariants/assumptions:
//! - A queue lock is held by callers when mutating tasks.
//! - App selection points at a valid task when edits are applied.

use super::app::App;
use crate::outpututil::format_custom_fields;
use crate::queue::{self, TaskEditKey};
use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskEditKind {
    Cycle,
    Text,
    List,
    Map,
    OptionalText,
}

#[derive(Debug, Clone)]
pub struct TaskEditEntry {
    pub key: TaskEditKey,
    pub label: &'static str,
    pub value: String,
    pub kind: TaskEditKind,
}

impl App {
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
            TaskEditEntry {
                key: TaskEditKey::ScheduledStart,
                label: "scheduled_start",
                value: display_optional(task.scheduled_start.as_deref()),
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
            // List fields: use newline for multi-line editing
            TaskEditKey::Tags => task.tags.join("\n"),
            TaskEditKey::Scope => task.scope.join("\n"),
            TaskEditKey::Evidence => task.evidence.join("\n"),
            TaskEditKey::Plan => task.plan.join("\n"),
            TaskEditKey::Notes => task.notes.join("\n"),
            TaskEditKey::Request => task.request.clone().unwrap_or_default(),
            TaskEditKey::DependsOn => task.depends_on.join("\n"),
            TaskEditKey::Blocks => task.blocks.join("\n"),
            TaskEditKey::RelatesTo => task.relates_to.join("\n"),
            TaskEditKey::Duplicates => task.duplicates.clone().unwrap_or_default(),
            TaskEditKey::CustomFields => format_custom_fields(&task.custom_fields, ""),
            TaskEditKey::CreatedAt => task.created_at.clone().unwrap_or_default(),
            TaskEditKey::UpdatedAt => task.updated_at.clone().unwrap_or_default(),
            TaskEditKey::CompletedAt => task.completed_at.clone().unwrap_or_default(),
            TaskEditKey::ScheduledStart => task.scheduled_start.clone().unwrap_or_default(),
            TaskEditKey::Status | TaskEditKey::Priority => String::new(),
        }
    }

    /// Check if a task edit key represents a list field.
    pub(crate) fn is_list_field(&self, key: TaskEditKey) -> bool {
        matches!(
            key,
            TaskEditKey::Tags
                | TaskEditKey::Scope
                | TaskEditKey::Evidence
                | TaskEditKey::Plan
                | TaskEditKey::Notes
                | TaskEditKey::DependsOn
                | TaskEditKey::Blocks
                | TaskEditKey::RelatesTo
        )
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

        queue::apply_task_edit(
            &mut self.queue,
            Some(&self.done),
            &task_id,
            key,
            input,
            now_rfc3339,
            &self.id_prefix,
            self.id_width,
            self.max_dependency_depth,
        )?;

        self.dirty = true;
        self.bump_queue_rev();
        self.set_status_message(format!("Updated {}", task_id));
        self.rebuild_filtered_view_with_preferred(Some(&task_id));
        Ok(())
    }

    /// Cycle the status of the selected task.
    pub fn cycle_status(&mut self, now_rfc3339: &str) -> Result<()> {
        self.apply_task_edit(TaskEditKey::Status, "", now_rfc3339)
    }

    /// Cycle the priority of the selected task.
    pub fn cycle_priority(&mut self, now_rfc3339: &str) -> Result<()> {
        self.apply_task_edit(TaskEditKey::Priority, "", now_rfc3339)
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "(empty)".to_string()
    } else {
        values.join(", ")
    }
}

fn display_optional(value: Option<&str>) -> String {
    match value {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => "(empty)".to_string(),
    }
}
