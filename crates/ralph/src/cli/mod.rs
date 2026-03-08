//! Ralph CLI surface (Clap types) and shared CLI helpers.
//!
//! Responsibilities:
//! - Centralize the top-level `Cli` and `Command` definitions for Clap.
//! - Delegate command-group logic to submodules (queue/run/task/scan/etc.).
//! - Provide small shared CLI helpers used across command groups.
//!
//! Not handled here:
//! - Command execution logic (see submodules).
//! - Queue persistence or lock management.
//! - Prompt rendering or runner execution.
//!
//! Invariants/assumptions:
//! - Subcommands validate their own inputs and config dependencies.
//! - CLI parsing happens after argument normalization in `main`.

pub mod app;
pub mod cleanup;
pub mod color;
pub mod completions;
pub mod config;
pub mod context;
pub mod daemon;
pub mod doctor;
pub mod init;
pub mod migrate;
pub mod plugin;
pub mod prd;
pub mod productivity;
pub mod prompt;
pub mod queue;
pub mod run;
pub mod runner;
pub mod scan;
pub mod task;
pub mod tutorial;
pub mod undo;
pub mod version;
pub mod watch;
pub mod webhook;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::contracts::QueueFile;

pub use color::ColorArg;

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph")]
#[command(version)]
#[command(after_long_help = r#"Runner selection:
  - CLI flags override project config, which overrides global config, which overrides built-in defaults.
  - Default runner/model come from config files: project config (.ralph/config.jsonc) > global config (~/.config/ralph/config.jsonc, with .json fallback) > built-in.
  - `task` and `scan` accept --runner/--model/--effort as one-off overrides.
  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.

Config example (.ralph/config.jsonc):
  {
    "version": 1,
    "agent": {
      "runner": "codex",
      "model": "gpt-5.4",
      "codex_bin": "codex",
      "gemini_bin": "gemini",
      "claude_bin": "claude"
    }
  }

Notes:
  - Allowed runners: codex, opencode, gemini, claude, cursor, kimi, pi
  - Allowed models: gpt-5.4, gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding (codex supports only gpt-5.4 + gpt-5.3-codex + gpt-5.3-codex-spark + gpt-5.3 + gpt-5.2-codex + gpt-5.2; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids))
  - On macOS: use `ralph app open` to launch the GUI (requires an installed Ralph.app)

Examples:
  ralph app open
  ralph queue list
  ralph queue show RQ-0008
  ralph queue next --with-title
  ralph scan --runner opencode --model gpt-5.2 --focus "CI gaps"
  ralph task --runner codex --model gpt-5.4 --effort high "Fix the flaky test"
  ralph scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"
  ralph scan --runner claude --model sonnet --focus "risk audit"
  ralph task --runner claude --model opus "Add tests for X"
  ralph scan --runner cursor --model claude-opus-4-5-20251101 --focus "risk audit"
  ralph task --runner cursor --model claude-opus-4-5-20251101 "Add tests for X"
  ralph scan --runner kimi --focus "risk audit"
  ralph task --runner kimi --model kimi-for-coding "Add tests for X"
  ralph run one
  ralph run loop --max-tasks 1
  ralph run loop"#)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Force operations (e.g., bypass stale queue locks; bypass clean-repo safety checks for commands that enforce them, e.g. `run one`, `run loop`, and `scan`).
    #[arg(long, global = true)]
    pub force: bool,

    /// Increase output verbosity (sets log level to info).
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Color output control.
    #[arg(long, value_enum, default_value = "auto", global = true)]
    pub color: ColorArg,

    /// Disable colored output (alias for `--color never`).
    /// Also respects the NO_COLOR environment variable.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Automatically approve all migrations and fixes without prompting.
    /// Useful for CI/scripting environments.
    #[arg(long, global = true, conflicts_with = "no_sanity_checks")]
    pub auto_fix: bool,

    /// Skip startup sanity checks (migrations and unknown-key prompts).
    #[arg(long, global = true, conflicts_with = "auto_fix")]
    pub no_sanity_checks: bool,
}

#[derive(Subcommand)]
pub enum Command {
    Queue(queue::QueueArgs),
    Config(config::ConfigArgs),
    Run(Box<run::RunArgs>),
    Task(Box<task::TaskArgs>),
    Scan(scan::ScanArgs),
    Init(init::InitArgs),
    /// macOS app integration commands.
    App(app::AppArgs),
    /// Render and print the final compiled prompts used by Ralph (for debugging/auditing).
    #[command(
        after_long_help = "Examples:\n  ralph prompt worker --phase 1 --repo-prompt plan\n  ralph prompt worker --phase 2 --task-id RQ-0001 --plan-file .ralph/cache/plans/RQ-0001.md\n  ralph prompt scan --focus \"CI gaps\" --repo-prompt off\n  ralph prompt task-builder --request \"Add tests\" --tags rust,tests --scope crates/ralph --repo-prompt tools\n"
    )]
    Prompt(prompt::PromptArgs),
    /// Verify environment readiness and configuration.
    #[command(
        after_long_help = "Examples:\n  ralph doctor\n  ralph doctor --auto-fix\n  ralph doctor --no-sanity-checks\n  ralph doctor --format json\n  ralph doctor --format json --auto-fix"
    )]
    Doctor(doctor::DoctorArgs),
    /// Manage project context (AGENTS.md) for AI agents.
    #[command(
        after_long_help = "Examples:\n  ralph context init\n  ralph context init --project-type rust\n  ralph context update --section troubleshooting\n  ralph context validate\n  ralph context update --dry-run"
    )]
    Context(context::ContextArgs),
    /// Manage Ralph daemon (background service).
    #[command(
        after_long_help = "Examples:\n  ralph daemon start\n  ralph daemon start --empty-poll-ms 5000\n  ralph daemon stop\n  ralph daemon status"
    )]
    Daemon(daemon::DaemonArgs),
    /// Convert PRD (Product Requirements Document) markdown to tasks.
    #[command(
        after_long_help = "Examples:\n  ralph prd create docs/prd/new-feature.md\n  ralph prd create docs/prd/new-feature.md --multi\n  ralph prd create docs/prd/new-feature.md --dry-run\n  ralph prd create docs/prd/new-feature.md --priority high --tag feature\n  ralph prd create docs/prd/new-feature.md --draft"
    )]
    Prd(prd::PrdArgs),
    /// Generate shell completion scripts.
    #[command(
        after_long_help = "Examples:\n  ralph completions bash\n  ralph completions bash > ~/.local/share/bash-completion/completions/ralph\n  ralph completions zsh > ~/.zfunc/_ralph\n  ralph completions fish > ~/.config/fish/completions/ralph.fish\n  ralph completions powershell\n\nInstallation locations by shell:\n  Bash:   ~/.local/share/bash-completion/completions/ralph\n  Zsh:    ~/.zfunc/_ralph (and add 'fpath+=~/.zfunc' to ~/.zshrc)\n  Fish:   ~/.config/fish/completions/ralph.fish\n  PowerShell: Add to $PROFILE (see: $PROFILE | Get-Member -Type NoteProperty)"
    )]
    Completions(completions::CompletionsArgs),
    /// Check and apply migrations for config and project files.
    #[command(
        after_long_help = "Examples:\n  ralph migrate              # Check for pending migrations\n  ralph migrate --check      # Exit with error code if migrations pending (CI)\n  ralph migrate --apply      # Apply all pending migrations\n  ralph migrate --list       # List all migrations and their status\n  ralph migrate status       # Show detailed migration status"
    )]
    Migrate(migrate::MigrateArgs),
    /// Clean up temporary files created by Ralph.
    #[command(
        after_long_help = "Examples:\n  ralph cleanup              # Clean temp files older than 7 days\n  ralph cleanup --force      # Clean all ralph temp files\n  ralph cleanup --dry-run    # Show what would be deleted without deleting"
    )]
    Cleanup(cleanup::CleanupArgs),
    /// Display version information.
    #[command(after_long_help = "Examples:\n  ralph version\n  ralph version --verbose")]
    Version(version::VersionArgs),
    /// Watch files for changes and auto-detect tasks from TODO/FIXME/HACK/XXX comments.
    #[command(
        after_long_help = "Examples:\n  ralph watch\n  ralph watch src/\n  ralph watch --patterns \"*.rs,*.toml\"\n  ralph watch --auto-queue\n  ralph watch --notify\n  ralph watch --comments todo,fixme\n  ralph watch --debounce-ms 1000\n  ralph watch --ignore-patterns \"vendor/,target/,node_modules/\""
    )]
    Watch(watch::WatchArgs),
    /// Webhook management commands.
    #[command(
        after_long_help = "Examples:\n  ralph webhook test\n  ralph webhook test --event task_completed\n  ralph webhook status --format json\n  ralph webhook replay --dry-run --id wf-1700000000-1"
    )]
    Webhook(webhook::WebhookArgs),

    /// Productivity analytics (streaks, velocity, milestones).
    #[command(
        after_long_help = "Examples:\n  ralph productivity summary\n  ralph productivity velocity\n  ralph productivity streak"
    )]
    Productivity(productivity::ProductivityArgs),

    /// Plugin management commands.
    #[command(
        after_long_help = "Examples:\n  ralph plugin init my.plugin\n  ralph plugin init my.plugin --scope global\n  ralph plugin list\n  ralph plugin validate\n  ralph plugin install ./my-plugin --scope project\n  ralph plugin uninstall my.plugin --scope project"
    )]
    Plugin(plugin::PluginArgs),

    /// Runner management commands (capabilities, list).
    #[command(
        after_long_help = "Examples:\n  ralph runner capabilities codex\n  ralph runner capabilities claude --format json\n  ralph runner list\n  ralph runner list --format json"
    )]
    Runner(runner::RunnerArgs),

    /// Run interactive tutorial for Ralph onboarding.
    #[command(
        after_long_help = "Examples:\n  ralph tutorial\n  ralph tutorial --keep-sandbox\n  ralph tutorial --non-interactive"
    )]
    Tutorial(tutorial::TutorialArgs),

    /// Undo the most recent queue-modifying operation.
    #[command(
        after_long_help = "Examples:\n  ralph undo\n  ralph undo --list\n  ralph undo --dry-run\n  ralph undo --id undo-20260215073000000000\n\nSnapshots are created automatically before queue mutations such as:\n  - ralph task done/reject/start/ready/schedule\n  - ralph task edit/field/clone/split\n  - ralph task relate/blocks/mark-duplicate\n  - ralph queue archive/prune/sort/import\n  - ralph queue issue publish/publish-many\n  - ralph task batch operations"
    )]
    Undo(undo::UndoArgs),

    /// Internal: Emit a machine-readable CLI specification (JSON) for tooling and GUI clients.
    #[command(name = "cli-spec", hide = true, alias = "__cli-spec")]
    CliSpec(CliSpecArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CliSpecArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = CliSpecFormatArg::Json)]
    pub format: CliSpecFormatArg,
}

#[derive(ValueEnum, Debug, Copy, Clone, PartialEq, Eq)]
pub enum CliSpecFormatArg {
    Json,
}

pub fn handle_cli_spec(args: CliSpecArgs) -> Result<()> {
    match args.format {
        CliSpecFormatArg::Json => {
            let json = crate::commands::cli_spec::emit_cli_spec_json_pretty()?;
            use std::io::{self, Write};
            let mut stdout = io::stdout().lock();
            if let Err(err) = writeln!(stdout, "{json}") {
                if err.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(());
                }
                return Err(err.into());
            }
            Ok(())
        }
    }
}

pub(crate) fn load_and_validate_queues_read_only(
    resolved: &crate::config::Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    crate::queue::load_and_validate_queues(resolved, include_done)
}

pub(crate) fn resolve_list_limit(limit: u32, all: bool) -> Option<usize> {
    if all || limit == 0 {
        None
    } else {
        Some(limit as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, run};
    use crate::cli::{queue, task};
    use clap::Parser;
    use clap::error::ErrorKind;

    #[test]
    fn cli_parses_queue_list_smoke() {
        let cli = Cli::try_parse_from(["ralph", "queue", "list"]).expect("parse");
        match cli.command {
            Command::Queue(_) => {}
            other => panic!(
                "expected queue command, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn cli_parses_queue_archive_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "queue", "archive"]).expect("parse");
        match cli.command {
            Command::Queue(queue::QueueArgs { command }) => match command {
                queue::QueueCommand::Archive(_) => {}
                _ => panic!("expected queue archive command"),
            },
            _ => panic!("expected queue command"),
        }
    }

    #[test]
    fn cli_rejects_invalid_prompt_phase() {
        let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "4"])
            .err()
            .expect("parse failure");
        let msg = err.to_string();
        assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
    }

    #[test]
    fn cli_parses_run_git_revert_mode() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--git-revert-mode", "disabled"])
            .expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::One(args) => {
                    assert_eq!(args.agent.git_revert_mode.as_deref(), Some("disabled"));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_git_commit_push_off() {
        let cli =
            Cli::try_parse_from(["ralph", "run", "one", "--git-commit-push-off"]).expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::One(args) => {
                    assert!(args.agent.git_commit_push_off);
                    assert!(!args.agent.git_commit_push_on);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_include_draft() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--include-draft"]).expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::One(args) => {
                    assert!(args.agent.include_draft);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_one_debug() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--debug"]).expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::One(args) => {
                    assert!(args.debug);
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_loop_debug() {
        let cli = Cli::try_parse_from(["ralph", "run", "loop", "--debug"]).expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::Loop(args) => {
                    assert!(args.debug);
                }
                _ => panic!("expected run loop command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_run_one_id() {
        let cli = Cli::try_parse_from(["ralph", "run", "one", "--id", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Run(args) => match args.command {
                run::RunCommand::One(args) => {
                    assert_eq!(args.id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected run one command"),
            },
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn cli_parses_task_update_without_id() {
        let cli = Cli::try_parse_from(["ralph", "task", "update"]).expect("parse");
        match cli.command {
            Command::Task(args) => match args.command {
                Some(task::TaskCommand::Update(args)) => {
                    assert!(args.task_id.is_none());
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_update_with_id() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Task(args) => match args.command {
                Some(task::TaskCommand::Update(args)) => {
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_rejects_removed_run_one_interactive_flag_short() {
        let err = Cli::try_parse_from(["ralph", "run", "one", "-i"])
            .err()
            .expect("parse failure");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_rejects_removed_run_one_interactive_flag_long() {
        let err = Cli::try_parse_from(["ralph", "run", "one", "--interactive"])
            .err()
            .expect("parse failure");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_parses_task_default_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "Add", "tests"]).expect("parse");
        match cli.command {
            Command::Task(args) => {
                assert!(args.command.is_none(), "expected implicit build subcommand");
                assert_eq!(
                    args.build.request,
                    vec!["Add".to_string(), "tests".to_string()]
                );
            }
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_ready_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "ready", "RQ-0005"]).expect("parse");
        match cli.command {
            Command::Task(args) => match args.command {
                Some(task::TaskCommand::Ready(args)) => {
                    assert_eq!(args.task_id, "RQ-0005");
                }
                _ => panic!("expected task ready command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_done_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "done", "RQ-0001"]).expect("parse");
        match cli.command {
            Command::Task(args) => match args.command {
                Some(task::TaskCommand::Done(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                }
                _ => panic!("expected task done command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_parses_task_reject_subcommand() {
        let cli = Cli::try_parse_from(["ralph", "task", "reject", "RQ-0002"]).expect("parse");
        match cli.command {
            Command::Task(args) => match args.command {
                Some(task::TaskCommand::Reject(args)) => {
                    assert_eq!(args.task_id, "RQ-0002");
                }
                _ => panic!("expected task reject command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn cli_rejects_queue_set_status_subcommand() {
        let result = Cli::try_parse_from(["ralph", "queue", "set-status", "RQ-0001", "doing"]);
        assert!(result.is_err(), "expected queue set-status to be rejected");
        let msg = result.err().unwrap().to_string().to_lowercase();
        assert!(
            msg.contains("unrecognized") || msg.contains("unexpected") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_rejects_removed_run_loop_interactive_flag_short() {
        let err = Cli::try_parse_from(["ralph", "run", "loop", "-i"])
            .err()
            .expect("parse failure");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_rejects_removed_run_loop_interactive_flag_long() {
        let err = Cli::try_parse_from(["ralph", "run", "loop", "--interactive"])
            .err()
            .expect("parse failure");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_rejects_removed_tui_command() {
        let err = Cli::try_parse_from(["ralph", "tui"])
            .err()
            .expect("parse failure");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_rejects_run_loop_with_id_flag() {
        let err = Cli::try_parse_from(["ralph", "run", "loop", "--id", "RQ-0001"])
            .err()
            .expect("parse failure");
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn cli_supports_top_level_version_flag_long() {
        let err = Cli::try_parse_from(["ralph", "--version"])
            .err()
            .expect("expected clap to render version and exit");
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
        let rendered = err.to_string();
        assert!(rendered.contains("ralph"));
        assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn cli_supports_top_level_version_flag_short() {
        let err = Cli::try_parse_from(["ralph", "-V"])
            .err()
            .expect("expected clap to render version and exit");
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
        let rendered = err.to_string();
        assert!(rendered.contains("ralph"));
        assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
    }
}
