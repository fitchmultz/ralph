//! Release script contract tests.
//!
//! Responsibilities:
//! - Guard release-script invariants that should not regress silently.
//! - Verify release automation derives GitHub metadata from the current repo.
//!
//! Not handled here:
//! - End-to-end release execution.
//! - Credentialed GitHub or crates.io interactions.
//!
//! Invariants/assumptions:
//! - The release script lives at `scripts/release.sh` relative to repo root.
//! - The release-notes template lives at `.github/release-notes-template.md`.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    profile_dir
        .parent()
        .expect("profile directory should have a parent (target)")
        .parent()
        .expect("target directory should have a parent (repo root)")
        .to_path_buf()
}

fn read_repo_file(relative_path: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative_path))
        .unwrap_or_else(|err| panic!("failed to read {relative_path}: {err}"))
}

#[test]
fn release_script_sets_explicit_github_release_title() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("--title \"v$VERSION\""),
        "release.sh should set an explicit GitHub release title"
    );
}

#[test]
fn release_script_derives_repo_url_from_origin_remote() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("get_repo_http_url()"),
        "release.sh should derive the repo URL from git remote origin"
    );
    assert!(
        !script.contains("https://github.com/fitchmultz/ralph/compare"),
        "release.sh should not hardcode compare links to a specific owner"
    );
}

#[test]
fn release_notes_template_uses_repo_placeholders() {
    let template = read_repo_file(".github/release-notes-template.md");
    assert!(
        template.contains("{{REPO_URL}}"),
        "release-notes template should use repo URL placeholders"
    );
    assert!(
        template.contains("{{REPO_CLONE_URL}}"),
        "release-notes template should use clone URL placeholders"
    );
}

#[test]
fn release_script_treats_cargo_lock_as_release_metadata() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("\"Cargo.lock\""),
        "release.sh should treat Cargo.lock as release metadata"
    );
    assert!(
        script.contains("git add VERSION Cargo.lock"),
        "release.sh should stage Cargo.lock in the release commit"
    );
    assert!(
        script.contains("git checkout -- VERSION Cargo.lock"),
        "release.sh should roll back Cargo.lock on failed release cleanup"
    );
}

#[test]
fn release_script_cleans_release_artifacts_directory() {
    let script = read_repo_file("scripts/release.sh");
    let cleanup_count = script.matches("rm -rf \"$RELEASE_ARTIFACTS_DIR\"").count();
    assert!(
        cleanup_count >= 2,
        "release.sh should clean target/release-artifacts before packaging and during cleanup"
    );
}

#[test]
fn release_script_allows_dirty_packaging_review_before_release_commit() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("cargo package --list -p \"$CRATE_PACKAGE_NAME\" --allow-dirty"),
        "release.sh should review packaged files with --allow-dirty before the release commit exists"
    );
    assert!(
        script
            .contains("cargo publish --dry-run -p \"$CRATE_PACKAGE_NAME\" --locked --allow-dirty"),
        "release.sh should dry-run crates.io publish with --allow-dirty before the release commit exists"
    );
}
