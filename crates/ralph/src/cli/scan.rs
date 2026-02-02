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
use clap::{Args, ValueEnum};

use crate::{agent, commands::scan as scan_cmd, config};

/// Scan mode determining the focus of the repository scan.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum ScanMode {
    /// Maintenance mode: find bugs, workflow gaps, design flaws, repo rules violations.
    /// This is the default mode focused on break-fix maintenance and code hygiene.
    #[default]
    Maintenance,
    /// Innovation mode: find feature gaps, use-case completeness issues, enhancement opportunities.
    /// Focus on new features and strategic additions.
    Innovation,
}

pub fn handle_scan(args: ScanArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
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

    scan_cmd::run_scan(
        &resolved,
        scan_cmd::ScanOptions {
            focus,
            mode: args.mode,
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
            lock_mode: scan_cmd::ScanLockMode::Acquire,
            output_handler: None,
            revert_prompt: None,
        },
    )
}

#[derive(Args)]
#[command(
    about = "Scan repository for new tasks and focus areas",
    after_long_help = "Runner selection:\n  - Override runner/model/effort for this invocation using flags.\n  - Defaults come from config when flags are omitted.\n\nRunner CLI options:\n  - Override approval/sandbox/verbosity/plan-mode via flags.\n  - Unsupported options follow --unsupported-option-policy.\n\nSafety:\n  - Clean-repo checks allow changes to `.ralph/queue.json` and `.ralph/done.json` only (not `.ralph/config.json`).\n  - Use `--force` to bypass the clean-repo check (and stale queue locks) entirely if needed.\n\nExamples:\n  ralph scan\n  ralph scan \"production readiness gaps\"                              # Positional prompt\n  ralph scan --focus \"production readiness gaps\"                     # Flag-based prompt (backward compatible)\n  ralph scan --mode maintenance \"security audit\"                     # Maintenance mode (default)\n  ralph scan --mode innovation \"feature gaps for CLI\"                # Innovation mode\n  ralph scan -m innovation \"enhancement opportunities\"               # Short flag for mode\n  ralph scan --runner opencode --model gpt-5.2 \"CI and safety gaps\"  # With runner overrides\n  ralph scan --runner gemini --model gemini-3-flash-preview \"risk audit\"\n  ralph scan --runner codex --model gpt-5.2-codex --effort high \"queue correctness\"\n  ralph scan --approval-mode auto-edits --runner claude \"auto edits review\"\n  ralph scan --sandbox disabled --runner codex \"sandbox audit\"\n  ralph scan --repo-prompt plan \"Deep codebase analysis\"\n  ralph scan --repo-prompt off \"Quick surface scan\"\n  ralph scan --runner kimi \"risk audit\"\n  ralph scan --runner pi \"risk audit\""
)]
pub struct ScanArgs {
    /// Optional focus prompt as positional argument (alternative to --focus).
    #[arg(value_name = "PROMPT")]
    pub prompt: Vec<String>,

    /// Optional focus prompt to guide the scan.
    #[arg(long, default_value = "")]
    pub focus: String,

    /// Scan mode: maintenance (default) for code hygiene and bug finding,
    /// innovation for feature discovery and enhancement opportunities.
    #[arg(short = 'm', long, value_enum, default_value_t = ScanMode::Maintenance)]
    pub mode: ScanMode,

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
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;
    use crate::cli::scan::ScanMode;

    #[test]
    fn scan_help_examples_include_repo_prompt_focus() {
        let mut cmd = Cli::command();
        let scan = cmd.find_subcommand_mut("scan").expect("scan subcommand");
        let help = scan.render_long_help().to_string();

        assert!(
            help.contains("--repo-prompt plan \"Deep codebase analysis\""),
            "missing repo-prompt plan example: {help}"
        );
        assert!(
            help.contains("--repo-prompt off \"Quick surface scan\""),
            "missing repo-prompt off example: {help}"
        );
    }

    #[test]
    fn scan_help_examples_include_positional_prompt() {
        let mut cmd = Cli::command();
        let scan = cmd.find_subcommand_mut("scan").expect("scan subcommand");
        let help = scan.render_long_help().to_string();

        assert!(
            help.contains("ralph scan \"production readiness gaps\""),
            "missing positional prompt example: {help}"
        );
        assert!(
            help.contains("# Positional prompt"),
            "missing positional prompt comment: {help}"
        );
        assert!(
            help.contains("# Flag-based prompt"),
            "missing flag-based prompt comment: {help}"
        );
    }

    #[test]
    fn scan_help_examples_include_runner_cli_overrides() {
        let mut cmd = Cli::command();
        let scan = cmd.find_subcommand_mut("scan").expect("scan subcommand");
        let help = scan.render_long_help().to_string();

        assert!(
            help.contains("--approval-mode auto-edits --runner claude"),
            "missing approval-mode example: {help}"
        );
        assert!(
            help.contains("--sandbox disabled --runner codex"),
            "missing sandbox example: {help}"
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

    #[test]
    fn scan_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "scan",
            "--approval-mode",
            "auto-edits",
            "--sandbox",
            "disabled",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
                assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_parses_positional_prompt() {
        let cli = Cli::try_parse_from(["ralph", "scan", "production", "readiness", "gaps"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.prompt, vec!["production", "readiness", "gaps"]);
                assert!(args.focus.is_empty());
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_parses_positional_prompt_with_flags() {
        let cli = Cli::try_parse_from([
            "ralph", "scan", "--runner", "opencode", "--model", "gpt-5.2", "CI", "and", "safety",
            "gaps",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.runner.as_deref(), Some("opencode"));
                assert_eq!(args.model.as_deref(), Some("gpt-5.2"));
                assert_eq!(args.prompt, vec!["CI", "and", "safety", "gaps"]);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_backward_compatible_with_focus_flag() {
        let cli = Cli::try_parse_from(["ralph", "scan", "--focus", "production readiness gaps"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.focus, "production readiness gaps");
                assert!(args.prompt.is_empty());
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_positional_takes_precedence_over_focus_flag() {
        // When both positional and --focus are provided, positional takes precedence
        let cli = Cli::try_parse_from([
            "ralph",
            "scan",
            "--focus",
            "flag-based focus",
            "positional",
            "focus",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                // Both should be parsed correctly
                assert_eq!(args.focus, "flag-based focus");
                assert_eq!(args.prompt, vec!["positional", "focus"]);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_parses_mode_maintenance() {
        let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "maintenance"]).expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.mode, ScanMode::Maintenance);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_parses_mode_innovation() {
        let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "innovation"]).expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.mode, ScanMode::Innovation);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_parses_mode_short_flag() {
        let cli = Cli::try_parse_from(["ralph", "scan", "-m", "innovation"]).expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.mode, ScanMode::Innovation);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_default_mode_is_maintenance() {
        // When no --mode flag is provided, default should be maintenance
        let cli = Cli::try_parse_from(["ralph", "scan"]).expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.mode, ScanMode::Maintenance);
            }
            _ => panic!("expected scan command"),
        }
    }

    #[test]
    fn scan_mode_with_positional_prompt() {
        let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "innovation", "feature gaps"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Scan(args) => {
                assert_eq!(args.mode, ScanMode::Innovation);
                assert_eq!(args.prompt, vec!["feature gaps"]);
            }
            _ => panic!("expected scan command"),
        }
    }
}
