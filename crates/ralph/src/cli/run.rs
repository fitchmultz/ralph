//! `ralph run ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{agent, config, run_cmd, runner, tui};

pub fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        RunCommand::One(args) => {
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory = move |task_id: String, handler: runner::OutputHandler| {
                    let resolved = resolved_clone.clone();
                    let overrides = overrides.clone();
                    let force = force;
                    move || {
                        run_cmd::run_one_with_id(
                            &resolved,
                            &overrides,
                            force,
                            &task_id,
                            Some(handler),
                        )
                    }
                };
                // Tasks are executed within TUI, run_tui returns None
                let _ = tui::run_tui(&resolved.queue_path, runner_factory)?;
                Ok(())
            } else {
                if let Some(task_id) = args.id.as_deref() {
                    run_cmd::run_one_with_id(&resolved, &overrides, force, task_id, None)?;
                } else {
                    let _ = run_cmd::run_one(&resolved, &overrides, force)?;
                }
                Ok(())
            }
        }
        RunCommand::Loop(args) => {
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory = move |task_id: String, handler: runner::OutputHandler| {
                    let resolved = resolved_clone.clone();
                    let overrides = overrides.clone();
                    let force = force;
                    move || {
                        run_cmd::run_one_with_id(
                            &resolved,
                            &overrides,
                            force,
                            &task_id,
                            Some(handler),
                        )
                    }
                };
                // Tasks are executed within TUI, run_tui returns None
                let _ = tui::run_tui(&resolved.queue_path, runner_factory)?;
                Ok(())
            } else {
                run_cmd::run_loop(
                    &resolved,
                    run_cmd::RunLoopOptions {
                        max_tasks: args.max_tasks,
                        agent_overrides: overrides,
                        force,
                    },
                )
            }
        }
    }
}

#[derive(Args)]
#[command(
    about = "Run the Ralph supervisor (executes queued tasks via codex/opencode/gemini/claude)",
    after_long_help = "Runner selection:\n  - `ralph run` selects runner/model/effort with this precedence:\n      1) CLI overrides (flags on `run one` / `run loop`)\n      2) the task's `agent` override (if present in .ralph/queue.json)\n      3) otherwise the resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\nNotes:\n  - Allowed runners: codex, opencode, gemini, claude\n  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n  - `--effort` is codex-only and is ignored for other runners.\n  - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\nTo change defaults for this repo, edit .ralph/config.json:\n  version: 1\n  agent:\n    runner: claude\n    model: sonnet\n    gemini_bin: gemini\n\nExamples:\n  ralph run one\n  ralph run one --phases 2\n  ralph run one --phases 1\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run one --git-revert-mode disabled\n  ralph run loop --max-tasks 0\n  ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n  ralph run loop --git-revert-mode ask --max-tasks 1"
)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: RunCommand,
}

#[derive(Subcommand)]
pub enum RunCommand {
    #[command(
        about = "Run exactly one task (the first todo in .ralph/queue.json)",
        after_long_help = "Runner selection (precedence):\n  1) CLI overrides (--runner/--model/--effort)\n  2) task.agent in .ralph/queue.json (if present)\n  3) config defaults (.ralph/config.json then ~/.config/ralph/config.json)\n\nExamples:\n  ralph run one\n  ralph run one --id RQ-0001\n  ralph run one -i\n  ralph run one --phases 3\n  ralph run one --phases 2\n  ralph run one --phases 1\n  ralph run one --runner opencode --model gpt-5.2\n  ralph run one --runner gemini --model gemini-3-flash-preview\n  ralph run one --runner codex --model gpt-5.2-codex --effort high\n  ralph run one --git-revert-mode enabled\n  ralph run one --rp-on\n  ralph run one --rp-off"
    )]
    One(RunOneArgs),
    #[command(
        about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)",
        after_long_help = "Examples:\n  ralph run loop --max-tasks 0\n  ralph run loop --phases 3 --max-tasks 0\n  ralph run loop --phases 2 --max-tasks 0\n  ralph run loop --phases 1 --max-tasks 1\n  ralph run loop --max-tasks 3\n  ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n  ralph run loop --git-revert-mode disabled --max-tasks 1\n  ralph run loop -i\n  ralph run loop --rp-on\n  ralph run loop --rp-off"
    )]
    Loop(RunLoopArgs),
}

#[derive(Args)]
pub struct RunOneArgs {
    /// Launch interactive TUI mode for task selection and management.
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Run a specific task by ID (non-interactive only).
    #[arg(long, value_name = "TASK_ID", conflicts_with = "interactive")]
    pub id: Option<String>,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct RunLoopArgs {
    /// Maximum tasks to run before stopping (0 = no limit).
    #[arg(long, default_value_t = 0)]
    pub max_tasks: u32,

    /// Launch interactive TUI mode for task selection and management.
    #[arg(short = 'i', long)]
    pub interactive: bool,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}
