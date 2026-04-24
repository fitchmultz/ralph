//! Tests for export command.
//!
//! Purpose:
//! - Tests for export command.
//!
//! Responsibilities:
//! - Test export command handlers and validation.
//! - Test export format and help text.
//!
//! Not handled here:
//! - Import operations (see import.rs).
//! - List/search operations (see list_search.rs).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::{base_export_args, resolved_for_dir};
use crate::cli::Cli;
use crate::cli::queue::export;
use clap::CommandFactory;
use tempfile::TempDir;

#[test]
fn queue_export_rejects_conflicting_archive_flags() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);

    let mut args = base_export_args();
    args.include_archive = true;
    args.only_archive = true;

    let err = export::handle(&resolved, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("Conflicting flags")
            && msg.contains("--include-archive")
            && msg.contains("--only-archive"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_export_help_examples_expanded() {
    let mut cmd = Cli::command();
    let queue = cmd.find_subcommand_mut("queue").expect("queue subcommand");
    let export_cmd = queue
        .find_subcommand_mut("export")
        .expect("queue export subcommand");
    let help = export_cmd.render_long_help().to_string();

    assert!(
        help.contains("ralph queue export"),
        "missing export example: {help}"
    );
    assert!(
        help.contains("--format csv"),
        "missing format example: {help}"
    );
    assert!(
        help.contains("--output tasks.csv"),
        "missing output example: {help}"
    );
}
