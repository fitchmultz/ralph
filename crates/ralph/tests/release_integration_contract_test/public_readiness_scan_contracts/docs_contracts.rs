//! Public-readiness scan stale-doc contract coverage.
//!
//! Purpose:
//! - Keep targeted stale documentation snippet coverage grouped with the public-readiness scan seam.
//!
//! Responsibilities:
//! - Verify docs mode rejects stale RalphMac decomposition guidance and stale session-management contract examples.
//!
//! Scope:
//! - Limited to path-specific stale-doc snippets that should never return once corrected.
//!
//! Usage:
//! - Loaded by `public_readiness_scan_contracts.rs`.
//!
//! Invariants/Assumptions:
//! - The scan must stay narrow and path-specific rather than banning version numbers repo-wide.

use std::process::Command;

use super::super::support::{copy_public_readiness_scan_fixture, write_file};

#[test]
fn public_readiness_scan_docs_mode_rejects_stale_app_decompose_command() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_public_readiness_scan_fixture(repo_root);
    write_file(
        &repo_root.join("docs/features/app.md"),
        "The app calls `ralph task decompose --format json`.\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("docs")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness docs scan");

    assert_eq!(
        output.status.code(),
        Some(1),
        "docs scan should reject the stale app decomposition command snippet"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "docs/features/app.md: use `ralph machine task decompose` for RalphMac decomposition docs"
        ),
        "docs scan should explain the stale app command failure\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_docs_mode_rejects_stale_session_resume_event_version() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_public_readiness_scan_fixture(repo_root);
    write_file(
        &repo_root.join("docs/features/session-management.md"),
        r#"```json
{
  "version": 2,
  "kind": "resume_decision",
  "task_id": "RQ-0001"
}
```
"#,
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("docs")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness docs scan");

    assert_eq!(
        output.status.code(),
        Some(1),
        "docs scan should reject stale machine run resume event versions"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "docs/features/session-management.md: machine run resume_decision examples must use version 3"
        ),
        "docs scan should explain the stale resume event version failure\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_docs_mode_rejects_stale_session_config_version() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_public_readiness_scan_fixture(repo_root);
    write_file(
        &repo_root.join("docs/features/session-management.md"),
        r#"```json
{
  "version": 3,
  "resume_preview": {
    "status": "refusing_to_resume"
  }
}
```
"#,
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("docs")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness docs scan");

    assert_eq!(
        output.status.code(),
        Some(1),
        "docs scan should reject stale machine config resolve versions"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(
            "docs/features/session-management.md: machine config resolve examples must use version 4"
        ),
        "docs scan should explain the stale config resolve version failure\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_docs_mode_allows_unrelated_version_three_json_blocks() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_public_readiness_scan_fixture(repo_root);
    write_file(
        &repo_root.join("docs/features/session-management.md"),
        r#"
Unrelated machine event example:

```json
{
  "version": 3,
  "kind": "resume_decision",
  "timestamp": "2026-04-26T06:00:00Z"
}
```

Valid config preview example:

```json
{
  "version": 4,
  "resume_preview": {
    "status": "refusing_to_resume"
  }
}
```
"#,
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("docs")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness docs scan");

    assert_eq!(
        output.status.code(),
        Some(0),
        "docs scan should ignore unrelated version 3 JSON blocks outside the config preview example"
    );
}
