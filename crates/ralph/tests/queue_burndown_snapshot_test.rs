//! Snapshot tests for `ralph queue burndown` text output.
//!
//! Responsibilities:
//! - Lock in the human-readable burndown text format.
//! - Avoid snapshot churn from unstable date strings (filters replace dates).
//!
//! Not handled here:
//! - JSON output schema assertions (covered by existing tests).
//!
//! Invariants/assumptions:
//! - Output contains date keys (`YYYY-MM-DD`) which must be filtered for stable snapshots.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

#[test]
fn burndown_days_2_one_open_task_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut t1 = test_support::make_test_task("RQ-0001", "Open task", TaskStatus::Todo);
    t1.created_at = Some("2026-01-20T00:00:00Z".to_string());
    t1.updated_at = Some("2026-01-20T00:00:00Z".to_string());

    test_support::write_queue(dir.path(), &[t1])?;
    test_support::write_done(dir.path(), &[])?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "burndown", "--days", "2"]);
    anyhow::ensure!(status.success(), "burndown failed\nstderr:\n{stderr}");

    test_support::with_insta_settings(|| {
        insta::assert_snapshot!(
            "queue_burndown_days2_one_open_task",
            test_support::normalize_for_snapshot(&stdout)
        );
    });

    Ok(())
}

#[test]
fn burndown_with_done_tasks_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut t1 = test_support::make_test_task("RQ-0001", "Todo task", TaskStatus::Todo);
    t1.created_at = Some("2026-01-15T00:00:00Z".to_string());

    let mut t2 = test_support::make_test_task("RQ-0002", "Done task", TaskStatus::Done);
    t2.created_at = Some("2026-01-10T00:00:00Z".to_string());
    t2.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let mut t3 = test_support::make_test_task("RQ-0003", "Doing task", TaskStatus::Doing);
    t3.created_at = Some("2026-01-12T00:00:00Z".to_string());

    test_support::write_queue(dir.path(), &[t1, t3])?;
    test_support::write_done(dir.path(), &[t2])?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "burndown", "--days", "14"]);
    anyhow::ensure!(status.success(), "burndown failed\nstderr:\n{stderr}");

    test_support::with_insta_settings(|| {
        insta::assert_snapshot!(
            "queue_burndown_with_done_tasks",
            test_support::normalize_for_snapshot(&stdout)
        );
    });

    Ok(())
}
