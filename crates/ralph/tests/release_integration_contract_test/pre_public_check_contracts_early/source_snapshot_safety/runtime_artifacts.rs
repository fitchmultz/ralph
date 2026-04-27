//! Source-snapshot runtime-artifact contracts for `pre-public-check.sh`.
//!
//! Purpose:
//! - Keep `--allow-no-git` success and runtime-artifact rejection coverage together.
//!
//! Responsibilities:
//! - Verify source-snapshot fallback success plus runtime, env, virtualenv, and metadata artifact rejection.
//!
//! Scope:
//! - Limited to source-snapshot runtime-artifact coverage.
//!
//! Usage:
//! - Loaded by `source_snapshot_safety.rs`.
//!
//! Invariants/Assumptions:
//! - These tests must preserve the existing release-contract assertions verbatim.

use std::process::Command;

use super::super::super::support::{copy_pre_public_check_fixture, copy_repo_files, write_file};

#[test]
fn pre_public_check_allow_no_git_supports_source_snapshot_safety_mode() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode");

    assert!(
        output.status.success(),
        "source-snapshot safety mode should succeed outside git\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("source-snapshot safety mode")
            || stdout.contains("Git worktree unavailable"),
        "source-snapshot safety mode should explain the skipped git-backed checks\nstdout:\n{}",
        stdout
    );
}

#[test]
fn agent_ci_succeeds_outside_git_via_source_snapshot_safety_mode() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_repo_files(
        repo_root,
        &[
            "Makefile",
            "mk/rust.mk",
            "mk/repo-safety.mk",
            "mk/ci.mk",
            "mk/macos.mk",
            "mk/coverage.mk",
            "Cargo.toml",
            "Cargo.lock",
            "VERSION",
            "rust-toolchain.toml",
            "scripts/ralph-cli-bundle.sh",
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
        ],
    );

    let wrapper_makefile = repo_root.join("OracleAgentCI.mk");
    write_file(
        &wrapper_makefile,
        "include Makefile\n\n# Test-only stubs so the contract test exercises routing instead of full toolchains.\ntarget/tmp/stamps/ralph-release-build.stamp:\n\t@mkdir -p target/tmp/stamps\n\t@touch $@\n\t@echo stub-release-stamp\n\ncheck-file-size-limits deps format-check type-check lint test build generate install-verify install macos-preflight macos-build macos-test macos-test-contracts:\n\t@echo stub-$@\n",
    );

    let fake_bin_dir = repo_root.join("test-bin");
    write_file(
        &fake_bin_dir.join("uname"),
        "#!/usr/bin/env bash\necho Linux\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let uname_path = fake_bin_dir.join("uname");
        let mut permissions = std::fs::metadata(&uname_path)
            .unwrap_or_else(|err| panic!("stat {}: {err}", uname_path.display()))
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&uname_path, permissions)
            .unwrap_or_else(|err| panic!("chmod {}: {err}", uname_path.display()));
    }

    let make_program = if Command::new("gmake").arg("--version").output().is_ok() {
        "gmake"
    } else {
        "make"
    };
    let original_path = std::env::var("PATH").unwrap_or_default();
    let combined_path = if original_path.is_empty() {
        fake_bin_dir.display().to_string()
    } else {
        format!("{}:{original_path}", fake_bin_dir.display())
    };

    let wrapper_path = wrapper_makefile.to_str().expect("wrapper makefile utf-8");
    let recursive_make = format!("{make_program} -f {wrapper_path}");
    let output = Command::new(make_program)
        .args([
            "-f",
            wrapper_path,
            &format!("MAKE={recursive_make}"),
            "agent-ci",
        ])
        .env("PATH", combined_path)
        .current_dir(repo_root)
        .output()
        .expect("run make agent-ci outside git worktree");

    assert!(
        output.status.success(),
        "agent-ci should succeed outside git\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("using platform-aware release gate fallback"),
        "agent-ci should report the no-git fallback path\nstdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("stub-build") || stdout.contains("stub-install"),
        "agent-ci should reach the release-gate fallback instead of failing in check-env-safety\nstdout:\n{}",
        stdout
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_runtime_artifacts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".ralph/cache/session.jsonc"),
        "{\"session\":\"test\"}\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with runtime artifacts");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject runtime artifacts\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".ralph/cache"),
        "runtime artifact rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_local_env_files() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(&repo_root.join(".env.local"), "SECRET_TOKEN=test\n");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with env file");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject local env files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".env.local"),
        "env-file rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_local_only_directory_contents() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".env.local/secret.txt"),
        "local-only directory payload\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with local-only directory contents");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject local-only directory contents\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".env.local/secret.txt"),
        "local-only directory rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_virtualenv_directory() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".venv/bin/python"),
        "#!/usr/bin/env python3\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with virtualenv directory");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject virtualenv directories\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".venv"),
        "virtualenv rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_allow_no_git_rejects_broken_virtualenv_symlink() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    symlink("DOES_NOT_EXIST", repo_root.join(".venv")).expect("create broken virtualenv symlink");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with broken virtualenv symlink");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject broken virtualenv symlinks\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".venv"),
        "broken virtualenv symlink rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_ds_store() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(&repo_root.join(".DS_Store"), "finder metadata\n");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with ds_store");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject .DS_Store\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".DS_Store"),
        ".DS_Store rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_allow_no_git_rejects_ds_store_symlink() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    symlink("README.md", repo_root.join(".DS_Store")).expect("create ds_store symlink");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-clean",
            "--allow-no-git",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run source-snapshot safety mode with ds_store symlink");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject .DS_Store symlinks\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".DS_Store"),
        ".DS_Store symlink rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}
