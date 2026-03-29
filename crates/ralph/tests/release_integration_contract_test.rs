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
    let focused_scan_helper = read_repo_file("scripts/lib/public_readiness_scan.sh");
    assert!(
        script.contains("bash \"$SCRIPT_DIR/lib/public_readiness_scan.sh\" links \"$REPO_ROOT\""),
        "pre-public check should delegate markdown discovery to the focused repo-wide scan helper"
    );
    assert!(
        focused_scan_helper.contains("public_readiness_scan.py")
            && focused_scan_helper.contains("python3 \"$scan_py_path\" links \"$repo_root\""),
        "focused public-readiness scan helper should drive repo-wide markdown discovery from the working tree"
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
        !project.contains("cargo ${BUILD_ARGS}")
            && !project.contains("target/debug/ralph")
            && !project.contains("target/release/ralph"),
        "Xcode project should not embed its own Cargo fallback policy or stale hardcoded CLI output paths"
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
    assert!(
        script.contains("--target") && script.contains("--jobs"),
        "shared CLI bundle script should act as the canonical build entrypoint for both native and cross-target builds"
    );
    assert!(
        !script.contains("RALPH_BIN_PATH"),
        "shared CLI bundle script should not allow callers to bypass the canonical build contract with an arbitrary binary override"
    );
}

#[test]
fn release_pipeline_uses_github_draft_then_publish_flow() {
    let script = read_repo_file("scripts/lib/release_publish_pipeline.sh");
    assert!(
        script.contains("gh release create \"v$VERSION\"")
            && script.contains("--draft")
            && script.contains("gh release edit \"v$VERSION\" --draft=false"),
        "release publish pipeline should prepare a draft release before final publication"
    );
    assert!(
        script.find("gh release create \"v$VERSION\"")
            < script.find("cargo publish -p \"$CRATE_PACKAGE_NAME\" --locked"),
        "GitHub draft preparation should happen before crates.io publish"
    );
    assert!(
        script.find("cargo publish -p \"$CRATE_PACKAGE_NAME\" --locked")
            < script.find("gh release edit \"v$VERSION\" --draft=false"),
        "GitHub release publication should happen only after crates.io publish"
    );
}

#[test]
fn makefile_release_build_uses_shared_bundle_entrypoint() {
    let makefile = read_repo_file("Makefile");
    assert!(
        makefile.contains("scripts/ralph-cli-bundle.sh --configuration Release"),
        "Makefile release builds should route through the shared CLI bundling entrypoint"
    );
    assert!(
        !makefile.contains("cargo build --workspace --release --locked"),
        "Makefile should not keep a separate direct cargo release-build path"
    );
    assert!(
        !makefile.contains("publish-crate:"),
        "Makefile should not expose a direct crates.io publish bypass outside the release transaction"
    );
}
