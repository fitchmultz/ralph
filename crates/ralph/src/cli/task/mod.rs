//! `ralph task ...` command group: Clap types and handler facade.
//!
//! Responsibilities:
//! - Define clap structures for task-related commands (re-exported from submodules).
//! - Route task subcommands to their specific handlers.
//! - Re-export argument types used by task commands.
//!
//! Not handled here:
//! - Queue persistence and locking semantics (see `crate::queue` and `crate::lock`).
//! - Task execution or runner behavior.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory.
//! - Task state changes occur within the subcommand handlers.

mod args;
mod batch;
mod build;
mod clone;
mod edit;
mod refactor;
mod relations;
mod schedule;
mod show;
mod split;
mod status;
mod template;

use anyhow::Result;

use crate::config;

// Re-export all argument types for backward compatibility
pub use args::{
    BatchEditArgs, BatchFieldArgs, BatchMode, BatchOperation, BatchStatusArgs, TaskArgs,
    TaskBatchArgs, TaskBlocksArgs, TaskBuildArgs, TaskBuildRefactorArgs, TaskCloneArgs,
    TaskCommand, TaskDoneArgs, TaskEditArgs, TaskEditFieldArg, TaskFieldArgs,
    TaskMarkDuplicateArgs, TaskReadyArgs, TaskRejectArgs, TaskRelateArgs, TaskScheduleArgs,
    TaskShowArgs, TaskSplitArgs, TaskStatusArg, TaskStatusArgs, TaskTemplateArgs,
    TaskTemplateBuildArgs, TaskTemplateCommand, TaskTemplateShowArgs, TaskUpdateArgs,
};

/// Main entry point for task commands.
pub fn handle_task(args: TaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        Some(TaskCommand::Ready(args)) => status::handle_ready(&args, force, &resolved),
        Some(TaskCommand::Status(args)) => status::handle_status(&args, force, &resolved),
        Some(TaskCommand::Done(args)) => status::handle_done(&args, force, &resolved),
        Some(TaskCommand::Reject(args)) => status::handle_reject(&args, force, &resolved),
        Some(TaskCommand::Field(args)) => edit::handle_field(&args, force, &resolved),
        Some(TaskCommand::Edit(args)) => edit::handle_edit(&args, force, &resolved),
        Some(TaskCommand::Update(args)) => edit::handle_update(&args, &resolved, force),
        Some(TaskCommand::Build(args)) => build::handle(&args, force, &resolved),
        Some(TaskCommand::Template(template_args)) => template::handle(&resolved, &template_args),
        Some(TaskCommand::BuildRefactor(args)) | Some(TaskCommand::Refactor(args)) => {
            refactor::handle(&args, force, &resolved)
        }
        Some(TaskCommand::Show(args)) => show::handle(&args, &resolved),
        Some(TaskCommand::Clone(args)) => clone::handle(&args, force, &resolved),
        Some(TaskCommand::Batch(args)) => batch::handle(&args, force, &resolved),
        Some(TaskCommand::Schedule(args)) => schedule::handle(&args, force, &resolved),
        Some(TaskCommand::Relate(args)) => relations::handle_relate(&args, force, &resolved),
        Some(TaskCommand::Blocks(args)) => relations::handle_blocks(&args, force, &resolved),
        Some(TaskCommand::MarkDuplicate(args)) => {
            relations::handle_mark_duplicate(&args, force, &resolved)
        }
        Some(TaskCommand::Split(args)) => split::handle(&args, force, &resolved),
        None => {
            // Default command: build from request
            build::handle(&args.build, force, &resolved)
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;
    use crate::cli::queue::QueueShowFormat;
    use crate::cli::task::args::{TaskEditFieldArg, TaskStatusArg};

    #[test]
    fn task_update_help_mentions_rp_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let update = task
            .find_subcommand_mut("update")
            .expect("task update subcommand");
        let help = update.render_long_help().to_string();

        assert!(
            help.contains("ralph task update --repo-prompt plan RQ-0001"),
            "missing repo-prompt plan example: {help}"
        );
        assert!(
            help.contains("ralph task update --repo-prompt off --fields scope,evidence RQ-0001"),
            "missing repo-prompt off example: {help}"
        );
        assert!(
            help.contains("ralph task update --approval-mode auto-edits --runner claude RQ-0001"),
            "missing approval-mode example: {help}"
        );
    }

    #[test]
    fn task_show_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let show = task
            .find_subcommand_mut("show")
            .expect("task show subcommand");
        let help = show.render_long_help().to_string();

        assert!(
            help.contains("ralph task show RQ-0001"),
            "missing show example: {help}"
        );
        assert!(
            help.contains("--format compact"),
            "missing format example: {help}"
        );
    }

    #[test]
    fn task_details_alias_parses() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "details", "RQ-0001", "--format", "compact"])
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
    fn task_build_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
            "--repo-prompt",
            "plan",
            "-e",
            "high",
            "Add tests",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Build(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Plan));
                    assert_eq!(args.effort.as_deref(), Some("high"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "build",
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
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("yolo"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                }
                _ => panic!("expected task build command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_repo_prompt_and_effort_alias() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
            "--repo-prompt",
            "off",
            "-e",
            "low",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert_eq!(args.repo_prompt, Some(crate::agent::RepoPromptMode::Off));
                    assert_eq!(args.effort.as_deref(), Some("low"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_runner_cli_overrides() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "update",
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
                    assert_eq!(args.runner_cli.approval_mode.as_deref(), Some("auto-edits"));
                    assert_eq!(args.runner_cli.sandbox.as_deref(), Some("disabled"));
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_parses_dry_run_flag() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "edit",
            "--dry-run",
            "title",
            "New title",
            "RQ-0001",
        ])
        .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert!(args.dry_run);
                    assert_eq!(args.task_ids, vec!["RQ-0001"]);
                    assert_eq!(args.value, "New title");
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_without_dry_run_defaults_to_false() {
        let cli = Cli::try_parse_from(["ralph", "task", "edit", "title", "New title", "RQ-0001"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_parses_dry_run_flag() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "--dry-run", "RQ-0001"])
            .expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert!(args.dry_run);
                    assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_update_without_dry_run_defaults_to_false() {
        let cli = Cli::try_parse_from(["ralph", "task", "update", "RQ-0001"]).expect("parse");

        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Update(args)) => {
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task update command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_refactor_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "refactor"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Refactor(_)) => {}
                _ => panic!("expected task refactor command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_ref_alias_parses() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "ref", "--threshold", "800"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Refactor(args)) => {
                    assert_eq!(args.threshold, 800);
                }
                _ => panic!("expected task refactor command via alias"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_build_refactor_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "build-refactor", "--threshold", "700"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::BuildRefactor(args)) => {
                    assert_eq!(args.threshold, 700);
                }
                _ => panic!("expected task build-refactor command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "clone", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert!(!args.dry_run);
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_duplicate_alias_parses() {
        let cli = Cli::try_parse_from(["ralph", "task", "duplicate", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                }
                _ => panic!("expected task clone command via duplicate alias"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_status_flag() {
        let cli = Cli::try_parse_from(["ralph", "task", "clone", "--status", "todo", "RQ-0001"])
            .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.status, Some(TaskStatusArg::Todo));
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_title_prefix() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "clone",
            "--title-prefix",
            "[Follow-up] ",
            "RQ-0001",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert_eq!(args.title_prefix, Some("[Follow-up] ".to_string()));
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_parses_dry_run_flag() {
        let cli =
            Cli::try_parse_from(["ralph", "task", "clone", "--dry-run", "RQ-0001"]).expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Clone(args)) => {
                    assert_eq!(args.task_id, "RQ-0001");
                    assert!(args.dry_run);
                }
                _ => panic!("expected task clone command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_clone_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let clone = task
            .find_subcommand_mut("clone")
            .expect("task clone subcommand");
        let help = clone.render_long_help().to_string();

        assert!(
            help.contains("ralph task clone RQ-0001"),
            "missing clone example: {help}"
        );
        assert!(
            help.contains("--status"),
            "missing --status example: {help}"
        );
        assert!(
            help.contains("--title-prefix"),
            "missing --title-prefix example: {help}"
        );
        assert!(
            help.contains("ralph task duplicate"),
            "missing duplicate alias example: {help}"
        );
    }

    #[test]
    fn task_batch_status_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "batch", "status", "doing", "RQ-0001", "RQ-0002", "RQ-0003",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::args::BatchOperation::Status(status_args) => {
                        assert_eq!(status_args.status, TaskStatusArg::Doing);
                        assert_eq!(status_args.task_ids, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
                        assert!(!args.dry_run);
                        assert!(!args.continue_on_error);
                    }
                    _ => panic!("expected batch status operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_status_parses_tag_filter() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "batch",
            "status",
            "doing",
            "--tag-filter",
            "rust",
            "--tag-filter",
            "cli",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::args::BatchOperation::Status(status_args) => {
                        assert_eq!(status_args.status, TaskStatusArg::Doing);
                        assert!(status_args.task_ids.is_empty());
                        assert_eq!(status_args.tag_filter, vec!["rust", "cli"]);
                    }
                    _ => panic!("expected batch status operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_field_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "batch", "field", "severity", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Batch(args)) => match args.operation {
                    crate::cli::task::args::BatchOperation::Field(field_args) => {
                        assert_eq!(field_args.key, "severity");
                        assert_eq!(field_args.value, "high");
                        assert_eq!(field_args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                    }
                    _ => panic!("expected batch field operation"),
                },
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_edit_parses_dry_run() {
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
                    assert!(!args.continue_on_error);
                    match args.operation {
                        crate::cli::task::args::BatchOperation::Edit(edit_args) => {
                            assert_eq!(edit_args.field, TaskEditFieldArg::Priority);
                            assert_eq!(edit_args.value, "high");
                            assert_eq!(edit_args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                        }
                        _ => panic!("expected batch edit operation"),
                    }
                }
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_parses_continue_on_error() {
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
                    assert!(!args.dry_run);
                    assert!(args.continue_on_error);
                    match args.operation {
                        crate::cli::task::args::BatchOperation::Status(status_args) => {
                            assert_eq!(status_args.status, TaskStatusArg::Doing);
                        }
                        _ => panic!("expected batch status operation"),
                    }
                }
                _ => panic!("expected task batch command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_batch_help_mentions_examples() {
        let mut cmd = Cli::command();
        let task = cmd.find_subcommand_mut("task").expect("task subcommand");
        let batch = task
            .find_subcommand_mut("batch")
            .expect("task batch subcommand");
        let help = batch.render_long_help().to_string();

        assert!(
            help.contains("ralph task batch status doing"),
            "missing batch status example: {help}"
        );
        assert!(
            help.contains("--tag-filter"),
            "missing --tag-filter example: {help}"
        );
        assert!(
            help.contains("--dry-run"),
            "missing --dry-run example: {help}"
        );
        assert!(
            help.contains("--continue-on-error"),
            "missing --continue-on-error example: {help}"
        );
    }

    #[test]
    fn task_status_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "status", "doing", "RQ-0001", "RQ-0002", "RQ-0003",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Status(args)) => {
                    assert_eq!(args.status, TaskStatusArg::Doing);
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002", "RQ-0003"]);
                }
                _ => panic!("expected task status command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_status_parses_tag_filter() {
        let cli = Cli::try_parse_from([
            "ralph",
            "task",
            "status",
            "doing",
            "--tag-filter",
            "rust",
            "--tag-filter",
            "cli",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Status(args)) => {
                    assert_eq!(args.status, TaskStatusArg::Doing);
                    assert!(args.task_ids.is_empty());
                    assert_eq!(args.tag_filter, vec!["rust", "cli"]);
                }
                _ => panic!("expected task status command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_field_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "field", "severity", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Field(args)) => {
                    assert_eq!(args.key, "severity");
                    assert_eq!(args.value, "high");
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                }
                _ => panic!("expected task field command"),
            },
            _ => panic!("expected task command"),
        }
    }

    #[test]
    fn task_edit_parses_multiple_ids() {
        let cli = Cli::try_parse_from([
            "ralph", "task", "edit", "priority", "high", "RQ-0001", "RQ-0002",
        ])
        .expect("parse");
        match cli.command {
            crate::cli::Command::Task(args) => match args.command {
                Some(crate::cli::task::TaskCommand::Edit(args)) => {
                    assert_eq!(args.field, TaskEditFieldArg::Priority);
                    assert_eq!(args.value, "high");
                    assert_eq!(args.task_ids, vec!["RQ-0001", "RQ-0002"]);
                }
                _ => panic!("expected task edit command"),
            },
            _ => panic!("expected task command"),
        }
    }
}
