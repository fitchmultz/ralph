//! Phase-specific execution logic for `ralph run`.
//!
//! This module isolates multi-phase runner workflows (planning, implementation,
//! code review) from higher-level orchestration in `crate::commands::run`.

use crate::config;
use crate::contracts::{GitRevertMode, ProjectType};
use crate::{promptflow, runner, runutil};

mod phase1;
mod phase2;
mod phase3;
mod shared;
mod single;

#[cfg(test)]
mod tests;

pub use phase1::execute_phase1_planning;
pub use phase2::execute_phase2_implementation;
pub(crate) use phase3::finalize_phase3_if_done;
pub use phase3::{apply_phase3_completion_signal, execute_phase3_review};
pub use single::execute_single_phase;

/// Represents the type of phase being executed.
///
/// This enum provides explicit phase metadata to runners that need
/// phase-aware behavior (e.g., Cursor uses different sandbox/plan
/// settings for planning vs implementation phases).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseType {
    /// Phase 1: Planning - agent produces implementation plan
    Planning,
    /// Phase 2: Implementation - agent implements the plan
    Implementation,
    /// Phase 3: Review - agent reviews completed work
    Review,
    /// Single phase execution (combines planning and implementation)
    SinglePhase,
}

/// Shared inputs for executing a run phase workflow.
///
/// This struct intentionally groups parameters to keep function signatures small and
/// avoid clippy `too_many_arguments`, while preserving exact behaviors from
/// `crate::commands::run`.
#[derive(Clone)]
pub struct PhaseInvocation<'a> {
    pub resolved: &'a config::Resolved,
    pub settings: &'a runner::AgentSettings,
    pub bins: runner::RunnerBinaries<'a>,
    pub task_id: &'a str,
    pub base_prompt: &'a str,
    pub policy: &'a promptflow::PromptPolicy,
    pub output_handler: Option<runner::OutputHandler>,
    pub output_stream: runner::OutputStream,
    pub project_type: ProjectType,
    pub git_revert_mode: GitRevertMode,
    pub git_commit_push_enabled: bool,
    pub revert_prompt: Option<runutil::RevertPromptHandler>,
    pub iteration_context: &'a str,
    pub iteration_completion_block: &'a str,
    pub phase3_completion_guidance: &'a str,
    pub is_final_iteration: bool,
    pub allow_dirty_repo: bool,
    /// Notification override from CLI (--notify/--no-notify).
    pub notify_on_complete: Option<bool>,
    /// Sound notification override from CLI (--notify-sound).
    pub notify_sound: Option<bool>,
}
