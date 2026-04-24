//! Integration tests for `ralph queue import` undo behavior.
//!
//! Purpose:
//! - Integration tests for `ralph queue import` undo behavior.
//!
//! Responsibilities:
//! - Verify no snapshot is created when import validation fails.
//! - Verify snapshot IS created when import succeeds.
//!
//! Not handled here:
//! - Unit tests for import parsing (see crates/ralph/src/cli/queue/import.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp directories outside the repo.
//! - Each test creates its own git repo and ralph project via test_support helpers.

use anyhow::Result;
use ralph::contracts::TaskStatus;

mod test_support;

/// Test that queue import with malformed JSON does NOT create an undo snapshot.
#[test]
fn queue_import_malformed_json_no_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Write malformed JSON to temp file
    let bad_json_path = dir.path().join("bad_import.json");
    std::fs::write(&bad_json_path, b"{ this is not valid json }")?;

    // Attempt import - should fail
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "json",
            "--input",
            bad_json_path.to_str().unwrap(),
        ],
    );
    anyhow::ensure!(!status.success(), "import should fail with malformed JSON");

    // Verify NO snapshot was created
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("No continuation checkpoints are available"),
        "expected no snapshots after failed import, got:\n{stdout}"
    );

    Ok(())
}

/// Test that queue import with valid JSON DOES create an undo snapshot.
#[test]
fn queue_import_valid_json_creates_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Write valid JSON to temp file
    let good_json_path = dir.path().join("good_import.json");
    let import_data = r#"[{"id": "RQ-0002", "title": "Imported task", "status": "todo"}]"#;
    std::fs::write(&good_json_path, import_data)?;

    // Import should succeed
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "json",
            "--input",
            good_json_path.to_str().unwrap(),
        ],
    );
    anyhow::ensure!(
        status.success(),
        "import should succeed\nstderr:\n{_stderr}"
    );

    // Verify snapshot WAS created
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("Available continuation checkpoints"),
        "expected snapshot after successful import, got:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("queue import"),
        "expected 'queue import' operation in snapshot list, got:\n{stdout}"
    );

    Ok(())
}

/// Test that queue import with malformed CSV does NOT create an undo snapshot.
#[test]
fn queue_import_malformed_csv_no_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Write CSV without required 'title' column
    let bad_csv_path = dir.path().join("bad_import.csv");
    std::fs::write(&bad_csv_path, b"id,status\nRQ-0002,todo")?;

    // Attempt import - should fail (missing title column)
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "csv",
            "--input",
            bad_csv_path.to_str().unwrap(),
        ],
    );
    anyhow::ensure!(
        !status.success(),
        "import should fail with missing title column"
    );

    // Verify NO snapshot was created
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("No continuation checkpoints are available"),
        "expected no snapshots after failed import, got:\n{stdout}"
    );

    Ok(())
}

/// Test that import --dry-run does NOT create a snapshot.
#[test]
fn queue_import_dry_run_no_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Write valid JSON to temp file
    let json_path = dir.path().join("import.json");
    let import_data = r#"[{"id": "RQ-0002", "title": "Imported task", "status": "todo"}]"#;
    std::fs::write(&json_path, import_data)?;

    // Import with --dry-run
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "json",
            "--input",
            json_path.to_str().unwrap(),
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "import --dry-run should succeed\nstderr:\n{_stderr}"
    );

    // Verify NO snapshot was created (dry-run should never create snapshots)
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("No continuation checkpoints are available"),
        "expected no snapshots after dry-run import, got:\n{stdout}"
    );

    Ok(())
}

/// Test that import with duplicate ID and --on-duplicate fail does NOT create a snapshot.
#[test]
fn queue_import_duplicate_fail_no_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Write JSON with duplicate ID
    let dup_json_path = dir.path().join("dup_import.json");
    let import_data = r#"[{"id": "RQ-0001", "title": "Duplicate task", "status": "todo"}]"#;
    std::fs::write(&dup_json_path, import_data)?;

    // Attempt import with --on-duplicate fail (default) - should fail
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "json",
            "--input",
            dup_json_path.to_str().unwrap(),
        ],
    );
    anyhow::ensure!(!status.success(), "import should fail with duplicate ID");

    // Verify NO snapshot was created
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("No continuation checkpoints are available"),
        "expected no snapshots after failed duplicate import, got:\n{stdout}"
    );

    Ok(())
}

/// Test that import with non-existent file does NOT create a snapshot.
#[test]
fn queue_import_missing_file_no_snapshot() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create initial task
    let task = test_support::make_test_task("RQ-0001", "Test task", TaskStatus::Todo);
    test_support::write_queue(dir.path(), &[task])?;
    test_support::write_done(dir.path(), &[])?;

    // Attempt import with non-existent file - should fail
    let missing_path = dir.path().join("nonexistent.json");
    let (status, _stdout, _stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "queue",
            "import",
            "--format",
            "json",
            "--input",
            missing_path.to_str().unwrap(),
        ],
    );
    anyhow::ensure!(!status.success(), "import should fail with missing file");

    // Verify NO snapshot was created
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["undo", "--list"]);
    anyhow::ensure!(status.success(), "undo --list failed\nstderr:\n{stderr}");

    anyhow::ensure!(
        stdout.contains("No continuation checkpoints are available"),
        "expected no snapshots after failed import with missing file, got:\n{stdout}"
    );

    Ok(())
}
