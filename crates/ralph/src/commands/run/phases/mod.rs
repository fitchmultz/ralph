//! Phase-specific execution logic for `ralph run`.
//!
//! This module isolates multi-phase runner workflows (planning, implementation,
//! code review) from higher-level orchestration in `crate::commands::run`.

use std::cell::RefCell;

use crate::commands::run::supervision::PushPolicy;
use crate::config;
use crate::contracts::{GitRevertMode, ProjectType, Runner};
use crate::{promptflow, runner, runutil};

// Re-export execution timing types for use by phase implementations
pub(crate) use crate::commands::run::execution_timings::RunExecutionTimings;

mod phase1;
mod phase2;
pub(crate) mod phase3;
mod shared;
mod single;

#[cfg(test)]
mod tests;

pub use phase1::execute_phase1_planning;
pub use phase2::execute_phase2_implementation;
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

/// Defines how post-run supervision should behave.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostRunMode {
    /// Standard post-run supervision (queue/done updates + git finalization).
    Normal,
    /// Parallel worker supervision (no queue/done mutations; completion signals only).
    ParallelWorker,
}

/// Shared inputs for executing a run phase workflow.
///
/// This struct intentionally groups parameters to keep function signatures small and
/// avoid clippy `too_many_arguments`, while preserving exact behaviors from
/// `crate::commands::run`.
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
    pub push_policy: PushPolicy,
    pub revert_prompt: Option<runutil::RevertPromptHandler>,
    pub iteration_context: &'a str,
    pub iteration_completion_block: &'a str,
    pub phase3_completion_guidance: &'a str,
    pub is_final_iteration: bool,
    pub allow_dirty_repo: bool,
    pub post_run_mode: PostRunMode,
    /// Notification override from CLI (--notify/--no-notify).
    pub notify_on_complete: Option<bool>,
    /// Sound notification override from CLI (--notify-sound).
    pub notify_sound: Option<bool>,
    /// Enable strict LFS validation before commit.
    pub lfs_check: bool,
    /// Disable progress indicators and celebrations (--no-progress).
    pub no_progress: bool,
    /// Optional execution timings accumulator for recording phase durations.
    pub execution_timings: Option<&'a RefCell<RunExecutionTimings>>,
    /// Optional plugin registry for processor hook invocation.
    pub plugins: Option<&'a crate::plugins::registry::PluginRegistry>,
}

/// Generate a unique session ID for runner session resumption.
///
/// Format: <task_id>-p<phase>-<timestamp>
/// Example: RQ-0001-p2-1704153600
pub(crate) fn generate_phase_session_id(task_id: &str, phase: u8) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}-p{}-{}", task_id, phase, timestamp)
}

/// Build a phase session ID only for runners that require Ralph-managed IDs.
///
/// Kimi does not emit session IDs in its JSON output, so Ralph must supply one.
pub(crate) fn phase_session_id_for_runner(
    runner: Runner,
    task_id: &str,
    phase: u8,
) -> Option<String> {
    match runner {
        Runner::Kimi => Some(generate_phase_session_id(task_id, phase)),
        _ => None,
    }
}
