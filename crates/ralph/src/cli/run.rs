//! `ralph run ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap structures for run commands and flags.
//! - Route run subcommands to task execution and TUI entry points.
//!
//! Not handled here:
//! - Queue persistence and task status transitions (see `crate::queue`).
//! - Runner implementations or model execution (see `crate::runner`).
//! - Global configuration precedence rules (see `crate::config`).
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Queue mutations occur inside downstream command handlers.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::cli::interactive;
use crate::{agent, commands::run as run_cmd, config, debuglog, tui};

pub fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        RunCommand::One(args) => {
            if args.debug {
                debuglog::enable(&resolved.repo_root)?;
            }
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                let factories = interactive::build_interactive_factories(
                    &resolved,
                    &overrides,
                    args.agent.repo_prompt,
                    force,
                )?;

                // Interactive one: open the TUI (no auto-loop).
                let options = tui::TuiOptions {
                    start_loop: false,
                    loop_max_tasks: None,
                    loop_include_draft: args.agent.include_draft,
                    show_flowchart: args.visualize,
                    no_mouse: false,
                    color: crate::tui::terminal::ColorOption::Auto,
                    ascii_borders: false,
                };
                let _ = tui::run_tui(
                    &resolved,
                    force,
                    options,
                    factories.runner_factory,
                    factories.scan_factory,
                )?;
                Ok(())
            } else {
                if let Some(task_id) = args.id.as_deref() {
                    run_cmd::run_one_with_id(&resolved, &overrides, force, task_id, None, None)?;
                } else {
                    let _ = run_cmd::run_one(&resolved, &overrides, force)?;
                }
                Ok(())
            }
        }
        RunCommand::Loop(args) => {
            if args.debug {
                debuglog::enable(&resolved.repo_root)?;
            }
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                let factories = interactive::build_interactive_factories(
                    &resolved,
                    &overrides,
                    args.agent.repo_prompt,
                    force,
                )?;

                // Interactive loop: auto-start the loop in the TUI to match semantics.
                let max = if args.max_tasks == 0 {
                    None
                } else {
                    Some(args.max_tasks)
                };

                let options = tui::TuiOptions {
                    start_loop: true,
                    loop_max_tasks: max,
                    loop_include_draft: args.agent.include_draft,
                    show_flowchart: args.visualize,
                    no_mouse: false,
                    color: crate::tui::terminal::ColorOption::Auto,
                    ascii_borders: false,
                };

                let _ = tui::run_tui(
                    &resolved,
                    force,
                    options,
                    factories.runner_factory,
                    factories.scan_factory,
                )?;
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
    about = "Run Ralph supervisor (executes queued tasks via codex/opencode/gemini/claude/cursor)",
    after_long_help = "Runner selection:\n\
  - `ralph run` selects runner/model/effort with this precedence:\n\
  1) CLI overrides (flags on `run one` / `run loop`)\n\
  2) task's `agent` override (runner/model plus `model_effort` if set)\n\
  3) otherwise: resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\
 \n\
 Notes:\n\
  - Allowed runners: codex, opencode, gemini, claude, cursor\n\
  - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude/cursor accept arbitrary model ids)\n\
  - `--effort` is codex-only and is ignored for other runners.\n\
  - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\
  - `--git-commit-push-on` / `--git-commit-push-off` control automatic git commit/push after successful runs.\n\
  - `--update-task` runs `ralph task update <TASK_ID>` once per task immediately before task is marked `doing`.\n\
  - Clean-repo checks allow changes to `.ralph/config.json` (plus `.ralph/queue.json` and `.ralph/done.json`); use `--force` to bypass entirely.\n\
  - TUI entrypoints: `ralph tui`, `ralph run one -i`, `ralph run loop -i`.\n\
 \n\
 To change defaults for this repo, edit .ralph/config.json:\n\
  version: 1\n\
  agent:\n\
  runner: claude\n\
  model: sonnet\n\
  gemini_bin: gemini\n\
 \n\
Notes:\n\
 - Allowed runners: codex, opencode, gemini, claude\n\
 - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n\
 - `--effort` is codex-only and is ignored for other runners.\n\
 - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\
 - `--git-commit-push-on` / `--git-commit-push-off` control automatic git commit/push after successful runs.\n\
 - `--update-task` runs `ralph task update <TASK_ID>` once per task immediately before the task is marked `doing`.\n\
 - Clean-repo checks allow changes to `.ralph/config.json` (plus `.ralph/queue.json` and `.ralph/done.json`); use `--force` to bypass entirely.\n\
 - TUI entrypoints: `ralph tui`, `ralph run one -i`, `ralph run loop -i`.\n\
\n\
To change defaults for this repo, edit .ralph/config.json:\n\
 version: 1\n\
 agent:\n\
 runner: claude\n\
 model: sonnet\n\
 gemini_bin: gemini\n\
\n\
Examples:\n\
 ralph run one\n\
 ralph run one --phases 2\n\
 ralph run one --phases 1\n\
 ralph run one --runner opencode --model gpt-5.2\n\
 ralph run one --runner codex --model gpt-5.2-codex --effort high\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode disabled\n\
 ralph run one --git-commit-push-off\n\
 ralph run one --update-task\n\
 ralph run one --lfs-check\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --update-task --max-tasks 1\n\
 ralph run loop --git-revert-mode ask --max-tasks 1\n\
 ralph run loop --git-commit-push-on --max-tasks 1\n\
 ralph run loop --lfs-check --max-tasks 1\n\
 ralph tui\n\
 ralph tui --read-only\n\
 ralph run one -i\n\
 ralph run loop -i"
)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: RunCommand,
}

#[derive(Subcommand)]
pub enum RunCommand {
    #[command(
        about = "Run exactly one task (the first todo in .ralph/queue.json)",
        after_long_help = "Runner selection (precedence):\n\
 1) CLI overrides (--runner/--model/--effort)\n\
 2) task.agent in .ralph/queue.json (if present)\n\
 3) config defaults (.ralph/config.json then ~/.config/ralph/config.json)\n\
\n\
Examples:\n\
 ralph run one\n\
 ralph run one --id RQ-0001\n\
 ralph run one -i\n\
 ralph run one --debug\n\
 ralph run one --phases 3 (plan/implement+CI/review+complete)\n\
 ralph run one --phases 2 (plan/implement)\n\
 ralph run one --phases 1 (single-pass)\n\
 ralph run one --quick (single-pass, same as --phases 1)\n\
 ralph run one --runner opencode --model gpt-5.2\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner codex --model gpt-5.2-codex --effort high\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode enabled\n\
 ralph run one --git-commit-push-off\n\
 ralph run one --update-task\n\
 ralph run one --lfs-check\n\
 ralph run one --repo-prompt plan\n\
 ralph run one --repo-prompt off\n\
 ralph tui"
    )]
    One(RunOneArgs),
    #[command(
        about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)",
        after_long_help = "Examples:\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --phases 3 --max-tasks 0 (plan/implement+CI/review+complete)\n\
 ralph run loop --phases 2 --max-tasks 0 (plan/implement)\n\
 ralph run loop --phases 1 --max-tasks 1 (single-pass)\n\
 ralph run loop --quick --max-tasks 1 (single-pass, same as --phases 1)\n\
 ralph run loop --max-tasks 3\n\
 ralph run loop --max-tasks 1 --debug\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode disabled --max-tasks 1\n\
 ralph run loop --git-commit-push-off --max-tasks 1\n\
 ralph run loop --update-task --max-tasks 1\n\
 ralph run loop --repo-prompt tools --max-tasks 1\n\
 ralph run loop --repo-prompt off --max-tasks 1\n\
 ralph run loop --lfs-check --max-tasks 1\n\
 ralph run loop -i\n\
 ralph tui"
    )]
    Loop(RunLoopArgs),
}

#[derive(Args)]
pub struct RunOneArgs {
    /// Launch interactive TUI mode for task selection and management.
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Capture raw supervisor + runner output to .ralph/logs/debug.log.
    #[arg(long)]
    pub debug: bool,

    /// Run a specific task by ID (non-interactive only).
    #[arg(long, value_name = "TASK_ID", conflicts_with = "interactive")]
    pub id: Option<String>,

    /// Show workflow flowchart visualization on start (interactive only).
    #[arg(long, default_value_t = false)]
    pub visualize: bool,

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

    /// Capture raw supervisor + runner output to .ralph/logs/debug.log.
    #[arg(long)]
    pub debug: bool,

    /// Show workflow flowchart visualization on start (interactive only).
    #[arg(long, default_value_t = false)]
    pub visualize: bool,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use crate::cli::Cli;

    #[test]
    fn run_one_help_includes_phase_semantics() {
        let mut cmd = Cli::command();
        let run = cmd.find_subcommand_mut("run").expect("run subcommand");
        let run_one = run.find_subcommand_mut("one").expect("run one subcommand");
        let help = run_one.render_long_help().to_string();

        assert!(
            help.contains("ralph run one --phases 3 (plan/implement+CI/review+complete)"),
            "missing phases=3 example: {help}"
        );
        assert!(
            help.contains("ralph run one --phases 2 (plan/implement)"),
            "missing phases=2 example: {help}"
        );
        assert!(
            help.contains("ralph run one --phases 1 (single-pass)"),
            "missing phases=1 example: {help}"
        );
        assert!(
            help.contains("ralph run one --quick (single-pass, same as --phases 1)"),
            "missing --quick example: {help}"
        );
    }

    #[test]
    fn run_loop_help_mentions_repo_prompt_examples() {
        let mut cmd = Cli::command();
        let run = cmd.find_subcommand_mut("run").expect("run subcommand");
        let run_loop = run
            .find_subcommand_mut("loop")
            .expect("run loop subcommand");
        let help = run_loop.render_long_help().to_string();

        assert!(
            help.contains("ralph run loop --repo-prompt tools --max-tasks 1"),
            "missing repo-prompt tools example: {help}"
        );
        assert!(
            help.contains("ralph run loop --repo-prompt off --max-tasks 1"),
            "missing repo-prompt off example: {help}"
        );
    }
}
