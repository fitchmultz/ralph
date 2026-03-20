//! Purpose: temp-root and stale-cleanup integration coverage for `ralph::fsutil`.
//!
//! Responsibilities:
//! - Verify Ralph temp roots are used for Ralph-scoped temp directories.
//! - Verify stale cleanup removes only matching prefixed entries.
//! - Verify prefix-list cleanup handles legacy and Ralph prefixes together.
//!
//! Scope:
//! - Temp cleanup and temp-dir creation only; atomic writes and tilde expansion live elsewhere.
//!
//! Usage:
//! - Compiled through the `fsutil_test` hub and relies on its shared imports.
//!
//! Invariants/Assumptions:
//! - Tests operate on per-test temp directories.
//! - Assertions remain identical to the pre-split suite.

use super::*;

#[test]
fn test_cleanup_stale_temp_dirs_removes_prefixed_entries_only() {
    let base = TempDir::new().expect("create temp dir");
    let stale_dir = base
        .path()
        .join(format!("{}old-dir", fsutil::RALPH_TEMP_PREFIX));
    let stale_file = base
        .path()
        .join(format!("{}old-file.txt", fsutil::RALPH_TEMP_PREFIX));
    let keep_dir = base.path().join("keep-dir");

    fs::create_dir_all(&stale_dir).expect("create stale dir");
    fs::write(&stale_file, "stale").expect("create stale file");
    fs::create_dir_all(&keep_dir).expect("create keep dir");

    let removed = fsutil::cleanup_stale_temp_dirs(base.path(), Duration::from_secs(0))
        .expect("cleanup temp dirs");
    assert_eq!(removed, 2);
    assert!(!stale_dir.exists());
    assert!(!stale_file.exists());
    assert!(keep_dir.exists());
}

#[test]
fn test_create_ralph_temp_dir_uses_temp_root() {
    let temp_dir = fsutil::create_ralph_temp_dir("unit").expect("create ralph temp dir");
    let path = temp_dir.path().to_path_buf();
    let root = fsutil::ralph_temp_root();
    assert!(path.starts_with(&root));
    let name = path.file_name().expect("temp dir name").to_string_lossy();
    assert!(name.starts_with(fsutil::RALPH_TEMP_PREFIX));
}

#[test]
fn test_cleanup_stale_temp_entries_honors_prefix_list() {
    let base = TempDir::new().expect("create temp dir");
    let legacy_dir = base.path().join("legacy-temp");
    let ralph_dir = base
        .path()
        .join(format!("{}new-temp", fsutil::RALPH_TEMP_PREFIX));
    fs::create_dir_all(&legacy_dir).expect("create legacy dir");
    fs::create_dir_all(&ralph_dir).expect("create ralph dir");

    let removed = fsutil::cleanup_stale_temp_entries(
        base.path(),
        &["legacy", fsutil::RALPH_TEMP_PREFIX],
        Duration::from_secs(0),
    )
    .expect("cleanup temp entries");

    assert_eq!(removed, 2);
    assert!(!legacy_dir.exists());
    assert!(!ralph_dir.exists());
}
