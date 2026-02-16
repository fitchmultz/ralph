//! Continue session support for supervision.
//!
//! Responsibilities:
//! - Define ContinueSession struct for resuming runner sessions.
//! - Define CiContinueContext for CI gate resume callbacks.
//! - Implement resume_continue_session to resume a runner with a message.
//!
//! Not handled here:
//! - CI gate logic (see ci.rs).
//! - Queue operations (see queue_ops.rs).
//!
//! Invariants/assumptions:
//! - ContinueSession.session_id must be Some for resume to succeed.
//! - Runner CLI options and phase type are preserved from the original session.

use crate::commands::run::PhaseType;
use anyhow::{Context, Result};

/// Session state for continuing an interrupted task.
#[derive(Clone)]
pub(crate) struct ContinueSession {
    pub runner: crate::contracts::Runner,
    pub model: crate::contracts::Model,
    pub reasoning_effort: Option<crate::contracts::ReasoningEffort>,
    /// The runner CLI settings resolved for the run that created this continue session.
    /// These must be preserved to avoid losing CLI overrides / task-specific settings.
    pub runner_cli: crate::runner::ResolvedRunnerCliOptions,
    /// The phase that created this continue session. Must be preserved so phase-aware
    /// runners (e.g., Cursor) behave correctly on Continue.
    pub phase_type: PhaseType,
    pub session_id: Option<String>,
    pub output_handler: Option<crate::runner::OutputHandler>,
    pub output_stream: crate::runner::OutputStream,
    /// Number of automatic "fix CI and rerun" retries already sent for the current CI gate loop.
    /// Used to auto-enforce CI compliance without prompting for the first N failures.
    pub ci_failure_retry_count: u8,
    /// The task ID for this continue session (needed for processor hooks).
    pub task_id: String,
}

/// Context for resuming a runner session during a post-run CI gate failure.
pub(crate) struct CiContinueContext<'a> {
    pub continue_session: &'a mut ContinueSession,
    /// Callback invoked after each resume, receiving both the output and the elapsed duration.
    /// The duration represents the wall-clock time spent in that specific resume session.
    pub on_resume:
        &'a mut dyn FnMut(&crate::runner::RunnerOutput, std::time::Duration) -> Result<()>,
}

/// Resume a continue session with a message.
///
/// Returns the runner output along with the wall-clock duration of the session.
/// The duration is measured from the start of the function to when the runner
/// output is received.
///
/// Invokes post_run processor hooks after successful resume if plugins are provided.
pub(crate) fn resume_continue_session(
    resolved: &crate::config::Resolved,
    session: &mut ContinueSession,
    message: &str,
    plugins: Option<&crate::plugins::registry::PluginRegistry>,
) -> Result<(crate::runner::RunnerOutput, std::time::Duration)> {
    let start = std::time::Instant::now();
    let session_id = session
        .session_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Catastrophic: no session id captured; cannot Continue."))?;
    let bins = crate::runner::resolve_binaries(&resolved.config.agent);
    // Use the stored runner_cli and phase_type from the session to preserve
    // CLI overrides and ensure phase-correct behavior for phase-aware runners.
    let output = crate::runner::resume_session(
        session.runner.clone(),
        &resolved.repo_root,
        bins,
        session.model.clone(),
        session.reasoning_effort,
        session.runner_cli,
        session_id,
        message,
        resolved.config.agent.claude_permission_mode,
        None,
        session.output_handler.clone(),
        session.output_stream,
        session.phase_type,
        plugins,
    )?;
    let elapsed = start.elapsed();
    if let Some(new_id) = output.session_id.as_ref() {
        session.session_id = Some(new_id.clone());
    }

    // Invoke post_run hooks after successful resume
    if let Some(registry) = plugins {
        let exec = crate::plugins::processor_executor::ProcessorExecutor::new(
            &resolved.repo_root,
            registry,
        );
        exec.post_run(&session.task_id, &output.stdout)
            .with_context(|| "processor post_run hook failed after resume")?;
    }

    Ok((output, elapsed))
}
