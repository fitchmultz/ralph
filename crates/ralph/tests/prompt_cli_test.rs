//! CLI argument parsing tests for `ralph prompt` subcommands.
//!
//! Responsibilities:
//! - Verify all prompt subcommand flags parse correctly
//! - Validate argument conflicts are rejected
//! - Test invalid value handling
//!
//! This file tests ONLY CLI parsing. For prompt content/rendering tests,
//! see `prompt_cmd_test.rs`.

use clap::Parser;

use ralph::agent::RepoPromptMode;
use ralph::cli::scan::ScanMode;
use ralph::cli::{Cli, Command, prompt::PromptCommand};
use ralph::promptflow::RunPhase;

// =============================================================================
// Worker Subcommand Tests
// =============================================================================

#[test]
fn prompt_worker_parses_phase_1() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "1"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.phase, Some(RunPhase::Phase1));
                assert!(!w.single);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_phase_2() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "2"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.phase, Some(RunPhase::Phase2));
                assert!(!w.single);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_phase_3() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "3"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.phase, Some(RunPhase::Phase3));
                assert!(!w.single);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_single() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--single"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert!(w.single);
                assert!(w.phase.is_none());
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_single_conflicts_with_phase() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--single", "--phase", "1"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("conflict") || msg.contains("cannot be used"),
        "unexpected error: {msg}"
    );
}

#[test]
fn prompt_worker_parses_task_id() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "worker", "--task-id", "RQ-0001"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.task_id.as_deref(), Some("RQ-0001"));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_plan_file() {
    let cli = Cli::try_parse_from([
        "ralph",
        "prompt",
        "worker",
        "--plan-file",
        "/path/to/plan.md",
    ])
    .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(
                    w.plan_file,
                    Some(std::path::PathBuf::from("/path/to/plan.md"))
                );
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_plan_text() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--plan-text", "my plan"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.plan_text.as_deref(), Some("my plan"));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_iterations() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "worker", "--iterations", "3"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.iterations, 3);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_iteration_index() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--iteration-index", "2"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.iteration_index, 2);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_repo_prompt_tools() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--repo-prompt", "tools"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.repo_prompt, Some(RepoPromptMode::Tools));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_repo_prompt_plan() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "worker", "--repo-prompt", "plan"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.repo_prompt, Some(RepoPromptMode::Plan));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_repo_prompt_off() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "worker", "--repo-prompt", "off"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.repo_prompt, Some(RepoPromptMode::Off));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_explain() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "worker", "--explain"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert!(w.explain);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

// =============================================================================
// Invalid Phase Tests
// =============================================================================

#[test]
fn prompt_worker_rejects_phase_0() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "0"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
}

#[test]
fn prompt_worker_rejects_phase_4() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "4"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
}

#[test]
fn prompt_worker_rejects_phase_5() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "5"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
}

#[test]
fn prompt_worker_rejects_phase_word() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "one"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
}

// =============================================================================
// Scan Subcommand Tests
// =============================================================================

#[test]
fn prompt_scan_parses_focus() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "scan", "--focus", "CI gaps"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert_eq!(s.focus, "CI gaps");
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_scan_parses_mode_maintenance() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "scan", "--mode", "maintenance"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert_eq!(s.mode, ScanMode::Maintenance);
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_scan_parses_mode_innovation() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "scan", "--mode", "innovation"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert_eq!(s.mode, ScanMode::Innovation);
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_scan_parses_repo_prompt_tools() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "scan", "--repo-prompt", "tools"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert_eq!(s.repo_prompt, Some(RepoPromptMode::Tools));
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_scan_parses_explain() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "scan", "--explain"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert!(s.explain);
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

// =============================================================================
// Task-Builder Subcommand Tests
// =============================================================================

#[test]
fn prompt_task_builder_parses_request() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "task-builder", "--request", "Add tests"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert_eq!(t.request.as_deref(), Some("Add tests"));
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_task_builder_parses_tags() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "task-builder", "--tags", "rust,tests"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert_eq!(t.tags, "rust,tests");
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_task_builder_parses_scope() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "task-builder", "--scope", "crates/ralph"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert_eq!(t.scope, "crates/ralph");
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_task_builder_parses_repo_prompt_plan() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "task-builder", "--repo-prompt", "plan"])
        .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert_eq!(t.repo_prompt, Some(RepoPromptMode::Plan));
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_task_builder_parses_explain() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "task-builder", "--explain"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert!(t.explain);
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}

// =============================================================================
// Management Command Tests
// =============================================================================

#[test]
fn prompt_list_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "list"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::List => {}
            _ => panic!("expected list command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_show_parses_name() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "show", "worker"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Show(s) => {
                assert_eq!(s.name, "worker");
                assert!(!s.raw);
            }
            _ => panic!("expected show command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_show_parses_raw() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "show", "worker", "--raw"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Show(s) => {
                assert_eq!(s.name, "worker");
                assert!(s.raw);
            }
            _ => panic!("expected show command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_export_parses_all() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "export", "--all"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Export(e) => {
                assert!(e.all);
                assert!(e.name.is_none());
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_export_parses_name() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "export", "worker"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Export(e) => {
                assert_eq!(e.name.as_deref(), Some("worker"));
                assert!(!e.all);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_export_parses_force() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "export", "worker", "--force"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Export(e) => {
                assert_eq!(e.name.as_deref(), Some("worker"));
                assert!(e.force);
            }
            _ => panic!("expected export command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_sync_parses_dry_run() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "sync", "--dry-run"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Sync(s) => {
                assert!(s.dry_run);
                assert!(!s.force);
            }
            _ => panic!("expected sync command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_sync_parses_force() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "sync", "--force"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Sync(s) => {
                assert!(!s.dry_run);
                assert!(s.force);
            }
            _ => panic!("expected sync command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_sync_parses_dry_run_and_force() {
    let cli =
        Cli::try_parse_from(["ralph", "prompt", "sync", "--dry-run", "--force"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Sync(s) => {
                assert!(s.dry_run);
                assert!(s.force);
            }
            _ => panic!("expected sync command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_diff_parses_name() {
    let cli = Cli::try_parse_from(["ralph", "prompt", "diff", "worker"]).expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Diff(d) => {
                assert_eq!(d.name, "worker");
            }
            _ => panic!("expected diff command"),
        },
        _ => panic!("expected prompt command"),
    }
}

// =============================================================================
// Combination Tests
// =============================================================================

#[test]
fn prompt_worker_parses_full_combination() {
    let cli = Cli::try_parse_from([
        "ralph",
        "prompt",
        "worker",
        "--phase",
        "2",
        "--task-id",
        "RQ-0001",
        "--plan-file",
        "/path/to/plan.md",
        "--iterations",
        "3",
        "--iteration-index",
        "2",
        "--repo-prompt",
        "plan",
        "--explain",
    ])
    .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert_eq!(w.phase, Some(RunPhase::Phase2));
                assert_eq!(w.task_id.as_deref(), Some("RQ-0001"));
                assert_eq!(
                    w.plan_file,
                    Some(std::path::PathBuf::from("/path/to/plan.md"))
                );
                assert_eq!(w.iterations, 3);
                assert_eq!(w.iteration_index, 2);
                assert_eq!(w.repo_prompt, Some(RepoPromptMode::Plan));
                assert!(w.explain);
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_worker_parses_single_with_other_flags() {
    let cli = Cli::try_parse_from([
        "ralph",
        "prompt",
        "worker",
        "--single",
        "--task-id",
        "RQ-0001",
        "--repo-prompt",
        "tools",
    ])
    .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Worker(w) => {
                assert!(w.single);
                assert!(w.phase.is_none());
                assert_eq!(w.task_id.as_deref(), Some("RQ-0001"));
                assert_eq!(w.repo_prompt, Some(RepoPromptMode::Tools));
            }
            _ => panic!("expected worker command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_scan_parses_full_combination() {
    let cli = Cli::try_parse_from([
        "ralph",
        "prompt",
        "scan",
        "--focus",
        "security audit",
        "--mode",
        "maintenance",
        "--repo-prompt",
        "off",
        "--explain",
    ])
    .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::Scan(s) => {
                assert_eq!(s.focus, "security audit");
                assert_eq!(s.mode, ScanMode::Maintenance);
                assert_eq!(s.repo_prompt, Some(RepoPromptMode::Off));
                assert!(s.explain);
            }
            _ => panic!("expected scan command"),
        },
        _ => panic!("expected prompt command"),
    }
}

#[test]
fn prompt_task_builder_parses_full_combination() {
    let cli = Cli::try_parse_from([
        "ralph",
        "prompt",
        "task-builder",
        "--request",
        "Add comprehensive tests",
        "--tags",
        "rust,testing,cli",
        "--scope",
        "crates/ralph/src/cli",
        "--repo-prompt",
        "tools",
        "--explain",
    ])
    .expect("parse");
    match cli.command {
        Command::Prompt(args) => match args.command {
            PromptCommand::TaskBuilder(t) => {
                assert_eq!(t.request.as_deref(), Some("Add comprehensive tests"));
                assert_eq!(t.tags, "rust,testing,cli");
                assert_eq!(t.scope, "crates/ralph/src/cli");
                assert_eq!(t.repo_prompt, Some(RepoPromptMode::Tools));
                assert!(t.explain);
            }
            _ => panic!("expected task-builder command"),
        },
        _ => panic!("expected prompt command"),
    }
}
