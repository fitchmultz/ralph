//! `ralph scan` command: Clap types and handler.
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
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Runner overrides are validated by the agent resolution helpers.

use anyhow::Result;
use clap::Args;

use crate::{agent, commands::scan as scan_cmd, config, runner};

pub fn handle_scan(args: ScanArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    let overrides = agent::resolve_agent_overrides(&agent::AgentArgs {
        runner: args.runner.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        repo_prompt: args.repo_prompt,
    })?;
    let settings = runner::resolve_agent_settings(
        overrides.runner,
        overrides.model,
        overrides.reasoning_effort,
        &overrides.runner_cli,
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
            repoprompt_tool_injection: agent::resolve_rp_required(args.repo_prompt, &resolved),
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
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nSafety:\n  - Clean-repo checks allow changes to `.ralph/queue.json` and `.ralph/done.json` only (not `.ralph/config.json`).\n  - Use `--force` to bypass the clean-repo check (and stale queue locks) entirely if needed.\n\nExamples:\n  ralph scan --focus \"production readiness gaps\"\n  ralph scan --runner opencode --model gpt-5.2 --focus \"CI and safety gaps\"\n  ralph scan --runner gemini --model gemini-3-flash-preview --focus \"risk audit\"\n  ralph scan --runner codex --model gpt-5.2-codex --effort high --focus \"queue correctness\"\n  ralph scan --repo-prompt plan --focus \"Deep codebase analysis\"\n  ralph scan --repo-prompt off --focus \"Quick surface scan\""
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
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<agent::RepoPromptMode>,
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn scan_help_examples_include_repo_prompt_focus() {
        let mut cmd = Cli::command();
        let scan = cmd.find_subcommand_mut("scan").expect("scan subcommand");
        let help = scan.render_long_help().to_string();

        assert!(
            help.contains("--repo-prompt plan --focus \"Deep codebase analysis\""),
            "missing repo-prompt plan example: {help}"
        );
        assert!(
            help.contains("--repo-prompt off --focus \"Quick surface scan\""),
            "missing repo-prompt off example: {help}"
        );
    }

    #[test]
    fn scan_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from(["ralph", "scan", "--repo-prompt", "tools", "-e", "high"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Tools));
                assert_eq!(args.effort.as_deref(), Some("high"));
            }
            _ => panic!("expected scan command"),
        }
    }
}
