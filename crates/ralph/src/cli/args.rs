//! Top-level Clap argument definitions for `ralph`.
//!
//! Purpose:
//! - Top-level Clap argument definitions for `ralph`.
//!
//! Responsibilities:
//! - Define the root `Cli` parser and the top-level command enum.
//! - Keep top-level command documentation and long-help text together.
//! - Re-export the small `cli-spec` format args used by the machine/app tooling.
//!
//! Not handled here:
//! - Command execution logic.
//! - Shared queue/list helper functions.
//! - Parse-regression tests.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Subcommands validate their own inputs and config dependencies.
//! - CLI parsing happens after argument normalization in `main`.

use clap::{Args, Parser, Subcommand, ValueEnum};

use super::{
    app, cleanup, color::ColorArg, completions, config, context, daemon, doctor, init, machine,
    migrate, plugin, prd, productivity, prompt, queue, run, runner, scan, task, tutorial, undo,
    version, watch, webhook,
};

#[derive(Parser)]
#[command(name = "ralph")]
#[command(about = "Ralph")]
#[command(version)]
#[command(after_long_help = r#"Runner selection:
  - CLI flags override project config, which overrides global config, which overrides built-in defaults.
  - Default runner/model come from config files: project config (.ralph/config.jsonc) > global config (~/.config/ralph/config.jsonc) > built-in.
  - `task` and `scan` accept --runner/--model/--effort as one-off overrides.
  - `run one` and `run loop` accept --runner/--model/--effort as one-off overrides; otherwise they use task.agent overrides when present; otherwise config agent defaults.

Config example (.ralph/config.jsonc):
  {
    "version": 2,
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
  - Allowed models: gpt-5.4, gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding (codex supports only gpt-5.4 + gpt-5.3-codex + gpt-5.3-codex-spark + gpt-5.3; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids))
  - On macOS: use `ralph app open` to launch the GUI (requires an installed Ralph.app)
  - App-launched runs are noninteractive: they stream output, but interactive approvals remain terminal-only.

Examples:
  ralph app open
  ralph queue list
  ralph queue show RQ-0008
  ralph queue next --with-title
  ralph scan --runner opencode --model gpt-5.3 --focus "CI gaps"
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
  ralph run loop

More help:
  - Default help shows core commands only.
  - Run `ralph help-all` to see advanced and experimental commands."#)]
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
    /// Show core, advanced, and experimental command groups.
    HelpAll,
    /// Versioned machine-facing JSON API for the macOS app.
    #[command(hide = true)]
    Machine(Box<machine::MachineArgs>),
    /// Render and print the final compiled prompts used by Ralph (for debugging/auditing).
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph prompt worker --phase 1 --repo-prompt plan\n  ralph prompt worker --phase 2 --task-id RQ-0001 --plan-file .ralph/cache/plans/RQ-0001.md\n  ralph prompt scan --focus \"CI gaps\" --repo-prompt off\n  ralph prompt task-builder --request \"Add tests\" --tags rust,tests --scope crates/ralph --repo-prompt tools\n"
    )]
    Prompt(prompt::PromptArgs),
    /// Verify environment readiness and configuration.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph doctor\n  ralph doctor --auto-fix\n  ralph doctor --no-sanity-checks\n  ralph doctor --format json\n  ralph doctor --format json --auto-fix"
    )]
    Doctor(doctor::DoctorArgs),
    /// Manage project context (AGENTS.md) for AI agents.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph context init\n  ralph context init --project-type rust\n  ralph context update --section troubleshooting\n  ralph context validate\n  ralph context update --dry-run"
    )]
    Context(context::ContextArgs),
    /// Manage Ralph daemon (background service).
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph daemon start\n  ralph daemon start --empty-poll-ms 5000\n  ralph daemon stop\n  ralph daemon status"
    )]
    Daemon(daemon::DaemonArgs),
    /// Convert PRD (Product Requirements Document) markdown to tasks.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph prd create docs/prd/new-feature.md\n  ralph prd create docs/prd/new-feature.md --multi\n  ralph prd create docs/prd/new-feature.md --dry-run\n  ralph prd create docs/prd/new-feature.md --priority high --tag feature\n  ralph prd create docs/prd/new-feature.md --draft"
    )]
    Prd(prd::PrdArgs),
    /// Generate shell completion scripts.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph completions bash\n  ralph completions bash > ~/.local/share/bash-completion/completions/ralph\n  ralph completions zsh > ~/.zfunc/_ralph\n  ralph completions fish > ~/.config/fish/completions/ralph.fish\n  ralph completions powershell\n\nInstallation locations by shell:\n  Bash:   ~/.local/share/bash-completion/completions/ralph\n  Zsh:    ~/.zfunc/_ralph (and add 'fpath+=~/.zfunc' to ~/.zshrc)\n  Fish:   ~/.config/fish/completions/ralph.fish\n  PowerShell: Add to $PROFILE (see: $PROFILE | Get-Member -Type NoteProperty)"
    )]
    Completions(completions::CompletionsArgs),
    /// Check and apply migrations for config and project files.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph migrate              # Check for pending migrations\n  ralph migrate --check      # Exit with error code if migrations pending (CI)\n  ralph migrate --apply      # Apply all pending migrations\n  ralph migrate --list       # List all migrations and their status\n  ralph migrate status       # Show detailed migration status"
    )]
    Migrate(migrate::MigrateArgs),
    /// Clean up temporary files created by Ralph.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph cleanup              # Clean temp files older than 7 days\n  ralph cleanup --force      # Clean all ralph temp files\n  ralph cleanup --dry-run    # Show what would be deleted without deleting"
    )]
    Cleanup(cleanup::CleanupArgs),
    /// Display version information.
    #[command(after_long_help = "Examples:\n  ralph version\n  ralph version --verbose")]
    Version(version::VersionArgs),
    /// Watch files for changes and auto-detect tasks from TODO/FIXME/HACK/XXX comments.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph watch\n  ralph watch src/\n  ralph watch --patterns \"*.rs,*.toml\"\n  ralph watch --auto-queue\n  ralph watch --notify\n  ralph watch --comments todo,fixme\n  ralph watch --debounce-ms 1000\n  ralph watch --ignore-patterns \"vendor/,target/,node_modules/\""
    )]
    Watch(watch::WatchArgs),
    /// Webhook management commands.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph webhook test\n  ralph webhook test --event task_completed\n  ralph webhook status --format json\n  ralph webhook replay --dry-run --id wf-1700000000-1"
    )]
    Webhook(webhook::WebhookArgs),

    /// Productivity analytics (streaks, velocity, milestones).
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph productivity summary\n  ralph productivity velocity\n  ralph productivity streak"
    )]
    Productivity(productivity::ProductivityArgs),

    /// Plugin management commands.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph plugin init my.plugin\n  ralph plugin init my.plugin --scope global\n  ralph plugin list\n  ralph plugin validate\n  ralph plugin install ./my-plugin --scope project\n  ralph plugin uninstall my.plugin --scope project"
    )]
    Plugin(plugin::PluginArgs),

    /// Runner management commands (capabilities, list).
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph runner capabilities codex\n  ralph runner capabilities claude --format json\n  ralph runner list\n  ralph runner list --format json"
    )]
    Runner(runner::RunnerArgs),

    /// Run interactive tutorial for Ralph onboarding.
    #[command(
        hide = true,
        after_long_help = "Examples:\n  ralph tutorial\n  ralph tutorial --keep-sandbox\n  ralph tutorial --non-interactive"
    )]
    Tutorial(tutorial::TutorialArgs),

    /// Restore or preview an earlier continuation checkpoint.
    #[command(
        after_long_help = "Continuation workflow:\n  - `ralph undo --list` shows the checkpoints Ralph created before queue-changing operations.\n  - `ralph undo --dry-run` previews the restore path without modifying queue files.\n  - `ralph undo` restores the most recent checkpoint; `--id` restores a specific one.\n  - After restoring, run `ralph queue validate` and then continue normal work.\n\nExamples:\n  ralph undo\n  ralph undo --list\n  ralph undo --dry-run\n  ralph undo --id undo-20260215073000000000\n\nCheckpoints are created automatically before queue mutations such as:\n  - ralph task mutate / task decompose --write\n  - ralph task done/reject/start/ready/schedule\n  - ralph task edit/field/clone/split\n  - ralph task relate/blocks/mark-duplicate\n  - ralph queue archive/prune/sort/import/repair\n  - ralph queue issue publish/publish-many\n  - ralph task batch operations"
    )]
    Undo(undo::UndoArgs),

    /// Emit a machine-readable CLI specification (JSON) for tooling and legacy clients.
    #[command(name = "cli-spec", alias = "__cli-spec", hide = true)]
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
