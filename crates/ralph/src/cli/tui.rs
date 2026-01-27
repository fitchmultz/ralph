//! `ralph tui` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap arguments for launching the TUI.
//! - Route to TUI setup with resolved agent overrides.
//!
//! Not handled here:
//! - TUI rendering/event loops (see `crate::tui`).
//! - Queue persistence or locking semantics.
//! - Runner execution details.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - RepoPrompt mode selection (if any) is already normalized.

use anyhow::{anyhow, Result};
use clap::Args;

use crate::cli::interactive;
use crate::{agent, config, runner, runutil, tui};

#[derive(Args)]
#[command(
    about = "Launch the interactive TUI (queue management + execution + loop)",
    after_long_help = "Notes:\n\
 - `ralph tui` is the primary interactive UI entry point.\n\
 - By default, execution is enabled (press Enter to run the selected task).\n\
 - Use `--read-only` to disable execution.\n\
 - `ralph run one -i` and `ralph run loop -i` launch the same TUI for compatibility.\n\
\n\
Examples:\n\
 ralph tui\n\
 ralph tui --read-only\n\
 ralph tui --runner codex --model gpt-5.2-codex --effort high\n\
 ralph tui --runner claude --model opus\n\
 ralph tui --runner opencode --model gpt-5.2\n"
)]
pub struct TuiArgs {
    /// Disable task execution (browse/edit only).
    #[arg(long)]
    pub read_only: bool,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

pub fn handle_tui(args: TuiArgs, force_lock: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    if args.read_only {
        let runner_factory = browse_only_runner;
        let scan_factory = browse_only_scan_runner;
        let _ = tui::run_tui(
            &resolved,
            force_lock,
            tui::TuiOptions::default(),
            runner_factory,
            scan_factory,
        )?;
        return Ok(());
    }

    let overrides = agent::resolve_run_agent_overrides(&args.agent)?;
    let factories = interactive::build_interactive_factories(
        &resolved,
        &overrides,
        args.agent.repo_prompt,
        force_lock,
    )?;

    let _ = tui::run_tui(
        &resolved,
        force_lock,
        tui::TuiOptions::default(),
        factories.runner_factory,
        factories.scan_factory,
    )?;
    Ok(())
}

fn browse_only_runner(
    _task_id: String,
    _handler: runner::OutputHandler,
    _revert_prompt: runutil::RevertPromptHandler,
) -> impl FnOnce() -> Result<()> + Send {
    move || {
        Err(anyhow!(
            "Task execution is disabled in read-only mode. Re-run without `--read-only`."
        ))
    }
}

fn browse_only_scan_runner(
    _focus: String,
    _handler: runner::OutputHandler,
    _revert_prompt: runutil::RevertPromptHandler,
) -> impl FnOnce() -> Result<()> + Send {
    move || {
        Err(anyhow!(
            "Scan is disabled in read-only mode. Re-run without `--read-only`."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn browse_only_runner_rejects_execution() {
        let handler: runner::OutputHandler = Arc::new(Box::new(|_text: &str| {}));
        let revert_prompt: runutil::RevertPromptHandler =
            Arc::new(|_context: &runutil::RevertPromptContext| runutil::RevertDecision::Keep);
        let runner = browse_only_runner("RQ-0001".to_string(), handler, revert_prompt);
        let err = runner().expect_err("expected browse-only error");
        assert!(err
            .to_string()
            .contains("Task execution is disabled in read-only mode"));
    }

    #[test]
    fn browse_only_scan_runner_rejects_scan() {
        let handler: runner::OutputHandler = Arc::new(Box::new(|_text: &str| {}));
        let revert_prompt: runutil::RevertPromptHandler =
            Arc::new(|_context: &runutil::RevertPromptContext| runutil::RevertDecision::Keep);
        let runner = browse_only_scan_runner("".to_string(), handler, revert_prompt);
        let err = runner().expect_err("expected browse-only scan error");
        assert!(err
            .to_string()
            .contains("Scan is disabled in read-only mode"));
    }
}
