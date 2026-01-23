//! `ralph run ...` command group: Clap types and handler.

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{agent, config, run_cmd, runner, runutil, scan_cmd, tui};

pub fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match cmd {
        RunCommand::One(args) => {
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                let scan_settings = runner::resolve_agent_settings(
                    overrides.runner,
                    overrides.model.clone(),
                    overrides.reasoning_effort,
                    None,
                    &resolved.config.agent,
                )?;
                let scan_repoprompt_required =
                    agent::resolve_rp_required(args.agent.rp_on, args.agent.rp_off, &resolved);
                let scan_git_revert_mode = overrides
                    .git_revert_mode
                    .or(resolved.config.agent.git_revert_mode)
                    .unwrap_or(crate::contracts::GitRevertMode::Ask);

                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory =
                    move |task_id: String,
                          handler: runner::OutputHandler,
                          revert_prompt: runutil::RevertPromptHandler| {
                        let resolved = resolved_clone.clone();
                        let overrides = overrides.clone();
                        let force = force;
                        move || {
                            run_cmd::run_one_with_id_locked(
                                &resolved,
                                &overrides,
                                force,
                                &task_id,
                                Some(handler),
                                Some(revert_prompt),
                            )
                        }
                    };
                let resolved_scan = resolved.clone();
                let scan_factory =
                    move |focus: String,
                          handler: runner::OutputHandler,
                          revert_prompt: runutil::RevertPromptHandler| {
                        let resolved = resolved_scan.clone();
                        let settings = scan_settings.clone();
                        let force = force;
                        let repoprompt_required = scan_repoprompt_required;
                        let git_revert_mode = scan_git_revert_mode;
                        move || {
                            scan_cmd::run_scan(
                                &resolved,
                                scan_cmd::ScanOptions {
                                    focus,
                                    runner: settings.runner,
                                    model: settings.model,
                                    reasoning_effort: settings.reasoning_effort,
                                    force,
                                    repoprompt_required,
                                    git_revert_mode,
                                    lock_mode: scan_cmd::ScanLockMode::Held,
                                    output_handler: Some(handler),
                                    revert_prompt: Some(revert_prompt),
                                },
                            )
                        }
                    };

                // Interactive one: open the TUI (no auto-loop).
                let _ = tui::run_tui(
                    &resolved,
                    force,
                    tui::TuiOptions::default(),
                    runner_factory,
                    scan_factory,
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
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.interactive {
                let scan_settings = runner::resolve_agent_settings(
                    overrides.runner,
                    overrides.model.clone(),
                    overrides.reasoning_effort,
                    None,
                    &resolved.config.agent,
                )?;
                let scan_repoprompt_required =
                    agent::resolve_rp_required(args.agent.rp_on, args.agent.rp_off, &resolved);
                let scan_git_revert_mode = overrides
                    .git_revert_mode
                    .or(resolved.config.agent.git_revert_mode)
                    .unwrap_or(crate::contracts::GitRevertMode::Ask);

                // Capture the values we need by moving them into the factory
                let resolved_clone = resolved.clone();
                let runner_factory =
                    move |task_id: String,
                          handler: runner::OutputHandler,
                          revert_prompt: runutil::RevertPromptHandler| {
                        let resolved = resolved_clone.clone();
                        let overrides = overrides.clone();
                        let force = force;
                        move || {
                            run_cmd::run_one_with_id_locked(
                                &resolved,
                                &overrides,
                                force,
                                &task_id,
                                Some(handler),
                                Some(revert_prompt),
                            )
                        }
                    };
                let resolved_scan = resolved.clone();
                let scan_factory =
                    move |focus: String,
                          handler: runner::OutputHandler,
                          revert_prompt: runutil::RevertPromptHandler| {
                        let resolved = resolved_scan.clone();
                        let settings = scan_settings.clone();
                        let force = force;
                        let repoprompt_required = scan_repoprompt_required;
                        let git_revert_mode = scan_git_revert_mode;
                        move || {
                            scan_cmd::run_scan(
                                &resolved,
                                scan_cmd::ScanOptions {
                                    focus,
                                    runner: settings.runner,
                                    model: settings.model,
                                    reasoning_effort: settings.reasoning_effort,
                                    force,
                                    repoprompt_required,
                                    git_revert_mode,
                                    lock_mode: scan_cmd::ScanLockMode::Held,
                                    output_handler: Some(handler),
                                    revert_prompt: Some(revert_prompt),
                                },
                            )
                        }
                    };

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
                };

                let _ = tui::run_tui(&resolved, force, options, runner_factory, scan_factory)?;
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
    after_long_help = "Runner selection:\n\
 - `ralph run` selects runner/model/effort with this precedence:\n\
 1) CLI overrides (flags on `run one` / `run loop`)\n\
 2) the task's `agent` override (if present in .ralph/queue.json)\n\
 3) otherwise the resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\
\n\
Notes:\n\
 - Allowed runners: codex, opencode, gemini, claude\n\
 - Allowed models: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus (codex supports only gpt-5.2-codex + gpt-5.2; opencode/gemini/claude accept arbitrary model ids)\n\
 - `--effort` is codex-only and is ignored for other runners.\n\
 - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\
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
 ralph run loop --max-tasks 0\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode ask --max-tasks 1\n\
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
 ralph run one --phases 3\n\
 ralph run one --phases 2\n\
 ralph run one --phases 1\n\
 ralph run one --runner opencode --model gpt-5.2\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner codex --model gpt-5.2-codex --effort high\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode enabled\n\
 ralph run one --rp-on\n\
 ralph run one --rp-off\n\
 ralph tui"
    )]
    One(RunOneArgs),
    #[command(
        about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)",
        after_long_help = "Examples:\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --phases 3 --max-tasks 0\n\
 ralph run loop --phases 2 --max-tasks 0\n\
 ralph run loop --phases 1 --max-tasks 1\n\
 ralph run loop --max-tasks 3\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode disabled --max-tasks 1\n\
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
