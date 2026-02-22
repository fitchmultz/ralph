//! `ralph run ...` command group: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap structures for run commands and flags.
//! - Route run subcommands to supervisor execution entry points.
//!
//! Not handled here:
//! - Queue persistence and task status transitions (see `crate::queue`).
//! - Runner implementations or model execution (see `crate::runner`).
//! - Global configuration precedence rules (see `crate::config`).
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Queue mutations occur inside downstream command handlers.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::{agent, commands::run as run_cmd, config, debuglog};

pub fn handle_run(cmd: RunCommand, force: bool) -> Result<()> {
    // Extract profile from the command to resolve config with the selected profile
    let profile = match &cmd {
        RunCommand::Resume(args) => args.agent.profile.as_deref(),
        RunCommand::One(args) => args.agent.profile.as_deref(),
        RunCommand::Loop(args) => args.agent.profile.as_deref(),
        RunCommand::Parallel(_) => None,
    };
    let resolved = config::resolve_from_cwd_with_profile(profile)?;
    match cmd {
        RunCommand::Resume(args) => {
            if args.debug {
                debuglog::enable(&resolved.repo_root)?;
            }
            // Profile already applied during config resolution; just resolve remaining overrides
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            // Resume is essentially a loop with auto_resume=true
            run_cmd::run_loop(
                &resolved,
                run_cmd::RunLoopOptions {
                    max_tasks: 0, // No limit when resuming
                    agent_overrides: overrides,
                    force: args.force || force,
                    auto_resume: true,
                    starting_completed: 0,
                    non_interactive: args.non_interactive,
                    parallel_workers: None,
                    wait_when_blocked: false,
                    wait_poll_ms: 1000,
                    wait_timeout_seconds: 0,
                    notify_when_unblocked: false,
                    wait_when_empty: false,
                    empty_poll_ms: 30_000,
                },
            )
        }
        RunCommand::One(args) => {
            if args.debug {
                debuglog::enable(&resolved.repo_root)?;
            }
            // Profile already applied during config resolution; just resolve remaining overrides
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.dry_run {
                if args.parallel_worker {
                    return Err(anyhow::anyhow!(
                        "--dry-run cannot be used with --parallel-worker"
                    ));
                }
                run_cmd::dry_run_one(&resolved, &overrides, args.id.as_deref())
            } else {
                if args.parallel_worker {
                    let task_id = args.id.as_deref().ok_or_else(|| {
                        anyhow::anyhow!("--parallel-worker requires --id <TASK_ID>")
                    })?;
                    let target_branch =
                        args.parallel_target_branch.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("--parallel-worker requires --parallel-target-branch")
                        })?;

                    let mut worker_resolved = resolved.clone();
                    worker_resolved.queue_path =
                        args.coordinator_queue_path.clone().ok_or_else(|| {
                            anyhow::anyhow!("--parallel-worker requires --coordinator-queue-path")
                        })?;
                    worker_resolved.done_path =
                        args.coordinator_done_path.clone().ok_or_else(|| {
                            anyhow::anyhow!("--parallel-worker requires --coordinator-done-path")
                        })?;
                    log::debug!(
                        "parallel worker using queue/done paths: queue={}, done={}, target_branch={}",
                        worker_resolved.queue_path.display(),
                        worker_resolved.done_path.display(),
                        target_branch
                    );

                    run_cmd::run_one_parallel_worker(
                        &worker_resolved,
                        &overrides,
                        force,
                        task_id,
                        target_branch,
                    )?;
                    return Ok(());
                }

                if let Some(task_id) = args.id.as_deref() {
                    run_cmd::run_one_with_id(&resolved, &overrides, force, task_id, None, None)?;
                } else {
                    run_cmd::run_one(&resolved, &overrides, force, None)?;
                }
                Ok(())
            }
        }
        RunCommand::Loop(args) => {
            if args.debug {
                debuglog::enable(&resolved.repo_root)?;
            }
            // Profile already applied during config resolution; just resolve remaining overrides
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;

            if args.dry_run {
                run_cmd::dry_run_loop(&resolved, &overrides)
            } else {
                run_cmd::run_loop(
                    &resolved,
                    run_cmd::RunLoopOptions {
                        max_tasks: args.max_tasks,
                        agent_overrides: overrides,
                        force,
                        auto_resume: args.resume,
                        starting_completed: 0,
                        non_interactive: args.non_interactive,
                        parallel_workers: args.parallel,
                        wait_when_blocked: args.wait_when_blocked,
                        wait_poll_ms: args.wait_poll_ms,
                        wait_timeout_seconds: args.wait_timeout_seconds,
                        notify_when_unblocked: args.notify_when_unblocked,
                        wait_when_empty: args.wait_when_empty,
                        empty_poll_ms: args.empty_poll_ms,
                    },
                )
            }
        }
        RunCommand::Parallel(args) => match args.command {
            ParallelSubcommand::Status(status_args) => {
                run_cmd::parallel_status(&resolved, status_args.json)
            }
            ParallelSubcommand::Retry(retry_args) => {
                run_cmd::parallel_retry(&resolved, &retry_args.task, force)
            }
        },
    }
}

#[derive(Args)]
#[command(
    about = "Run Ralph supervisor (executes queued tasks via codex/opencode/gemini/claude/cursor/kimi/pi)",
    after_long_help = "Runner selection:\n\
  - `ralph run` selects runner/model/effort with this precedence:\n\
  1) CLI overrides (flags on `run one` / `run loop`)\n\
  2) task's `agent` override (runner/model plus `model_effort` if set)\n\
  3) otherwise: resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\
 \n\
 Notes:\n\
	  - Allowed runners: codex, opencode, gemini, claude, cursor, kimi, pi\n\
	  - Allowed models: gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding (codex supports only gpt-5.3-codex + gpt-5.3-codex-spark + gpt-5.3 + gpt-5.2-codex + gpt-5.2; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids)\n\
	  - `--effort` is codex-only and is ignored for other runners.\n\
	  - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\
	  - `--git-commit-push-on` / `--git-commit-push-off` control automatic git commit/push after successful runs.\n\
	     - `--parallel` runs loop tasks concurrently in workspaces (clone-based).\n\
	     - Workers push directly to the target branch after phase execution.\n\
	  - Clean-repo checks allow changes to `.ralph/config.{json,jsonc}` (plus `.ralph/queue.{json,jsonc}` and `.ralph/done.{json,jsonc}`); use `--force` to bypass entirely.\n\
	 \n\
Phase-specific overrides:\n\
	  Use --runner-phaseN, --model-phaseN, --effort-phaseN to override settings for a specific phase.\n\
  Phase-specific flags take precedence over global flags for that phase.\n\
  Single-pass (--phases 1) uses Phase 2 overrides.\n\
 \n\
  Precedence per phase (highest to lowest):\n\
    1) CLI phase override (--runner-phaseN, --model-phaseN, --effort-phaseN)\n\
    2) Task phase override (task.agent.phase_overrides.phaseN.*)\n\
    3) Config phase override (agent.phase_overrides.phaseN.*)\n\
    4) CLI global override (--runner, --model, --effort)\n\
    5) Task global override (task.agent.runner/model/model_effort)\n\
    6) Config defaults (agent.*)\n\
 \n\
 To change defaults for this repo, edit .ralph/config.jsonc:\n\
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
 ralph run one --runner codex --model gpt-5.3-codex --effort high\n\
 ralph run one --runner-phase1 codex --model-phase1 gpt-5.2-codex --effort-phase1 high\n\
 ralph run one --runner-phase2 claude --model-phase2 opus\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner pi --model gpt-5.2\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode disabled\n\
 ralph run one --git-commit-push-off\n\
 ralph run one --lfs-check\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode ask --max-tasks 1\n\
 ralph run loop --git-commit-push-on --max-tasks 1\n\
 ralph run loop --lfs-check --max-tasks 1\n\
 ralph run loop --parallel --max-tasks 4\n\
	 ralph run loop --parallel 4 --max-tasks 8\n\
	 ralph run resume\n\
	 ralph run resume --force\n\
	 ralph run loop --resume --max-tasks 5"
)]
pub struct RunArgs {
    #[command(subcommand)]
    pub command: RunCommand,
}

#[derive(Subcommand)]
pub enum RunCommand {
    /// Resume an interrupted session from where it left off.
    #[command(
        about = "Resume an interrupted session from where it left off",
        after_long_help = "Examples:
 ralph run resume
 ralph run resume --force"
    )]
    Resume(ResumeArgs),
    #[command(
        about = "Run exactly one task (the first todo in the configured queue file)",
        after_long_help = "Runner selection (precedence):\n\
 1) CLI overrides (--runner/--model/--effort)\n\
 2) task.agent in the configured queue file (if present)\n\
 3) selected profile (if --profile specified)\n\
 4) config defaults (.ralph/config.jsonc then ~/.config/ralph/config.jsonc, with .json fallback)\n\
\n\
Examples:\n\
 ralph run one\n\
 ralph run one --id RQ-0001\n\
 ralph run one --debug\n\
 ralph run one --profile quick (kimi, 1-phase)\n\
 ralph run one --profile thorough (claude/opus, 3-phase)\n\
 ralph run one --phases 3 (plan/implement+CI/review+complete)\n\
 ralph run one --phases 2 (plan/implement)\n\
 ralph run one --phases 1 (single-pass)\n\
 ralph run one --quick (single-pass, same as --phases 1)\n\
 ralph run one --runner opencode --model gpt-5.2\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner pi --model gpt-5.2\n\
 ralph run one --runner codex --model gpt-5.3-codex --effort high\n\
 ralph run one --runner-phase1 codex --model-phase1 gpt-5.2-codex --effort-phase1 high\n\
 ralph run one --runner-phase2 claude --model-phase2 opus\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode enabled\n\
 ralph run one --git-commit-push-off\n\
 ralph run one --lfs-check\n\
 ralph run one --repo-prompt plan\n\
 ralph run one --repo-prompt off\n\
 ralph run one --non-interactive\n\
 ralph run one --dry-run\n\
 ralph run one --dry-run --include-draft\n\
 ralph run one --dry-run --id RQ-0001"
    )]
    One(RunOneArgs),
    #[command(
        about = "Run tasks repeatedly until no todo remain (or --max-tasks is reached)",
        after_long_help = "Examples:\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --profile quick --max-tasks 5 (kimi, 1-phase)\n\
 ralph run loop --profile thorough --max-tasks 5 (claude/opus, 3-phase)\n\
 ralph run loop --phases 3 --max-tasks 0 (plan/implement+CI/review+complete)\n\
 ralph run loop --phases 2 --max-tasks 0 (plan/implement)\n\
 ralph run loop --phases 1 --max-tasks 1 (single-pass)\n\
 ralph run loop --quick --max-tasks 1 (single-pass, same as --phases 1)\n\
 ralph run loop --max-tasks 3\n\
 ralph run loop --max-tasks 1 --debug\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.2\n\
 ralph run loop --runner-phase1 codex --model-phase1 gpt-5.2-codex --effort-phase1 high --max-tasks 1\n\
 ralph run loop --runner-phase2 claude --model-phase2 opus --max-tasks 1\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode disabled --max-tasks 1\n\
 ralph run loop --git-commit-push-off --max-tasks 1\n\
 ralph run loop --repo-prompt tools --max-tasks 1\n\
 ralph run loop --repo-prompt off --max-tasks 1\n\
	 ralph run loop --lfs-check --max-tasks 1\n\
	 ralph run loop --dry-run\n\
	 ralph run loop --wait-when-blocked\n\
	 ralph run loop --wait-when-blocked --wait-timeout-seconds 600\n\
	 ralph run loop --wait-when-blocked --wait-poll-ms 250\n\
	 ralph run loop --wait-when-blocked --notify-when-unblocked"
    )]
    Loop(RunLoopArgs),
    /// Manage parallel mode operations (status, retry blocked workers).
    #[command(
        about = "Manage parallel mode operations",
        after_long_help = "Examples:\n\
 ralph run parallel status\n\
 ralph run parallel status --json\n\
 ralph run parallel retry --task RQ-0001"
    )]
    Parallel(ParallelArgs),
}

// MergeAgent command removed in direct-push rewrite (Phase D)
// Workers now push directly to the target branch without creating PRs

#[derive(Args)]
pub struct ResumeArgs {
    /// Skip the confirmation prompt for stale sessions.
    #[arg(long)]
    pub force: bool,

    /// Capture raw supervisor + runner output to .ralph/logs/debug.log.
    #[arg(long)]
    pub debug: bool,

    /// Skip interactive prompts (for CI/non-interactive environments).
    #[arg(long)]
    pub non_interactive: bool,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct RunOneArgs {
    /// Capture raw supervisor + runner output to .ralph/logs/debug.log.
    #[arg(long)]
    pub debug: bool,

    /// Run a specific task by ID.
    #[arg(long, value_name = "TASK_ID")]
    pub id: Option<String>,

    /// Skip interactive prompts (for CI/non-interactive environments).
    #[arg(long)]
    pub non_interactive: bool,

    /// Select a task and print why it would (or would not) run.
    /// Does not invoke any runner and does not write queue/done.
    #[arg(long, conflicts_with = "parallel_worker")]
    pub dry_run: bool,

    /// Internal: run as a parallel worker (skips queue lock, allows upstream creation).
    #[arg(
        long,
        hide = true,
        requires_all = [
            "id",
            "coordinator_queue_path",
            "coordinator_done_path",
            "parallel_target_branch"
        ]
    )]
    pub parallel_worker: bool,

    /// Internal: queue file path for the parallel worker workspace checkout.
    #[arg(
        long,
        hide = true,
        value_name = "PATH",
        requires_all = ["parallel_worker", "coordinator_done_path"]
    )]
    pub coordinator_queue_path: Option<PathBuf>,

    /// Internal: done file path for the parallel worker workspace checkout.
    #[arg(
        long,
        hide = true,
        value_name = "PATH",
        requires_all = ["parallel_worker", "coordinator_queue_path"]
    )]
    pub coordinator_done_path: Option<PathBuf>,

    /// Internal: explicit coordinator target branch for parallel integration.
    #[arg(
        long,
        hide = true,
        value_name = "BRANCH",
        requires_all = ["parallel_worker", "coordinator_queue_path", "coordinator_done_path"]
    )]
    pub parallel_target_branch: Option<String>,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

#[derive(Args)]
pub struct RunLoopArgs {
    /// Maximum tasks to run before stopping (0 = no limit).
    #[arg(long, default_value_t = 0)]
    pub max_tasks: u32,

    /// Capture raw supervisor + runner output to .ralph/logs/debug.log.
    #[arg(long)]
    pub debug: bool,

    /// Automatically resume an interrupted session without prompting.
    #[arg(long)]
    pub resume: bool,

    /// Skip interactive prompts (for CI/non-interactive environments).
    #[arg(long)]
    pub non_interactive: bool,

    /// Select a task and print why it would (or would not) run.
    /// Does not invoke any runner and does not write queue/done.
    #[arg(long, conflicts_with = "parallel")]
    pub dry_run: bool,

    /// Run tasks in parallel using N workers (default when flag present: 2).
    #[arg(
        long,
        value_parser = clap::value_parser!(u8).range(2..),
        num_args = 0..=1,
        default_missing_value = "2",
        value_name = "N",
    )]
    pub parallel: Option<u8>,

    /// Wait when blocked by dependencies/schedule instead of exiting.
    /// The loop will poll until a runnable task appears or timeout is reached.
    #[arg(long, conflicts_with = "parallel")]
    pub wait_when_blocked: bool,

    /// Poll interval in milliseconds while waiting for unblocked tasks (default: 1000, min: 50).
    #[arg(
        long,
        default_value_t = 1000,
        value_parser = clap::value_parser!(u64).range(50..),
        value_name = "MS"
    )]
    pub wait_poll_ms: u64,

    /// Timeout in seconds for waiting (0 = no timeout).
    #[arg(long, default_value_t = 0, value_name = "SECONDS")]
    pub wait_timeout_seconds: u64,

    /// Notify when queue becomes unblocked (desktop + webhook).
    #[arg(long)]
    pub notify_when_unblocked: bool,

    /// Wait when queue is empty instead of exiting (continuous mode).
    /// Alias: --continuous
    #[arg(long, alias = "continuous", conflicts_with = "parallel")]
    pub wait_when_empty: bool,

    /// Poll interval in milliseconds while waiting for new tasks when queue is empty
    /// (default: 30000, min: 50). Only used with --wait-when-empty.
    #[arg(
        long,
        default_value_t = 30_000,
        value_parser = clap::value_parser!(u64).range(50..),
        value_name = "MS"
    )]
    pub empty_poll_ms: u64,

    #[command(flatten)]
    pub agent: crate::agent::RunAgentArgs,
}

/// Arguments for `ralph run parallel` subcommand.
#[derive(Args)]
pub struct ParallelArgs {
    #[command(subcommand)]
    pub command: ParallelSubcommand,
}

/// Subcommands for `ralph run parallel`.
#[derive(Subcommand)]
pub enum ParallelSubcommand {
    /// Show status of parallel workers (active, completed, failed, blocked).
    #[command(
        about = "Show status of parallel workers",
        after_long_help = "Examples:\n\
 ralph run parallel status\n\
 ralph run parallel status --json"
    )]
    Status(ParallelStatusArgs),
    /// Retry a blocked or failed parallel worker.
    #[command(
        about = "Retry a blocked or failed parallel worker",
        after_long_help = "Examples:\n\
 ralph run parallel retry --task RQ-0001"
    )]
    Retry(ParallelRetryArgs),
}

/// Arguments for `ralph run parallel status`.
#[derive(Args)]
pub struct ParallelStatusArgs {
    /// Output as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `ralph run parallel retry`.
#[derive(Args)]
pub struct ParallelRetryArgs {
    /// Task ID of the blocked/failed worker to retry.
    #[arg(long, value_name = "TASK_ID", required = true)]
    pub task: String,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::{CommandFactory, Parser};

    use crate::cli::{Cli, run::RunCommand};

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

    #[test]
    fn run_one_non_interactive_parses() {
        let args = vec!["ralph", "run", "one", "--non-interactive"];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::One(one_args) => {
                    assert!(one_args.non_interactive);
                }
                _ => panic!("expected RunCommand::One"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_one_non_interactive_with_id_parses() {
        let args = vec![
            "ralph",
            "run",
            "one",
            "--non-interactive",
            "--id",
            "RQ-0001",
        ];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::One(one_args) => {
                    assert!(one_args.non_interactive);
                    assert_eq!(one_args.id, Some("RQ-0001".to_string()));
                }
                _ => panic!("expected RunCommand::One"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_one_dry_run_parses() {
        let args = vec!["ralph", "run", "one", "--dry-run"];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::One(one_args) => {
                    assert!(one_args.dry_run);
                }
                _ => panic!("expected RunCommand::One"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_one_dry_run_with_id_parses() {
        let args = vec!["ralph", "run", "one", "--dry-run", "--id", "RQ-0001"];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::One(one_args) => {
                    assert!(one_args.dry_run);
                    assert_eq!(one_args.id, Some("RQ-0001".to_string()));
                }
                _ => panic!("expected RunCommand::One"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_loop_dry_run_parses() {
        let args = vec!["ralph", "run", "loop", "--dry-run"];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::Loop(loop_args) => {
                    assert!(loop_args.dry_run);
                }
                _ => panic!("expected RunCommand::Loop"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_loop_dry_run_conflicts_with_parallel() {
        let args = vec!["ralph", "run", "loop", "--dry-run", "--parallel"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "--dry-run and --parallel should conflict");
    }

    #[test]
    fn run_one_help_includes_dry_run_examples() {
        let mut cmd = Cli::command();
        let run = cmd.find_subcommand_mut("run").expect("run subcommand");
        let run_one = run.find_subcommand_mut("one").expect("run one subcommand");
        let help = run_one.render_long_help().to_string();

        assert!(
            help.contains("ralph run one --dry-run"),
            "missing dry-run example: {help}"
        );
        assert!(
            help.contains("ralph run one --dry-run --include-draft"),
            "missing dry-run --include-draft example: {help}"
        );
        assert!(
            help.contains("ralph run one --dry-run --id RQ-0001"),
            "missing dry-run --id example: {help}"
        );
    }

    #[test]
    fn run_loop_help_includes_dry_run_examples() {
        let mut cmd = Cli::command();
        let run = cmd.find_subcommand_mut("run").expect("run subcommand");
        let run_loop = run
            .find_subcommand_mut("loop")
            .expect("run loop subcommand");
        let help = run_loop.render_long_help().to_string();

        assert!(
            help.contains("ralph run loop --dry-run"),
            "missing dry-run example: {help}"
        );
    }

    #[test]
    fn run_loop_wait_poll_ms_rejects_below_minimum() {
        let args = vec!["ralph", "run", "loop", "--wait-poll-ms", "10"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "--wait-poll-ms should reject values below 50"
        );
    }

    #[test]
    fn run_loop_empty_poll_ms_rejects_below_minimum() {
        let args = vec!["ralph", "run", "loop", "--empty-poll-ms", "10"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "--empty-poll-ms should reject values below 50"
        );
    }

    #[test]
    fn run_loop_wait_poll_ms_accepts_minimum() {
        let args = vec!["ralph", "run", "loop", "--wait-poll-ms", "50"];
        let cli = Cli::try_parse_from(args);
        assert!(cli.is_ok(), "--wait-poll-ms should accept 50");
        if let Ok(cli) = cli {
            match cli.command {
                crate::cli::Command::Run(run_args) => match run_args.command {
                    RunCommand::Loop(loop_args) => {
                        assert_eq!(loop_args.wait_poll_ms, 50);
                    }
                    _ => panic!("expected RunCommand::Loop"),
                },
                _ => panic!("expected Command::Run"),
            }
        }
    }

    #[test]
    fn run_one_parallel_worker_with_coordinator_paths_parses() {
        let args = vec![
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001",
            "--coordinator-queue-path",
            "/path/to/queue.json",
            "--coordinator-done-path",
            "/path/to/done.json",
            "--parallel-target-branch",
            "main",
        ];
        let cli = Cli::parse_from(args);
        match cli.command {
            crate::cli::Command::Run(run_args) => match run_args.command {
                RunCommand::One(one_args) => {
                    assert!(one_args.parallel_worker);
                    assert_eq!(one_args.id, Some("RQ-0001".to_string()));
                    assert_eq!(
                        one_args.coordinator_queue_path,
                        Some(PathBuf::from("/path/to/queue.json"))
                    );
                    assert_eq!(
                        one_args.coordinator_done_path,
                        Some(PathBuf::from("/path/to/done.json"))
                    );
                    assert_eq!(one_args.parallel_target_branch, Some("main".to_string()));
                }
                _ => panic!("expected RunCommand::One"),
            },
            _ => panic!("expected Command::Run"),
        }
    }

    #[test]
    fn run_one_parallel_worker_requires_coordinator_paths() {
        let args = vec![
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001",
        ];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "--parallel-worker should require coordinator queue/done paths and target branch"
        );
    }

    #[test]
    fn run_one_parallel_worker_requires_both_coordinator_paths() {
        let args = vec![
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001",
            "--coordinator-queue-path",
            "/path/to/queue.json",
        ];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "missing --coordinator-done-path should fail parsing"
        );
    }

    #[test]
    fn run_one_parallel_worker_requires_target_branch() {
        let args = vec![
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001",
            "--coordinator-queue-path",
            "/path/to/queue.json",
            "--coordinator-done-path",
            "/path/to/done.json",
        ];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "missing --parallel-target-branch should fail parsing"
        );
    }
}
