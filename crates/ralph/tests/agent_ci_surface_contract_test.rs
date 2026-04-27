//! Agent CI surface classifier contract tests.
//!
//! Purpose:
//! - Agent CI surface classifier contract tests.
//!
//! Responsibilities:
//! - Verify the classifier reasons about the current uncommitted working tree.
//! - Guard representative path routing for `noop`, `ci-docs`, `ci-fast`, `ci`, and `macos-ci`.
//!
//! Not handled here:
//! - Executing the Makefile targets selected by the classifier.
//! - Exhaustive path-matrix testing for every policy branch.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The classifier script and shared shell libs live at stable repo-relative paths.
//! - Git is available locally for temporary repository setup.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

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

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, content).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

fn copy_script(repo_root: &Path, temp_repo: &Path, relative_path: &str) {
    let src = repo_root.join(relative_path);
    let dest = temp_repo.join(relative_path);
    let content =
        std::fs::read_to_string(&src).unwrap_or_else(|err| panic!("read {}: {err}", src.display()));
    write_file(&dest, &content);
}

fn git(temp_repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(temp_repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_classifier(temp_repo: &Path, mode: &str) -> String {
    let output = Command::new("bash")
        .arg(temp_repo.join("scripts/agent-ci-surface.sh"))
        .arg(mode)
        .current_dir(temp_repo)
        .output()
        .expect("run agent-ci classifier");
    assert!(
        output.status.success(),
        "classifier {mode} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_temp_repo() -> TempDir {
    let repo_root = repo_root();
    let temp_repo = tempfile::tempdir().expect("create temp repo");
    let repo_path = temp_repo.path();

    copy_script(&repo_root, repo_path, "scripts/agent-ci-surface.sh");
    copy_script(&repo_root, repo_path, "scripts/lib/ralph-shell.sh");
    copy_script(&repo_root, repo_path, "scripts/lib/release_policy.sh");

    write_file(&repo_path.join("README.md"), "# Temp repo\n");
    write_file(&repo_path.join("docs/guide.md"), "# Docs\n");
    write_file(&repo_path.join("Makefile"), "help:\n\t@echo ok\n");
    write_file(
        &repo_path.join("crates/ralph/src/lib.rs"),
        "// stub crate\n",
    );

    git(repo_path, &["init", "-b", "main"]);
    git(repo_path, &["config", "user.name", "Codex"]);
    git(repo_path, &["config", "user.email", "codex@example.com"]);
    git(repo_path, &["add", "."]);
    git(repo_path, &["commit", "-m", "initial"]);

    temp_repo
}

#[test]
fn classifier_routes_docs_only_working_tree_to_ci_docs() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(&repo_path.join("docs/guide.md"), "# Docs\n\nupdated\n");

    assert_eq!(run_classifier(repo_path, "--target"), "ci-docs");
}

#[test]
fn classifier_routes_non_app_working_tree_to_ci_fast() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(&repo_path.join(".gitignore"), "target/\n");

    assert_eq!(run_classifier(repo_path, "--target"), "ci-fast");
    assert!(
        run_classifier(repo_path, "--reason").contains("Rust/CLI verification"),
        "expected Rust/CLI routing explanation"
    );
}

#[test]
fn classifier_routes_makefile_ci_router_edit_to_ci_fast() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("Makefile"),
        "help:\n\t@echo ok\n\nagent-ci:\n\t@echo changed\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci-fast");
    assert!(
        run_classifier(repo_path, "--reason").contains("Makefile CI/router change"),
        "expected Makefile CI/router routing explanation"
    );
}

#[test]
fn classifier_routes_clean_main_without_local_changes_to_noop() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    assert_eq!(run_classifier(repo_path, "--target"), "noop");
    assert_eq!(
        run_classifier(repo_path, "--reason"),
        "no local changes; nothing to validate"
    );
}

#[test]
fn classifier_routes_crates_working_tree_to_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("crates/ralph/src/lib.rs"),
        "// stub crate\n\npub fn touched() {}\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci");
    assert!(
        run_classifier(repo_path, "--reason").contains("Rust crate"),
        "expected Rust release gate routing explanation"
    );
}

#[test]
fn classifier_routes_schemas_working_tree_to_macos_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(&repo_path.join("schemas/config.schema.json"), "{}\n");

    assert_eq!(run_classifier(repo_path, "--target"), "macos-ci");
}

#[test]
fn classifier_routes_apps_ralphmac_working_tree_to_macos_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("apps/RalphMac/Stub.swift"),
        "// placeholder\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "macos-ci");
}

#[test]
fn classifier_routes_makefile_macos_build_edit_to_macos_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("Makefile"),
        "help:\n\t@echo ok\n\nmacos-build:\n\t@echo changed\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "macos-ci");
    assert!(
        run_classifier(repo_path, "--reason").contains("Makefile app/macOS build change"),
        "expected Makefile macOS ship routing explanation"
    );
}

#[test]
fn classifier_routes_ci_router_script_to_ci_fast() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("scripts/pre-public-check.sh"),
        "#!/usr/bin/env bash\n# touched\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci-fast");
    assert!(
        run_classifier(repo_path, "--reason").contains("CI/router/tooling script"),
        "expected CI/router script routing explanation"
    );
}

#[test]
fn classifier_routes_cli_bundle_script_to_macos_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("scripts/ralph-cli-bundle.sh"),
        "#!/usr/bin/env bash\n# bundle touched\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "macos-ci");
}

#[test]
fn classifier_routes_rust_toolchain_check_script_to_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("scripts/check-rust-toolchain.sh"),
        "#!/usr/bin/env bash\n# toolchain check touched\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci");
    assert!(
        run_classifier(repo_path, "--reason").contains("release/build script"),
        "expected release/build script routing explanation"
    );
}

#[test]
fn classifier_routes_release_verify_pipeline_script_to_ci() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_file(
        &repo_path.join("scripts/lib/release_verify_pipeline.sh"),
        "#!/usr/bin/env bash\n# release verify touched\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci");
}

#[test]
fn classifier_ignores_previous_branch_commits_when_local_diff_is_docs_only() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    git(repo_path, &["checkout", "-b", "feature/app-then-docs"]);
    write_file(
        &repo_path.join("apps/RalphMac/Stub.swift"),
        "// prior app change\n",
    );
    git(repo_path, &["add", "apps/RalphMac/Stub.swift"]);
    git(repo_path, &["commit", "-m", "touch app surface"]);

    write_file(
        &repo_path.join("docs/guide.md"),
        "# Docs\n\nupdated later\n",
    );

    assert_eq!(run_classifier(repo_path, "--target"), "ci-docs");
    assert_eq!(
        run_classifier(repo_path, "--reason"),
        "docs/community metadata only"
    );
}

#[test]
fn classifier_emit_eval_exports_assignments() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    let output = Command::new("bash")
        .arg(repo_path.join("scripts/agent-ci-surface.sh"))
        .arg("--emit-eval")
        .current_dir(repo_path)
        .output()
        .expect("run emit-eval");

    assert!(
        output.status.success(),
        "emit-eval failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("RALPH_AGENT_CI_TARGET=") && stdout.contains("noop"),
        "emit-eval should assign RALPH_AGENT_CI_TARGET=noop on clean tree:\n{stdout}"
    );
    assert!(
        stdout.contains("RALPH_AGENT_CI_REASON="),
        "emit-eval should assign RALPH_AGENT_CI_REASON:\n{stdout}"
    );
}
