//! `ralph scan` command: Clap types and handler.

use anyhow::Result;
use clap::Args;

use crate::{agent, commands::scan as scan_cmd, config, runner};

pub fn handle_scan(args: ScanArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        rp_on: args.rp_on,
        rp_off: args.rp_off,
    })?;
    let settings = runner::resolve_agent_settings(
        overrides.runner,
        overrides.model,
        overrides.reasoning_effort,
        None,
        &resolved.config.agent,
    )?;

    scan_cmd::run_scan(
        &resolved,
        scan_cmd::ScanOptions {
            focus: args.focus,
            runner: settings.runner,
            model: settings.model,
            reasoning_effort: settings.reasoning_effort,
            force,
            repoprompt_tool_injection: agent::resolve_rp_required(
                args.rp_on,
                args.rp_off,
                &resolved,
            ),
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            lock_mode: scan_cmd::ScanLockMode::Acquire,
            output_handler: None,
            revert_prompt: None,
        },
    )
}

#[derive(Args)]
#[command(
    about = "Scan repository for new tasks and focus areas",
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nSafety:\n  - Clean-repo checks allow changes to `.ralph/queue.json` and `.ralph/done.json` only (not `.ralph/config.json`).\n  - Use `--force` to bypass the clean-repo check (and stale queue locks) entirely if needed.\n\nExamples:\n  ralph scan --focus \"production readiness gaps\"\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI and safety gaps\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner codex --model gpt-5.2-codex --effort high --focus \"queue correctness\"\n  ralph scan --rp-on \"Deep codebase analysis\"\n  ralph scan --rp-off \"Quick surface scan\""
)]
pub struct ScanArgs {
    /// Optional focus prompt to guide the scan.
    #[arg(long, default_value = "")]
    pub focus: String,

    /// Runner to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub runner: Option<String>,

    /// Model to use. CLI flag overrides config defaults (project > global > built-in).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort. CLI flag overrides config defaults (project > global > built-in).
    /// Ignored for opencode and gemini.
    #[arg(long)]
    pub effort: Option<String>,

    /// Force RepoPrompt flags on (planning requirement + tooling reminders).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt flags off (planning requirement + tooling reminders).
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,
}
