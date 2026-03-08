//! Release script contract tests.
//!
//! Responsibilities:
//! - Guard release-script transaction invariants that should not regress silently.
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
fn release_script_derives_repo_url_from_origin_remote() {
    let script = read_repo_file("scripts/lib/release_pipeline.sh");
    assert!(
        script.contains("ralph_get_repo_http_url"),
        "release pipeline should derive the repo URL from git remote origin"
    );
    assert!(
        !script.contains("https://github.com/fitchmultz/ralph/compare"),
        "release pipeline should not hardcode compare links to a specific owner"
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
fn release_policy_treats_cargo_lock_as_release_metadata() {
    let script = read_repo_file("scripts/lib/release_policy.sh");
    assert!(
        script.contains("\"Cargo.lock\""),
        "release policy should treat Cargo.lock as release metadata"
    );
}

#[test]
fn release_artifact_builder_cleans_release_artifacts_directory() {
    let script = read_repo_file("scripts/build-release-artifacts.sh");
    let cleanup_count = script.matches("rm -rf \"$RELEASE_ARTIFACTS_DIR\"").count();
    assert!(
        cleanup_count >= 1,
        "artifact builder should clean target/release-artifacts before packaging"
    );
}

#[test]
fn release_policy_uses_target_transaction_state() {
    let script = read_repo_file("scripts/lib/release_state.sh");
    assert!(
        script.contains("target/release-transactions"),
        "release state should keep release transaction state under target/release-transactions"
    );
}

#[test]
fn release_script_publishes_only_after_local_release_is_prepared() {
    let script = read_repo_file("scripts/lib/release_pipeline.sh");
    assert!(
        script.contains("release_prepare_verified_snapshot")
            && script.contains("release_create_commit_and_tag")
            && script.contains("release_publish_crate"),
        "release pipeline should verify locally before publishing externally"
    );
    assert!(
        script.find("release_prepare_verified_snapshot") < script.find("release_publish_crate"),
        "release pipeline should prepare the verified snapshot before publish"
    );
}

#[test]
fn release_script_reconciles_without_legacy_skip_flags() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("scripts/release.sh reconcile")
            || script.contains("reconcile 0.2.0")
            || script.contains("reconcile <version>"),
        "release.sh should document durable transaction-state reconciliation"
    );
    assert!(
        !script.contains("RALPH_RELEASE_SKIP_PUBLISH"),
        "release.sh should not rely on the old manual skip-publish recovery flag"
    );
    assert!(
        !script.contains("RALPH_RELEASE_ALLOW_EXISTING_TAG"),
        "release.sh should not retain the old existing-tag override flag"
    );
}

#[test]
fn release_verify_allows_release_metadata_drift_after_version_sync() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("release_validate_repo_state 0 1"),
        "execute mode should allow release-only metadata drift after verify prepares the publish snapshot"
    );
}

#[test]
fn release_execute_initializes_transaction_after_validation() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.find("release_validate_repo_state 0")
            < script.find("release_state_init \"execute\""),
        "execute mode should not create transaction state before repo-state validation succeeds"
    );
}

#[test]
fn release_verify_records_a_reusable_snapshot() {
    let script = read_repo_file("scripts/release.sh");
    assert!(
        script.contains("release_verify_state_init")
            && script.contains("release_prepare_verified_snapshot")
            && script.contains("release_verify_assert_ready_for_execute"),
        "release.sh should prepare a durable verify snapshot and require it before execute"
    );
}

#[test]
fn release_verify_state_uses_target_release_verifications_directory() {
    let script = read_repo_file("scripts/lib/release_verify_state.sh");
    assert!(
        script.contains("target/release-verifications"),
        "verified release state should live under target/release-verifications"
    );
}
