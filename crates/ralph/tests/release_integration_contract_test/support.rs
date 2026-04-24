//! Shared helpers for the release integration contract test suite.
//!
//! Purpose:
//! - Shared helpers for the release integration contract test suite.
//!
//! Responsibilities:
//! - Resolve the repo root from the test binary location.
//! - Copy tracked fixtures and run shell/git helpers used across contract tests.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) fn repo_root() -> PathBuf {
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

pub(crate) fn read_repo_file(relative_path: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative_path))
        .unwrap_or_else(|err| panic!("failed to read {relative_path}: {err}"))
}

pub(crate) fn public_readiness_scan_shell_helper_path() -> PathBuf {
    repo_root().join("scripts/lib/public_readiness_scan.sh")
}

pub(crate) fn public_readiness_scan_python_path() -> PathBuf {
    repo_root().join("scripts/lib/public_readiness_scan.py")
}

pub(crate) fn copy_repo_file(relative_path: &str, destination_root: &Path) {
    let source = repo_root().join(relative_path);
    let destination = destination_root.join(relative_path);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|err| panic!("create {}: {err}", parent.display()));
    }
    std::fs::copy(&source, &destination).unwrap_or_else(|err| {
        panic!(
            "copy {} -> {}: {err}",
            source.display(),
            destination.display()
        )
    });
}

pub(crate) fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|err| panic!("create {}: {err}", parent.display()));
    }
    std::fs::write(path, content).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

pub(crate) fn assert_output_redacts_secret(output: &Output, secret: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(secret),
        "scanner stdout must not echo matched secret material"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(secret),
        "scanner stderr must not echo matched secret material"
    );
}

pub(crate) fn init_git_repo(repo_root: &Path) {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(repo_root)
        .output()
        .expect("init git repo");
    Command::new("git")
        .args(["config", "user.name", "Pi Tests"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.name");
    Command::new("git")
        .args(["config", "user.email", "pi-tests@example.com"])
        .current_dir(repo_root)
        .output()
        .expect("configure git user.email");
}

pub(crate) fn copy_pre_public_check_fixture(repo_root: &Path) {
    for relative_path in [
        "scripts/pre-public-check.sh",
        "scripts/lib/ralph-shell.sh",
        "scripts/lib/release_policy.sh",
        "scripts/lib/public_readiness_scan.sh",
        "scripts/lib/public_readiness_scan.py",
        "README.md",
        "LICENSE",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        "SECURITY.md",
        "CODE_OF_CONDUCT.md",
        "docs/guides/public-readiness.md",
        "docs/guides/release-runbook.md",
        "docs/releasing.md",
        ".github/ISSUE_TEMPLATE/bug_report.md",
        ".github/ISSUE_TEMPLATE/feature_request.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
}

pub(crate) fn break_git_index(repo_root: &Path) {
    let index_path = repo_root.join(".git/index");
    if index_path.exists() {
        std::fs::remove_file(&index_path)
            .unwrap_or_else(|err| panic!("remove {}: {err}", index_path.display()));
    }
    std::fs::create_dir(&index_path)
        .unwrap_or_else(|err| panic!("create {}: {err}", index_path.display()));
}

pub(crate) fn swift_file_names(relative_dir: &str) -> Vec<String> {
    let mut names = std::fs::read_dir(repo_root().join(relative_dir))
        .unwrap_or_else(|err| panic!("read_dir {relative_dir}: {err}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("read_dir entry {relative_dir}: {err}"))
                .file_name()
                .into_string()
                .unwrap_or_else(|value| panic!("non-utf8 file name in {relative_dir}: {value:?}"))
        })
        .filter(|name| name.ends_with(".swift"))
        .collect::<Vec<_>>();
    names.sort();
    names
}
