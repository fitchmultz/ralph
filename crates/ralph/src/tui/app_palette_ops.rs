//! Palette command execution for the TUI.
//!
//! Responsibilities:
//! - Execute palette commands and dispatch to appropriate handlers
//! - Coordinate between palette UI and app operations
//! - Handle command-specific validation and setup
//!
//! Not handled here:
//! - Palette entry building/filtering (see app_palette module)
//! - UI rendering of palette (see render module)
//! - Key event handling (see events module)
//!
//! Invariants/assumptions:
//! - Commands are validated before execution
//! - Runner state is checked before spawning new tasks
//! - Loop mode coordination happens here for run commands

use anyhow::Result;

use crate::tui::events::{PaletteCommand, TuiAction};

/// Trait for palette command execution.
pub trait PaletteOperations {
    /// Execute a palette command (also used by direct keybinds for consistency).
    fn execute_palette_command(
        &mut self,
        cmd: PaletteCommand,
        now_rfc3339: &str,
    ) -> Result<TuiAction>;
}

use crate::contracts::TaskStatus;
use crate::tui::TextInput;
use crate::tui::app::App;
use crate::tui::app_filters::{FilterManagementOperations, FilterOperations};
use crate::tui::app_multi_select::MultiSelectOperations;
use crate::tui::app_tasks::TaskMovementOperations;
use crate::tui::events::{AppMode, ConfirmDiscardAction};

// Implementation for App
impl PaletteOperations for App {
    fn execute_palette_command(
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
                self.mode = AppMode::CreatingTask(TextInput::new(""));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BuildTaskAgent => {
                if self.runner_active {
                    self.set_status_message("Runner already active");
                } else {
                    self.start_task_builder_options_flow();
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
                    self.mode = AppMode::Scanning(TextInput::new(""));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::Search => {
                self.start_search_input();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterTags => {
                self.start_filter_tags_input();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::FilterScopes => {
                self.start_filter_scopes_input();
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
            PaletteCommand::SetStatusDraft => {
                self.set_task_status("draft", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusTodo => {
                self.set_task_status("todo", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusDoing => {
                self.set_task_status("doing", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusDone => {
                self.set_task_status("done", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetStatusRejected => {
                self.set_task_status("rejected", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityCritical => {
                self.set_task_priority("critical", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityHigh => {
                self.set_task_priority("high", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityMedium => {
                self.set_task_priority("medium", now_rfc3339);
                Ok(TuiAction::Continue)
            }
            PaletteCommand::SetPriorityLow => {
                self.set_task_priority("low", now_rfc3339);
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
            PaletteCommand::ToggleFuzzy => {
                self.toggle_fuzzy();
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ReloadQueue => {
                if self.unsafe_to_discard() {
                    self.mode = AppMode::ConfirmDiscard {
                        action: ConfirmDiscardAction::ReloadQueue,
                    };
                    Ok(TuiAction::Continue)
                } else {
                    Ok(TuiAction::ReloadQueue)
                }
            }
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
            PaletteCommand::JumpToTask => {
                self.mode = AppMode::JumpingToTask(TextInput::new(""));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::RepairQueue => {
                self.mode = AppMode::ConfirmRepair { dry_run: false };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::RepairQueueDryRun => {
                self.mode = AppMode::ConfirmRepair { dry_run: true };
                Ok(TuiAction::Continue)
            }
            PaletteCommand::UnlockQueue => {
                self.mode = AppMode::ConfirmUnlock;
                Ok(TuiAction::Continue)
            }
            PaletteCommand::Quit => {
                if self.runner_active {
                    self.mode = AppMode::ConfirmQuit;
                    Ok(TuiAction::Continue)
                } else if self.unsafe_to_discard() {
                    self.mode = AppMode::ConfirmDiscard {
                        action: ConfirmDiscardAction::Quit,
                    };
                    Ok(TuiAction::Continue)
                } else {
                    Ok(TuiAction::Quit)
                }
            }
            PaletteCommand::ToggleMultiSelectMode => {
                self.toggle_multi_select_mode();
                if self.multi_select_mode {
                    self.set_status_message(
                        "Multi-select mode ON. Space: toggle, m: exit, a: archive, d: delete",
                    );
                } else {
                    self.set_status_message("Multi-select mode OFF");
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ToggleTaskSelection => {
                self.toggle_current_selection();
                let count = self.selection_count();
                self.set_status_message(format!("{} tasks selected", count));
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchDelete => {
                let count = self.selection_count();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    self.mode = AppMode::ConfirmBatchDelete { count };
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchArchive => {
                let count = self.selection_count();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    self.mode = AppMode::ConfirmBatchArchive { count };
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::BatchSetStatus(status) => {
                let indices: Vec<usize> = self.selected_indices.iter().copied().collect();
                let count = indices.len();
                if count == 0 {
                    self.set_status_message("No tasks selected");
                } else {
                    let queue_indices: Vec<usize> = indices
                        .iter()
                        .filter_map(|&filtered_idx| {
                            self.filtered_indices.get(filtered_idx).copied()
                        })
                        .collect();
                    for idx in queue_indices {
                        if let Some(task) = self.queue.tasks.get_mut(idx) {
                            task.status = status;
                            task.updated_at = Some(now_rfc3339.to_string());
                        }
                    }
                    self.dirty = true;
                    self.bump_queue_rev();
                    self.set_status_message(format!(
                        "Set status to {:?} for {} tasks",
                        status, count
                    ));
                }
                Ok(TuiAction::Continue)
            }
            PaletteCommand::ClearSelection => {
                self.clear_selection();
                self.set_status_message("Selection cleared");
                Ok(TuiAction::Continue)
            }
            PaletteCommand::OpenScopeInEditor => {
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };

                if task.scope.is_empty() {
                    self.set_status_message("Selected task has no scope paths");
                    return Ok(TuiAction::Continue);
                }

                Ok(TuiAction::OpenScopeInEditor(task.scope.clone()))
            }
            PaletteCommand::CopyFileLineRef => {
                let Some(task) = self.selected_task() else {
                    self.set_status_message("No task selected");
                    return Ok(TuiAction::Continue);
                };

                let refs = crate::tui::file_line_refs::extract_file_line_refs(
                    task.notes
                        .iter()
                        .chain(task.evidence.iter())
                        .map(|s| s.as_str()),
                );

                if refs.is_empty() {
                    self.set_status_message("No file:line references found in notes/evidence");
                    return Ok(TuiAction::Continue);
                }

                let text = crate::tui::file_line_refs::format_refs_for_clipboard(&refs);
                Ok(TuiAction::CopyToClipboard(text))
            }
        }
    }
}
