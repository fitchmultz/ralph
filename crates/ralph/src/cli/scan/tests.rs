//! Regression coverage for the `ralph scan` CLI surface.
//!
//! Purpose:
//! - Regression coverage for the `ralph scan` CLI surface.
//!
//! Responsibilities:
//! - Verify help examples stay current.
//! - Verify scan flag parsing and positional-prompt behavior.
//!
//! Not handled here:
//! - Scan execution and queue writes.
//! - Runner integration behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Help output stays aligned with the clap surface.
//! - Explicit and implicit general-mode parsing remain compatible.

use clap::{CommandFactory, Parser};

use super::ScanMode;
use crate::cli::Cli;

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
        help.contains("# General mode with focus prompt"),
        "missing general mode comment: {help}"
    );
    assert!(
        help.contains("# General mode with --focus flag"),
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
    let cli =
        Cli::try_parse_from(["ralph", "scan", "production", "readiness", "gaps"]).expect("parse");

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
        "ralph", "scan", "--runner", "opencode", "--model", "gpt-5.3", "CI", "and", "safety",
        "gaps",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.runner.as_deref(), Some("opencode"));
            assert_eq!(args.model.as_deref(), Some("gpt-5.3"));
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
            assert_eq!(args.mode, Some(ScanMode::Maintenance));
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_parses_mode_innovation() {
    let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "innovation"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::Innovation));
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_parses_mode_general() {
    let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "general"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::General));
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_parses_mode_short_flag() {
    let cli = Cli::try_parse_from(["ralph", "scan", "-m", "innovation"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::Innovation));
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_no_mode_no_focus_requires_input() {
    let cli = Cli::try_parse_from(["ralph", "scan"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, None);
            assert!(args.prompt.is_empty());
            assert!(args.focus.is_empty());
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_focus_only_defaults_to_general_mode() {
    let cli = Cli::try_parse_from(["ralph", "scan", "production", "readiness"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, None);
            assert_eq!(args.prompt, vec!["production", "readiness"]);
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_explicit_maintenance_mode_with_focus() {
    let cli = Cli::try_parse_from([
        "ralph",
        "scan",
        "--mode",
        "maintenance",
        "security",
        "audit",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::Maintenance));
            assert_eq!(args.prompt, vec!["security", "audit"]);
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_explicit_innovation_mode_without_focus() {
    let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "innovation"]).expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::Innovation));
            assert!(args.prompt.is_empty());
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
            assert_eq!(args.mode, Some(ScanMode::Innovation));
            assert_eq!(args.prompt, vec!["feature gaps"]);
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_general_mode_explicit() {
    let cli = Cli::try_parse_from(["ralph", "scan", "--mode", "general", "some", "focus"])
        .expect("parse");

    match cli.command {
        crate::cli::Command::Scan(args) => {
            assert_eq!(args.mode, Some(ScanMode::General));
            assert_eq!(args.prompt, vec!["some", "focus"]);
        }
        _ => panic!("expected scan command"),
    }
}

#[test]
fn scan_explicit_general_mode_equivalent_to_implicit_with_focus() {
    let cli_explicit = Cli::try_parse_from(["ralph", "scan", "--mode", "general", "test", "focus"])
        .expect("parse explicit mode");

    let cli_implicit =
        Cli::try_parse_from(["ralph", "scan", "test", "focus"]).expect("parse implicit mode");

    match (cli_explicit.command, cli_implicit.command) {
        (crate::cli::Command::Scan(args_explicit), crate::cli::Command::Scan(args_implicit)) => {
            assert_eq!(args_explicit.mode, Some(ScanMode::General));
            assert_eq!(args_explicit.prompt, vec!["test", "focus"]);
            assert_eq!(args_implicit.mode, None);
            assert_eq!(args_implicit.prompt, vec!["test", "focus"]);
        }
        _ => panic!("expected scan commands"),
    }
}
