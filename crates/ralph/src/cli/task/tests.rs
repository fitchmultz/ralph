//! Task CLI parsing and help tests grouped by command surface.
//!
//! Purpose:
//! - Task CLI parsing and help tests grouped by command surface.
//!
//! Responsibilities:
//! - Cover representative parsing/help regressions for the task command facade.
//! - Keep the production facade free of large inline clap scenario blocks.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use clap::{CommandFactory, Parser};

use crate::cli::Cli;
use crate::cli::queue::QueueShowFormat;
use crate::cli::task::args::{BatchOperation, TaskEditFieldArg, TaskStatusArg};

#[test]
fn task_update_help_mentions_rp_examples() {
    let mut cmd = Cli::command();
    let task = cmd.find_subcommand_mut("task").expect("task subcommand");
    let update = task
        .find_subcommand_mut("update")
        .expect("task update subcommand");
    let help = update.render_long_help().to_string();

    assert!(help.contains("ralph task update --repo-prompt plan RQ-0001"));
    assert!(help.contains("ralph task update --repo-prompt off --fields scope,evidence RQ-0001"));
    assert!(help.contains("ralph task update --approval-mode auto-edits --runner claude RQ-0001"));
}

#[test]
fn task_show_help_mentions_examples() {
    let mut cmd = Cli::command();
    let task = cmd.find_subcommand_mut("task").expect("task subcommand");
    let show = task
        .find_subcommand_mut("show")
        .expect("task show subcommand");
    let help = show.render_long_help().to_string();

    assert!(help.contains("ralph task show RQ-0001"));
    assert!(help.contains("--format compact"));
}

#[test]
fn task_details_alias_parses() {
    let cli = Cli::try_parse_from(["ralph", "task", "details", "RQ-0001", "--format", "compact"])
        .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Show(args)) => {
                assert_eq!(args.task_id, "RQ-0001");
                assert_eq!(args.format, QueueShowFormat::Compact);
            }
            _ => panic!("expected task show command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_build_parses_repo_prompt_and_runner_cli_overrides() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "build",
        "--repo-prompt",
        "plan",
        "-e",
        "high",
        "--approval-mode",
        "yolo",
        "--sandbox",
        "disabled",
        "Add tests",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Build(args)) => {
                assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Plan));
                assert_eq!(args.effort.as_deref(), Some("high"));
                assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("yolo"));
                assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
            }
            _ => panic!("expected task build command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_decompose_parses_preview_runner_overrides_and_limits() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "decompose",
        "--preview",
        "--attach-to",
        "RQ-0042",
        "--child-policy",
        "append",
        "--with-dependencies",
        "--format",
        "json",
        "--max-depth",
        "4",
        "--max-children",
        "6",
        "--max-nodes",
        "24",
        "--runner",
        "codex",
        "--model",
        "gpt-5.4",
        "-e",
        "high",
        "--repo-prompt",
        "tools",
        "--approval-mode",
        "auto-edits",
        "RQ-0001",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Decompose(args)) => {
                assert!(args.preview);
                assert_eq!(args.attach_to.as_deref(), Some("RQ-0042"));
                assert_eq!(
                    args.child_policy,
                    crate::cli::task::TaskDecomposeChildPolicyArg::Append
                );
                assert!(args.with_dependencies);
                assert_eq!(args.format, crate::cli::task::TaskDecomposeFormatArg::Json);
                assert_eq!(args.max_depth, 4);
                assert_eq!(args.max_children, 6);
                assert_eq!(args.max_nodes, 24);
                assert_eq!(args.runner.as_deref(), Some("codex"));
                assert_eq!(args.model.as_deref(), Some("gpt-5.4"));
                assert_eq!(args.effort.as_deref(), Some("high"));
                assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Tools));
                assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
            }
            _ => panic!("expected task decompose command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_decompose_help_mentions_write_and_attach_examples() {
    let mut cmd = Cli::command();
    let task = cmd.find_subcommand_mut("task").expect("task subcommand");
    let decompose = task
        .find_subcommand_mut("decompose")
        .expect("task decompose subcommand");
    let help = decompose.render_long_help().to_string();

    assert!(help.contains("Improve webhook reliability\" --write"));
    assert!(help.contains("--attach-to RQ-0042"));
    assert!(help.contains("--format json"));
}

#[test]
fn task_followups_apply_parses_source_input_dry_run_and_format() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "followups",
        "apply",
        "--task",
        "RQ-0135",
        "--input",
        "/tmp/followups.json",
        "--dry-run",
        "--format",
        "json",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Followups(args)) => match args.command {
                crate::cli::task::TaskFollowupsCommand::Apply(args) => {
                    assert_eq!(args.task, "RQ-0135");
                    assert_eq!(
                        args.input.as_deref(),
                        Some(std::path::Path::new("/tmp/followups.json"))
                    );
                    assert!(args.dry_run);
                    assert_eq!(args.format, crate::cli::task::TaskFollowupsFormatArg::Json);
                }
            },
            _ => panic!("expected task followups apply command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_update_and_edit_parse_dry_run_and_runner_overrides() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "update",
        "--dry-run",
        "--repo-prompt",
        "off",
        "-e",
        "low",
        "--approval-mode",
        "auto-edits",
        "--sandbox",
        "disabled",
        "RQ-0001",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Update(args)) => {
                assert!(args.dry_run);
                assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Off));
                assert_eq!(args.effort.as_deref(), Some("low"));
                assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
                assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
            }
            _ => panic!("expected task update command"),
        },
        _ => panic!("expected task command"),
    }

    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "edit",
        "--dry-run",
        "priority",
        "high",
        "RQ-0001",
        "RQ-0002",
    ])
    .expect("parse");

    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Edit(args)) => {
                assert!(args.dry_run);
                assert_eq!(args.field, TaskEditFieldArg::Priority);
                assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002"]);
            }
            _ => panic!("expected task edit command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_refactor_aliases_parse() {
    let cli = Cli::try_parse_from(["ralph", "task", "ref", "--threshold", "800"]).expect("parse");
    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Refactor(args)) => assert_eq!(args.threshold, 800),
            _ => panic!("expected task refactor command via alias"),
        },
        _ => panic!("expected task command"),
    }

    let cli = Cli::try_parse_from(["ralph", "task", "build-refactor", "--threshold", "700"])
        .expect("parse");
    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::BuildRefactor(args)) => {
                assert_eq!(args.threshold, 700)
            }
            _ => panic!("expected task build-refactor command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn task_clone_parses_flags_and_help_examples() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "clone",
        "--status",
        "todo",
        "--title-prefix",
        "[Follow-up] ",
        "--dry-run",
        "RQ-0001",
    ])
    .expect("parse");
    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Clone(args)) => {
                assert_eq!(args.task_id, "RQ-0001");
                assert_eq!(args.status, Some(TaskStatusArg::Todo));
                assert_eq!(args.title_prefix, Some("[Follow-up] ".to_string()));
                assert!(args.dry_run);
            }
            _ => panic!("expected task clone command"),
        },
        _ => panic!("expected task command"),
    }

    let mut cmd = Cli::command();
    let task = cmd.find_subcommand_mut("task").expect("task subcommand");
    let clone = task
        .find_subcommand_mut("clone")
        .expect("task clone subcommand");
    let help = clone.render_long_help().to_string();
    assert!(help.contains("ralph task clone RQ-0001"));
    assert!(help.contains("--status"));
    assert!(help.contains("--title-prefix"));
    assert!(help.contains("ralph task duplicate"));
}

#[test]
fn task_batch_parses_status_and_edit_modes() {
    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "batch",
        "--continue-on-error",
        "status",
        "doing",
        "RQ-0001",
        "RQ-0002",
    ])
    .expect("parse");
    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Batch(args)) => {
                assert!(args.continue_on_error);
                match args.operation {
                    BatchOperation::Status(status_args) => {
                        assert_eq!(status_args.status, TaskStatusArg::Doing);
                        assert_eq!(status_args.select.task_ids, vec!["RQ-0001", "RQ-0002"]);
                    }
                    _ => panic!("expected batch status operation"),
                }
            }
            _ => panic!("expected task batch command"),
        },
        _ => panic!("expected task command"),
    }

    let cli = Cli::try_parse_from([
        "ralph",
        "task",
        "batch",
        "--dry-run",
        "edit",
        "priority",
        "high",
        "RQ-0001",
        "RQ-0002",
    ])
    .expect("parse");
    match cli.command {
        crate::cli::Command::Task(args) => match args.command {
            Some(crate::cli::task::TaskCommand::Batch(args)) => {
                assert!(args.dry_run);
                match args.operation {
                    BatchOperation::Edit(edit_args) => {
                        assert_eq!(edit_args.field, TaskEditFieldArg::Priority);
                        assert_eq!(edit_args.select.task_ids, vec!["RQ-0001", "RQ-0002"]);
                    }
                    _ => panic!("expected batch edit operation"),
                }
            }
            _ => panic!("expected task batch command"),
        },
        _ => panic!("expected task command"),
    }
}
