//! CLI argument structs for agent configuration.
//!
//! Purpose:
//! - CLI argument structs for agent configuration.
//!
//! Responsibilities:
//! - Define CLI argument structs with clap derive macros for agent-related flags.
//! - Keep fields as raw CLI shapes (Strings, bools, Options).
//!
//! Not handled here:
//! - Parsing or validation logic (see `super::parse`).
//! - Override resolution (see `super::resolve`).
//! - RepoPrompt flag resolution (see `super::repoprompt`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All structs use `clap::Args` derive for CLI integration.
//! - `RepoPromptMode` is imported from the repoprompt module.

use clap::Args;

use super::repoprompt::RepoPromptMode;

/// CLI arguments for runner CLI overrides.
#[derive(Args, Clone, Debug, Default)]
pub struct RunnerCliArgs {
    /// Desired runner output format (stream-json, json, text). Ralph execution requires stream-json.
    #[arg(long)]
    pub output_format: Option<String>,

    /// Desired verbosity (quiet, normal, verbose). Only some runners support this.
    #[arg(long)]
    pub verbosity: Option<String>,

    /// Desired approval mode (default, auto-edits, yolo, safe).
    #[arg(long)]
    pub approval_mode: Option<String>,

    /// Desired sandbox mode (default, enabled, disabled). Only some runners support this.
    #[arg(long)]
    pub sandbox: Option<String>,

    /// Desired plan/read-only mode (default, enabled, disabled). Only Cursor currently supports this.
    #[arg(long)]
    pub plan_mode: Option<String>,

    /// Policy for unsupported options (ignore, warn, error). Default is warn.
    #[arg(long)]
    pub unsupported_option_policy: Option<String>,
}

/// CLI arguments for agent configuration.
///
/// Used by `task` and `scan` commands.
#[derive(Args, Clone, Debug, Default)]
pub struct AgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini, claude, cursor, kimi, pi).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.4, gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding
    /// (codex supports only gpt-5.4/gpt-5.3-codex/gpt-5.3-codex-spark/gpt-5.3; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (low, medium, high, xhigh).
    /// Ignored for other runners.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: RunnerCliArgs,
}

/// Extended agent arguments for run commands (includes phases).
#[derive(Args, Clone, Debug, Default)]
pub struct RunAgentArgs {
    /// Named configuration profile to apply before resolving task/CLI overrides.
    /// Examples: fast-local, deep-review, quick-fix
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Runner override for this invocation (codex, opencode, gemini, claude, cursor, kimi, pi).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.4, gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding
    /// (codex supports only gpt-5.4/gpt-5.3-codex/gpt-5.3-codex-spark/gpt-5.3; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (low, medium, high, xhigh).
    /// Ignored for other runners.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    #[command(flatten)]
    pub runner_cli: RunnerCliArgs,

    /// Execution shape:
    /// - 1 => single-pass execution (no mandated planning step)
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    ///
    /// If omitted, defaults to config `agent.phases`.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    pub phases: Option<u8>,

    /// Quick mode: skip planning phase and run single-pass execution.
    ///
    /// Equivalent to --phases=1. Cannot be used with --phases.
    #[arg(long, conflicts_with = "phases")]
    pub quick: bool,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<RepoPromptMode>,

    /// Git revert mode for automatic error handling (ask, enabled, disabled).
    #[arg(long, value_parser = ["ask", "enabled", "disabled"])]
    pub git_revert_mode: Option<String>,

    /// Post-run git publication mode (off, commit, commit_and_push).
    #[arg(
        long,
        value_name = "MODE",
        value_parser = ["off", "commit", "commit_and_push"]
    )]
    pub git_publish_mode: Option<String>,

    /// Include draft tasks when selecting what to run.
    #[arg(long)]
    pub include_draft: bool,

    /// Enable desktop notification on task completion (overrides config).
    #[arg(long, conflicts_with = "no_notify")]
    pub notify: bool,

    /// Disable desktop notification on task completion (overrides config).
    #[arg(long, conflicts_with = "notify")]
    pub no_notify: bool,

    /// Enable desktop notification on task failure (overrides config).
    #[arg(long, conflicts_with = "no_notify_fail")]
    pub notify_fail: bool,

    /// Disable desktop notification on task failure (overrides config).
    #[arg(long, conflicts_with = "notify_fail")]
    pub no_notify_fail: bool,

    /// Enable sound alert with notification (requires --notify or config enabled).
    #[arg(long)]
    pub notify_sound: bool,

    /// Enable strict LFS validation before commit (fail if filters misconfigured).
    #[arg(long)]
    pub lfs_check: bool,

    /// Disable progress indicators and celebrations.
    #[arg(long)]
    pub no_progress: bool,

    // Phase 1 overrides
    /// Runner override for Phase 1 (planning).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase1: Option<String>,

    /// Model override for Phase 1 (planning).
    #[arg(long, value_name = "MODEL")]
    pub model_phase1: Option<String>,

    /// Reasoning effort override for Phase 1 (planning).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase1: Option<String>,

    // Phase 2 overrides
    /// Runner override for Phase 2 (implementation).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase2: Option<String>,

    /// Model override for Phase 2 (implementation).
    #[arg(long, value_name = "MODEL")]
    pub model_phase2: Option<String>,

    /// Reasoning effort override for Phase 2 (implementation).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase2: Option<String>,

    // Phase 3 overrides
    /// Runner override for Phase 3 (review).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase3: Option<String>,

    /// Model override for Phase 3 (review).
    #[arg(long, value_name = "MODEL")]
    pub model_phase3: Option<String>,

    /// Reasoning effort override for Phase 3 (review).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase3: Option<String>,
}
