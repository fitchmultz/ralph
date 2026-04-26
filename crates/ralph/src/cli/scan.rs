//! `ralph scan` command: Clap types and handler.
//!
//! Purpose:
//! - `ralph scan` command: Clap types and handler.
//!
//! Responsibilities:
//! - Define clap arguments for scan commands.
//! - Dispatch scan execution with resolved runner overrides.
//!
//! Not handled here:
//! - Queue storage and task persistence.
//! - Runner implementation details or model execution.
//! - Config precedence rules beyond loading the current repo config.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Runner overrides are validated by the agent resolution helpers.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{agent, commands::scan as scan_cmd, config};

/// Scan mode determining the focus of the repository scan.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum ScanMode {
    /// General mode: user provides focus prompt without specifying a mode.
    /// Uses task building instructions without maintenance or innovation specific criteria.
    #[default]
    General,
    /// Maintenance mode: find bugs, workflow gaps, design flaws, repo rules violations.
    /// Focused on break-fix maintenance and code hygiene.
    Maintenance,
    /// Innovation mode: find feature gaps, use-case completeness issues, enhancement opportunities.
    /// Focus on new features and strategic additions.
    Innovation,
}

pub fn handle_scan(args: ScanArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd_with_profile(args.profile.as_deref())?;
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
        runner_cli: args.runner_cli.clone(),
    })?;

    // Merge positional prompt and --focus flag: positional takes precedence
    let focus = if !args.prompt.is_empty() {
        args.prompt.join(" ")
    } else {
        args.focus.clone()
    };

    // Determine the effective scan mode
    let mode = match (args.mode, focus.trim().is_empty()) {
        // No mode specified and no focus prompt → show help/error
        (None, true) => {
            return Err(anyhow::anyhow!(
                "Please provide one of:\n\
                 • A focus prompt: ralph scan \"your focus here\"\n\
                 • A scan mode: ralph scan --mode maintenance\n\
                 • Both: ralph scan --mode innovation \"your focus here\"\n\n\
                 Run 'ralph scan --help' for more information."
            ));
        }
        // Mode specified → use that mode
        (Some(mode), _) => mode,
        // No mode specified but focus prompt provided → use General mode
        (None, false) => ScanMode::General,
    };

    scan_cmd::run_scan(
        &resolved,
        scan_cmd::ScanOptions {
            focus,
            mode,
            runner_override: overrides.runner,
            model_override: overrides.model,
            reasoning_effort_override: overrides.reasoning_effort,
            runner_cli_overrides: overrides.runner_cli,
            force,
            repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, &resolved),
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            lock_mode: if force {
                scan_cmd::ScanLockMode::Held
            } else {
                scan_cmd::ScanLockMode::Acquire
            },
            output_handler: None,
            revert_prompt: None,
        },
    )
}

#[derive(Args)]
#[command(
    about = "Scan repository for new tasks and focus areas",
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n  - Use --profile to apply a configured profile from `.ralph/config.jsonc` or `~/.config/ralph/config.jsonc`.\n\nRunner CLI options:\n  - Override approval/sandbox/verbosity/plan-mode via flags.\n  - Unsupported options follow --unsupported-option-policy.\n\nProfile precedence:\n  - CLI flags > task.agent > selected profile > base config\n\nSafety:\n  - Clean-repo checks allow changes to `.ralph/queue.jsonc`, `.ralph/done.jsonc`, and `.ralph/config.jsonc`.\n  - Use `--force` to bypass the clean-repo check entirely if needed.\n\nExamples:\n  ralph scan \"production readiness gaps\"                              # General mode with focus prompt\n  ralph scan --focus \"production readiness gaps\"                     # General mode with --focus flag\n  ralph scan --mode maintenance \"security audit\"                     # Maintenance mode with focus\n  ralph scan --mode maintenance                                        # Maintenance mode without focus\n  ralph scan --mode innovation \"feature gaps for CLI\"                # Innovation mode with focus\n  ralph scan --mode innovation                                         # Innovation mode without focus\n  ralph scan -m innovation \"enhancement opportunities\"               # Short flag for mode\n  ralph scan --profile deep-review \"queue correctness audit\"         # Use a configured custom profile\n  ralph scan --profile fast-local \"small cleanup opportunities\"      # Use a configured custom profile\n  ralph scan --runner opencode --model gpt-5.3 \"CI and safety gaps\"  # With runner overrides\n  ralph scan --runner gemini --model gemini-3-flash-preview \"risk audit\"\n  ralph scan --runner codex --model gpt-5.4 --effort high \"queue correctness\"\n  ralph scan --approval-mode auto-edits --runner claude \"auto edits review\"\n  ralph scan --sandbox disabled --runner codex \"sandbox audit\"\n  ralph scan --repo-prompt plan \"Deep codebase analysis\"\n  ralph scan --repo-prompt off \"Quick surface scan\"\n  ralph scan --runner kimi \"risk audit\"\n  ralph scan --runner pi \"risk audit\""
)]
pub struct ScanArgs {
    /// Optional focus prompt as positional argument (alternative to --focus).
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,

    /// Optional focus prompt to guide the scan.
    #[arg(long, default_value = "")]
    pub focus: String,

    /// Scan mode: maintenance for code hygiene and bug finding,
    /// innovation for feature discovery and enhancement opportunities,
    /// general (default) when only focus prompt is provided.
    #[arg(short = 'm', long, value_enum)]
    pub mode: Option<ScanMode>,

    /// Named configuration profile to apply before resolving CLI overrides.
    /// Examples: fast-local, deep-review, quick-fix
    #[arg(long, value_name = "NAME")]
    pub profile: Option<String>,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: agent::RunnerCliArgs,
}

#[cfg(test)]
mod tests;
