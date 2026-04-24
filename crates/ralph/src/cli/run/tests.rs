//! Tests for `crate::cli::run`.
//!
//! Purpose:
//! - Tests for `crate::cli::run`.
//!
//! Responsibilities:
//! - Verify help text and clap parsing for run-related command surfaces.
//!
//! Not handled here:
//! - Runtime execution behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests exercise the public CLI parser rather than internal helpers.

use std::path::PathBuf;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, run::RunCommand};

#[test]
fn run_one_help_includes_phase_semantics() {
    let mut cmd = Cli::command();
    let run = cmd.find_subcommand_mut("run").expect("run subcommand");
    let run_one = run.find_subcommand_mut("one").expect("run one subcommand");
    let help = run_one.render_long_help().to_string();

    assert!(help.contains("ralph run one --phases 3 (plan/implement+CI/review+complete)"));
    assert!(help.contains("ralph run one --phases 2 (plan/implement)"));
    assert!(help.contains("ralph run one --phases 1 (single-pass)"));
    assert!(help.contains("ralph run one --quick (single-pass, same as --phases 1)"));
}

#[test]
fn run_loop_help_mentions_repo_prompt_examples() {
    let mut cmd = Cli::command();
    let run = cmd.find_subcommand_mut("run").expect("run subcommand");
    let run_loop = run
        .find_subcommand_mut("loop")
        .expect("run loop subcommand");
    let help = run_loop.render_long_help().to_string();

    assert!(help.contains("ralph run loop --repo-prompt tools --max-tasks 1"));
    assert!(help.contains("ralph run loop --repo-prompt off --max-tasks 1"));
}

#[test]
fn run_one_non_interactive_parses() {
    let cli = Cli::parse_from(["ralph", "run", "one", "--non-interactive"]);
    match cli.command {
        crate::cli::Command::Run(run_args) => match run_args.command {
            RunCommand::One(one_args) => assert!(one_args.non_interactive),
            _ => panic!("expected RunCommand::One"),
        },
        _ => panic!("expected Command::Run"),
    }
}

#[test]
fn run_one_non_interactive_with_id_parses() {
    let cli = Cli::parse_from([
        "ralph",
        "run",
        "one",
        "--non-interactive",
        "--id",
        "RQ-0001",
    ]);
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
    let cli = Cli::parse_from(["ralph", "run", "one", "--dry-run"]);
    match cli.command {
        crate::cli::Command::Run(run_args) => match run_args.command {
            RunCommand::One(one_args) => assert!(one_args.dry_run),
            _ => panic!("expected RunCommand::One"),
        },
        _ => panic!("expected Command::Run"),
    }
}

#[test]
fn run_one_dry_run_with_id_parses() {
    let cli = Cli::parse_from(["ralph", "run", "one", "--dry-run", "--id", "RQ-0001"]);
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
    let cli = Cli::parse_from(["ralph", "run", "loop", "--dry-run"]);
    match cli.command {
        crate::cli::Command::Run(run_args) => match run_args.command {
            RunCommand::Loop(loop_args) => assert!(loop_args.dry_run),
            _ => panic!("expected RunCommand::Loop"),
        },
        _ => panic!("expected Command::Run"),
    }
}

#[test]
fn run_loop_dry_run_conflicts_with_parallel() {
    assert!(Cli::try_parse_from(["ralph", "run", "loop", "--dry-run", "--parallel"]).is_err());
}

#[test]
fn run_one_help_includes_dry_run_examples() {
    let mut cmd = Cli::command();
    let run = cmd.find_subcommand_mut("run").expect("run subcommand");
    let run_one = run.find_subcommand_mut("one").expect("run one subcommand");
    let help = run_one.render_long_help().to_string();

    assert!(help.contains("ralph run one --dry-run"));
    assert!(help.contains("ralph run one --dry-run --include-draft"));
    assert!(help.contains("ralph run one --dry-run --id RQ-0001"));
}

#[test]
fn run_loop_help_includes_dry_run_examples() {
    let mut cmd = Cli::command();
    let run = cmd.find_subcommand_mut("run").expect("run subcommand");
    let run_loop = run
        .find_subcommand_mut("loop")
        .expect("run loop subcommand");
    let help = run_loop.render_long_help().to_string();

    assert!(help.contains("ralph run loop --dry-run"));
}

#[test]
fn run_loop_wait_poll_ms_rejects_below_minimum() {
    assert!(Cli::try_parse_from(["ralph", "run", "loop", "--wait-poll-ms", "10"]).is_err());
}

#[test]
fn run_loop_empty_poll_ms_rejects_below_minimum() {
    assert!(Cli::try_parse_from(["ralph", "run", "loop", "--empty-poll-ms", "10"]).is_err());
}

#[test]
fn run_loop_wait_poll_ms_accepts_minimum() {
    let cli = Cli::try_parse_from(["ralph", "run", "loop", "--wait-poll-ms", "50"]).unwrap();
    match cli.command {
        crate::cli::Command::Run(run_args) => match run_args.command {
            RunCommand::Loop(loop_args) => assert_eq!(loop_args.wait_poll_ms, 50),
            _ => panic!("expected RunCommand::Loop"),
        },
        _ => panic!("expected Command::Run"),
    }
}

#[test]
fn run_one_parallel_worker_with_coordinator_paths_parses() {
    let cli = Cli::parse_from([
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
    ]);

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
    assert!(
        Cli::try_parse_from([
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001"
        ])
        .is_err()
    );
}

#[test]
fn run_one_parallel_worker_requires_both_coordinator_paths() {
    assert!(
        Cli::try_parse_from([
            "ralph",
            "run",
            "one",
            "--parallel-worker",
            "--id",
            "RQ-0001",
            "--coordinator-queue-path",
            "/path/to/queue.json",
        ])
        .is_err()
    );
}

#[test]
fn run_one_parallel_worker_requires_target_branch() {
    assert!(
        Cli::try_parse_from([
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
        ])
        .is_err()
    );
}
