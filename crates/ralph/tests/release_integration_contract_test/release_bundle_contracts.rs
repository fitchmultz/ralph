//! Xcode project, Makefile, and release pipeline bundling contracts.

use super::support::{read_repo_file, swift_file_names};

#[test]
fn xcode_project_references_all_committed_swift_sources() {
    let project = read_repo_file("apps/RalphMac/RalphMac.xcodeproj/project.pbxproj");

    for relative_dir in [
        "apps/RalphMac/RalphCore",
        "apps/RalphMac/RalphCoreTests",
        "apps/RalphMac/RalphMac",
        "apps/RalphMac/RalphMacUITests",
    ] {
        for file_name in swift_file_names(relative_dir) {
            let file_ref_marker = format!("/* {file_name} */");
            let build_marker = format!("/* {file_name} in Sources */");
            assert!(
                project.contains(&file_ref_marker),
                "Xcode project is missing file reference for {relative_dir}/{file_name}"
            );
            assert!(
                project.contains(&build_marker),
                "Xcode project is missing Sources membership for {relative_dir}/{file_name}"
            );
        }
    }
}

#[test]
fn xcode_build_phase_uses_shared_cli_bundle_entrypoint() {
    let project = read_repo_file("apps/RalphMac/RalphMac.xcodeproj/project.pbxproj");
    assert!(
        project.contains("scripts/ralph-cli-bundle.sh"),
        "Xcode project should call the shared CLI bundling script"
    );
    assert!(
        !project.contains("cargo ${BUILD_ARGS}") && !project.contains("target/debug/ralph"),
        "Xcode project should not embed its own Cargo invocation policy or debug hardcoded CLI paths"
    );
    assert!(
        project.contains("target/release/ralph") && project.contains("ralph-cli-bundle.sh"),
        "Release should prefer copying an existing target/release/ralph when present, with ralph-cli-bundle.sh as fallback"
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
