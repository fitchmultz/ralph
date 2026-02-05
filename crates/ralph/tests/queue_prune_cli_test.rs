//! Integration tests for `ralph queue prune`.
//!
//! Responsibilities:
//! - Verify age/status filters remove the intended done tasks.
//! - Verify `--keep-last` protects most recent completions.
//! - Verify `--dry-run` does not mutate disk state.
//! - Verify safety rule: missing/invalid `completed_at` does not match age filter.
//!
//! Not handled here:
//! - Internal ordering/algorithm unit tests (covered near implementation).
//! - Snapshot testing of logs (prefer state assertions).
//!
//! Invariants/assumptions:
//! - Prune operates only on `.ralph/done.json`.
//! - `--keep-last` uses `completed_at` ordering (missing/invalid treated oldest).

use anyhow::Result;
use chrono::{Duration, SecondsFormat, Utc};
use ralph::contracts::TaskStatus;

mod test_support;

fn rfc3339_days_ago(days: i64) -> String {
    (Utc::now() - Duration::days(days)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[test]
fn queue_prune_dry_run_does_not_modify_done_file() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut old = test_support::make_test_task("RQ-0100", "Old done", TaskStatus::Done);
    old.completed_at = Some(rfc3339_days_ago(45));
    let mut recent = test_support::make_test_task("RQ-0101", "Recent done", TaskStatus::Done);
    recent.completed_at = Some(rfc3339_days_ago(5));

    test_support::write_done(dir.path(), &[old, recent])?;

    let before = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "prune", "--dry-run", "--age", "30"]);
    anyhow::ensure!(status.success(), "prune dry-run failed\nstderr:\n{stderr}");
    let after = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;

    anyhow::ensure!(before == after, "done.json mutated during --dry-run");
    Ok(())
}

#[test]
fn queue_prune_age_filter_respects_keep_last() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut a = test_support::make_test_task("RQ-0200", "A", TaskStatus::Done);
    a.completed_at = Some(rfc3339_days_ago(40));
    let mut b = test_support::make_test_task("RQ-0201", "B", TaskStatus::Done);
    b.completed_at = Some(rfc3339_days_ago(35));
    let mut c = test_support::make_test_task("RQ-0202", "C", TaskStatus::Done);
    c.completed_at = Some(rfc3339_days_ago(31));
    let mut d = test_support::make_test_task("RQ-0203", "D", TaskStatus::Done);
    d.completed_at = Some(rfc3339_days_ago(5));

    test_support::write_done(dir.path(), &[a, b, c, d])?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["queue", "prune", "--age", "30", "--keep-last", "2"],
    );
    anyhow::ensure!(status.success(), "prune failed\nstderr:\n{stderr}");

    let done = test_support::read_done(dir.path())?;
    let ids: Vec<_> = done.tasks.iter().map(|t| t.id.as_str()).collect();

    anyhow::ensure!(
        ids.contains(&"RQ-0202") && ids.contains(&"RQ-0203"),
        "expected keep-last protection; got {ids:?}"
    );
    anyhow::ensure!(
        !ids.contains(&"RQ-0200") && !ids.contains(&"RQ-0201"),
        "expected old tasks pruned; got {ids:?}"
    );

    Ok(())
}

#[test]
fn queue_prune_missing_completed_at_does_not_match_age_filter() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut old = test_support::make_test_task("RQ-0300", "Old", TaskStatus::Done);
    old.completed_at = Some(rfc3339_days_ago(60));

    let mut missing =
        test_support::make_test_task("RQ-0301", "Missing completed_at", TaskStatus::Done);
    missing.completed_at = None;

    let mut invalid =
        test_support::make_test_task("RQ-0302", "Invalid completed_at", TaskStatus::Done);
    invalid.completed_at = Some("not-a-timestamp".to_string());

    test_support::write_done(dir.path(), &[old, missing, invalid])?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["queue", "prune", "--age", "30"]);
    anyhow::ensure!(status.success(), "prune failed\nstderr:\n{stderr}");

    let done = test_support::read_done(dir.path())?;
    let ids: Vec<_> = done.tasks.iter().map(|t| t.id.as_str()).collect();

    anyhow::ensure!(
        !ids.contains(&"RQ-0300"),
        "expected old task pruned; got {ids:?}"
    );
    anyhow::ensure!(
        ids.contains(&"RQ-0301"),
        "missing completed_at should be kept for age filter; got {ids:?}"
    );
    anyhow::ensure!(
        ids.contains(&"RQ-0302"),
        "invalid completed_at should be kept for age filter; got {ids:?}"
    );

    Ok(())
}

#[test]
fn queue_prune_status_filter_works() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let mut done_old = test_support::make_test_task("RQ-0400", "Old done", TaskStatus::Done);
    done_old.completed_at = Some(rfc3339_days_ago(45));

    let mut rejected_old =
        test_support::make_test_task("RQ-0401", "Old rejected", TaskStatus::Rejected);
    rejected_old.completed_at = Some(rfc3339_days_ago(45));

    test_support::write_done(dir.path(), &[done_old, rejected_old])?;

    // Only prune rejected tasks older than 30 days
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["queue", "prune", "--age", "30", "--status", "rejected"],
    );
    anyhow::ensure!(status.success(), "prune failed\nstderr:\n{stderr}");

    let done = test_support::read_done(dir.path())?;
    let ids: Vec<_> = done.tasks.iter().map(|t| t.id.as_str()).collect();

    anyhow::ensure!(
        ids.contains(&"RQ-0400"),
        "done task should be kept when filtering by rejected; got {ids:?}"
    );
    anyhow::ensure!(
        !ids.contains(&"RQ-0401"),
        "rejected task should be pruned; got {ids:?}"
    );

    Ok(())
}
