//! `pre-public-check.sh` contract coverage (exact paths and local-only artifacts).
//!
//! Responsibilities:
//! - Virtualenv, target, pytest cache, `.DS_Store`, and related tracked-path rejections.

use std::process::Command;

use super::support::{copy_pre_public_check_fixture, copy_repo_file, init_git_repo, write_file};

#[test]
fn pre_public_check_rejects_tracked_exact_ralph_root_file() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(&repo_root.join(".ralph"), "tracked root ralph file\n");

    init_git_repo(repo_root);
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-secrets",
            "--skip-clean",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked exact .ralph root file");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked exact .ralph root files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked .ralph files outside the public allowlist detected")
            && combined.contains(".ralph"),
        "tracked exact .ralph root file rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_tracked_exact_ralph_root_symlink() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let outside_dir = tempfile::tempdir().expect("create outside dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    std::fs::write(outside_dir.path().join("outside.txt"), "outside\n")
        .expect("write outside file");
    symlink(
        outside_dir.path().join("outside.txt"),
        repo_root.join(".ralph"),
    )
    .expect("create tracked root .ralph symlink");

    init_git_repo(repo_root);
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-secrets",
            "--skip-clean",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked exact .ralph root symlink");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked exact .ralph root symlinks\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked .ralph files outside the public allowlist detected")
            && combined.contains(".ralph"),
        "tracked exact .ralph root symlink rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_exact_target_path() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(&repo_root.join("target"), "tracked exact target path\n");

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
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with exact tracked target path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject exact tracked target paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked runtime/build artifacts detected")
            && combined.contains("target"),
        "exact tracked target path rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_virtualenv_artifacts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(
        &repo_root.join(".venv/bin/python"),
        "#!/usr/bin/env python3\n",
    );

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
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked virtualenv artifact");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked virtualenv artifacts\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked runtime/build artifacts detected")
            && combined.contains(".venv/bin/python"),
        "tracked virtualenv artifact rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_exact_virtualenv_path() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(&repo_root.join(".venv"), "tracked exact .venv path\n");

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
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with exact tracked virtualenv path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject exact tracked virtualenv paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked runtime/build artifacts detected") && combined.contains(".venv"),
        "exact tracked virtualenv path rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_pytest_cache_artifacts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(&repo_root.join(".pytest_cache/v/cache/nodeids"), "[]\n");

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
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked pytest cache artifact");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked pytest cache artifacts\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked runtime/build artifacts detected")
            && combined.contains(".pytest_cache/v/cache/nodeids"),
        "tracked pytest cache artifact rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_exact_pytest_cache_path() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(
        &repo_root.join(".pytest_cache"),
        "tracked exact .pytest_cache path\n",
    );

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
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with exact tracked pytest cache path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject exact tracked pytest cache paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked runtime/build artifacts detected")
            && combined.contains(".pytest_cache"),
        "exact tracked pytest cache path rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_ds_store() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    write_file(&repo_root.join(".DS_Store"), "finder metadata\n");

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
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked ds_store");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked .DS_Store files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked local-only files detected") && combined.contains(".DS_Store"),
        "tracked .DS_Store rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_tracked_broken_ds_store_symlink() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

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
    symlink("DOES_NOT_EXIST", repo_root.join(".DS_Store")).expect("create broken ds_store symlink");

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
    Command::new("git")
        .args(["config", "core.excludesFile", "/dev/null"])
        .current_dir(repo_root)
        .output()
        .expect("disable global excludes for fixture repo");
    Command::new("git")
        .args(["add", "-f", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-clean"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with broken tracked ds_store symlink");

    assert!(
        !output.status.success(),
        "pre-public-check should reject broken tracked .DS_Store symlinks\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked local-only files detected") && combined.contains(".DS_Store"),
        "broken tracked .DS_Store symlink rejection should explain the offending path\noutput:\n{}",
        combined
    );
}
