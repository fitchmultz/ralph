//! Release/public-readiness/build integration contract tests.
//!
//! Responsibilities:
//! - Guard shared contracts between Xcode, shell scripts, and the Makefile.
//! - Ensure public-readiness and bundling logic stays centralized.
//!
//! Not handled here:
//! - End-to-end release execution.
//! - Credentialed crates.io or GitHub interactions.
//!
//! Invariants/assumptions:
//! - Contract files live at stable repo-relative paths.

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
fn pre_public_check_uses_repo_wide_markdown_discovery() {
    let script = read_repo_file("scripts/pre-public-check.sh");
    assert!(
        script.contains("ls-files") && script.contains("\"*.md\""),
        "pre-public check should discover markdown files repo-wide instead of hardcoding a short subset"
    );
    assert!(
        script.contains("--release-context"),
        "pre-public check should expose an explicit release-context mode"
    );
}

#[test]
fn xcode_build_phase_uses_shared_cli_bundle_entrypoint() {
    let project = read_repo_file("apps/RalphMac/RalphMac.xcodeproj/project.pbxproj");
    assert!(
        project.contains("scripts/ralph-cli-bundle.sh"),
        "Xcode project should call the shared CLI bundling script"
    );
    assert!(
        !project.contains("cargo ${BUILD_ARGS}"),
        "Xcode project should not embed its own Cargo fallback build policy"
    );
}

#[test]
fn shared_cli_bundle_script_supports_configuration_and_bundle_dir() {
    let script = read_repo_file("scripts/ralph-cli-bundle.sh");
    assert!(
        script.contains("--configuration") && script.contains("--bundle-dir"),
        "shared CLI bundle script should accept configuration and bundle destination inputs"
    );
    assert!(
        script.contains("ralph_activate_pinned_rust_toolchain"),
        "shared CLI bundle script should honor the pinned rustup toolchain"
    );
}
