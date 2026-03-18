//! Queue issue CLI tests grouped by behavior.
//!
//! Responsibilities:
//! - Act as the thin suite hub for queue issue publish regressions.
//! - Re-export shared issue-test helpers for focused companion modules.
//! - Keep only the lightweight CLI help smoke tests inline.
//!
//! Not handled here:
//! - Fake `gh` executable generation.
//! - Large create/update/failure scenario bodies.
//! - Shared queue fixture construction details.
//!
//! Invariants/assumptions:
//! - Companion modules preserve existing test names and assertions.
//! - Parent queue-test helpers are imported from `super::*`.
//! - Unix-only helpers remain cfg-gated so non-Unix builds still compile.

use super::*;
use crate::cli::Cli;
use clap::CommandFactory;

mod create_update_tests;
mod dry_run_tests;
mod failure_tests;
mod fake_gh;
mod publish_many_tests;
mod support;

#[cfg(unix)]
use fake_gh::{create_fake_gh_for_issue_publish, create_fake_gh_for_issue_publish_multi};
use support::{
    base_issue_publish_args, issue_task, run_issue_publish, run_issue_publish_many,
    write_issue_queue_tasks,
};

#[test]
fn queue_issue_publish_help_examples_expanded() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
    let help = issue_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue issue publish"),
        "missing issue publish example: {help}"
    );
    assert!(help.contains("ralph queue issue publish-many"));
}

#[test]
fn queue_issue_publish_help_contains_publish_subcommand() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let _issue_cmd = queue
        .find_subcommand_mut("issue")
        .expect("queue issue subcommand");
}
