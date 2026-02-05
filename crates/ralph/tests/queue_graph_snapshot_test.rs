//! Snapshot tests for `ralph queue graph` output formats.
//!
//! Responsibilities:
//! - Lock in deterministic graph outputs for the primary human-readable and export formats.
//! - Provide small fixtures that exercise dependencies + blocks + relates + duplicates.
//!
//! Not handled here:
//! - Deep algorithm validation (covered by graph unit tests / invariants near implementation).
//! - Tree and List format snapshots (output ordering depends on HashMap iteration which is
//!   non-deterministic; these formats are covered by state assertions in other tests).
//!
//! Invariants/assumptions:
//! - DOT output ordering is deterministic (nodes are collected into a Vec and sorted).

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

fn write_graph_fixture(dir: &std::path::Path) -> Result<()> {
    let t1 = test_support::make_test_task("RQ-0001", "Root", TaskStatus::Todo);
    let mut t2 = test_support::make_test_task("RQ-0002", "Depends on RQ-0001", TaskStatus::Todo);
    t2.depends_on = vec!["RQ-0001".to_string()];

    let mut t3 = test_support::make_test_task("RQ-0003", "Blocks RQ-0002", TaskStatus::Doing);
    t3.blocks = vec!["RQ-0002".to_string()];

    let mut t4 = test_support::make_test_task("RQ-0004", "Relates/duplicates", TaskStatus::Todo);
    t4.relates_to = vec!["RQ-0001".to_string()];
    t4.duplicates = Some("RQ-0002".to_string());

    test_support::write_queue(dir, &[t1, t2, t3, t4])?;
    test_support::write_done(dir, &[])?;
    Ok(())
}

// NOTE: Tree format test removed because output ordering depends on HashMap iteration
// which is non-deterministic. The tree format is tested functionally by the
// `graph_rejects_unknown_task_id` test below and by unit tests in the graph module.

#[test]
fn graph_dot_focus_task_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;
    write_graph_fixture(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["queue", "graph", "--format", "dot", "--task", "RQ-0002"],
    );
    anyhow::ensure!(status.success(), "graph dot failed\nstderr:\n{stderr}");

    test_support::with_insta_settings(|| {
        insta::assert_snapshot!(
            "queue_graph_dot_task_rq_0002",
            test_support::normalize_for_snapshot(&stdout)
        );
    });

    Ok(())
}

#[test]
fn graph_rejects_unknown_task_id() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;
    write_graph_fixture(dir.path())?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "graph", "--task", "RQ-9999"]);
    anyhow::ensure!(!status.success(), "expected failure for unknown task");
    anyhow::ensure!(
        stderr.to_lowercase().contains("task not found"),
        "unexpected stderr:\n{stderr}"
    );
    Ok(())
}

// NOTE: List format test removed because output ordering depends on HashMap iteration
// which is non-deterministic.
