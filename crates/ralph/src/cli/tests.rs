//! Parse-regression tests for the top-level CLI surface.
//!
//! Responsibilities:
//! - Verify key top-level command routes and rejected legacy flags.
//! - Keep root CLI parsing coverage out of the root facade file.
//! - Assert version/help behaviors exposed by Clap.
//!
//! Not handled here:
//! - Exhaustive per-subcommand argument validation owned by submodules.
//! - Runtime execution behavior after parsing succeeds.
//!
//! Invariants/assumptions:
//! - Tests exercise the public `Cli` parser exactly as end users invoke it.
//! - Removed flags/subcommands must remain rejected.

use super::{Cli, Command};
use crate::cli::{queue, run, task};
use clap::Parser;
use clap::error::ErrorKind;

#[test]
fn cli_parses_queue_list_smoke() {
    let cli = Cli::try_parse_from(["ralph", "queue", "list"]).expect("parse");
    match cli.command {
        Command::Queue(_) => {}
        other => panic!(
            "expected queue command, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn cli_parses_queue_archive_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "queue", "archive"]).expect("parse");
    match cli.command {
        Command::Queue(queue::QueueArgs { command }) => match command {
            queue::QueueCommand::Archive(_) => {}
            _ => panic!("expected queue archive command"),
        },
        _ => panic!("expected queue command"),
    }
}

#[test]
fn cli_rejects_invalid_prompt_phase() {
    let err = Cli::try_parse_from(["ralph", "prompt", "worker", "--phase", "4"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(msg.contains("invalid phase"), "unexpected error: {msg}");
}

#[test]
fn cli_parses_run_git_revert_mode() {
    let cli = Cli::try_parse_from(["ralph", "run", "one", "--git-revert-mode", "disabled"])
        .expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::One(args) => {
                assert_eq!(args.agent.git_revert_mode.as_deref(), Some("disabled"));
            }
            _ => panic!("expected run one command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_run_git_publish_mode() {
    let cli =
        Cli::try_parse_from(["ralph", "run", "one", "--git-publish-mode", "off"]).expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::One(args) => {
                assert_eq!(args.agent.git_publish_mode.as_deref(), Some("off"));
            }
            _ => panic!("expected run one command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_run_include_draft() {
    let cli = Cli::try_parse_from(["ralph", "run", "one", "--include-draft"]).expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::One(args) => {
                assert!(args.agent.include_draft);
            }
            _ => panic!("expected run one command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_run_one_debug() {
    let cli = Cli::try_parse_from(["ralph", "run", "one", "--debug"]).expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::One(args) => {
                assert!(args.debug);
            }
            _ => panic!("expected run one command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_run_loop_debug() {
    let cli = Cli::try_parse_from(["ralph", "run", "loop", "--debug"]).expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::Loop(args) => {
                assert!(args.debug);
            }
            _ => panic!("expected run loop command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_run_one_id() {
    let cli = Cli::try_parse_from(["ralph", "run", "one", "--id", "RQ-0001"]).expect("parse");
    match cli.command {
        Command::Run(args) => match args.command {
            run::RunCommand::One(args) => {
                assert_eq!(args.id.as_deref(), Some("RQ-0001"));
            }
            _ => panic!("expected run one command"),
        },
        _ => panic!("expected run command"),
    }
}

#[test]
fn cli_parses_task_update_without_id() {
    let cli = Cli::try_parse_from(["ralph", "task", "update"]).expect("parse");
    match cli.command {
        Command::Task(args) => match args.command {
            Some(task::TaskCommand::Update(args)) => {
                assert!(args.task_id.is_none());
            }
            _ => panic!("expected task update command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_update_with_id() {
    let cli = Cli::try_parse_from(["ralph", "task", "update", "RQ-0001"]).expect("parse");
    match cli.command {
        Command::Task(args) => match args.command {
            Some(task::TaskCommand::Update(args)) => {
                assert_eq!(args.task_id.as_deref(), Some("RQ-0001"));
            }
            _ => panic!("expected task update command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_rejects_removed_run_one_interactive_flag_short() {
    let err = Cli::try_parse_from(["ralph", "run", "one", "-i"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_rejects_removed_run_one_interactive_flag_long() {
    let err = Cli::try_parse_from(["ralph", "run", "one", "--interactive"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_parses_task_default_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "task", "Add", "tests"]).expect("parse");
    match cli.command {
        Command::Task(args) => {
            assert!(args.command.is_none(), "expected implicit build subcommand");
            assert_eq!(
                args.build.request,
                vec!["Add".to_string(), "tests".to_string()]
            );
        }
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_ready_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "task", "ready", "RQ-0005"]).expect("parse");
    match cli.command {
        Command::Task(args) => match args.command {
            Some(task::TaskCommand::Ready(args)) => {
                assert_eq!(args.task_id, "RQ-0005");
            }
            _ => panic!("expected task ready command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_done_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "task", "done", "RQ-0001"]).expect("parse");
    match cli.command {
        Command::Task(args) => match args.command {
            Some(task::TaskCommand::Done(args)) => {
                assert_eq!(args.task_id, "RQ-0001");
            }
            _ => panic!("expected task done command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_reject_subcommand() {
    let cli = Cli::try_parse_from(["ralph", "task", "reject", "RQ-0002"]).expect("parse");
    match cli.command {
        Command::Task(args) => match args.command {
            Some(task::TaskCommand::Reject(args)) => {
                assert_eq!(args.task_id, "RQ-0002");
            }
            _ => panic!("expected task reject command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_rejects_queue_set_status_subcommand() {
    let result = Cli::try_parse_from(["ralph", "queue", "set-status", "RQ-0001", "doing"]);
    assert!(result.is_err(), "expected queue set-status to be rejected");
    let msg = result
        .err()
        .expect("queue set-status error")
        .to_string()
        .to_lowercase();
    assert!(
        msg.contains("unrecognized") || msg.contains("unexpected") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_rejects_removed_run_loop_interactive_flag_short() {
    let err = Cli::try_parse_from(["ralph", "run", "loop", "-i"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_rejects_removed_run_loop_interactive_flag_long() {
    let err = Cli::try_parse_from(["ralph", "run", "loop", "--interactive"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_rejects_removed_tui_command() {
    let err = Cli::try_parse_from(["ralph", "tui"])
        .err()
        .expect("parse failure");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_rejects_run_loop_with_id_flag() {
    let err = Cli::try_parse_from(["ralph", "run", "loop", "--id", "RQ-0001"])
        .err()
        .expect("parse failure");
    let msg = err.to_string();
    assert!(
        msg.contains("unexpected") || msg.contains("unrecognized") || msg.contains("unknown"),
        "unexpected error: {msg}"
    );
}

#[test]
fn cli_supports_top_level_version_flag_long() {
    let err = Cli::try_parse_from(["ralph", "--version"])
        .err()
        .expect("expected clap to render version and exit");
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    let rendered = err.to_string();
    assert!(rendered.contains("ralph"));
    assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn cli_supports_top_level_version_flag_short() {
    let err = Cli::try_parse_from(["ralph", "-V"])
        .err()
        .expect("expected clap to render version and exit");
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    let rendered = err.to_string();
    assert!(rendered.contains("ralph"));
    assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
}
