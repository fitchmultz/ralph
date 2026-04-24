//! `pre-public-check.sh` contract coverage (`.ralph` cleanliness and symlink cases).
//!
//! Purpose:
//! - `pre-public-check.sh` contract coverage (`.ralph` cleanliness and symlink cases).
//!
//! Responsibilities:
//! - Allowlisted `.ralph` path checks, trust-file siblings, and symlinked allowlisted files.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::process::Command;

use super::support::{copy_repo_file, write_file};

#[test]
fn pre_public_check_rejects_dirty_allowlisted_ralph_readme() {
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
    write_file(&repo_root.join(".ralph/README.md"), "baseline\n");

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

    write_file(&repo_root.join(".ralph/README.md"), "dirty\n");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-secrets"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with dirty allowlisted .ralph readme");

    assert!(
        !output.status.success(),
        "pre-public-check should reject dirty allowlisted .ralph files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Working tree is not clean") && combined.contains(".ralph/README.md"),
        "dirty allowlisted .ralph file should be surfaced by cleanliness checks\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_trust_file_siblings_in_cleanliness_checks() {
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
    write_file(&repo_root.join(".ralph/README.md"), "baseline\n");

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

    write_file(&repo_root.join(".ralph/trust.json.backup"), "{}\n");
    write_file(&repo_root.join(".ralph/trust.jsonc.backup"), "{}\n");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-links", "--skip-secrets"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with trust file siblings");

    assert!(
        !output.status.success(),
        "pre-public-check should reject .ralph trust-file siblings\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Working tree is not clean")
            && combined.contains(".ralph/trust.json.backup")
            && combined.contains(".ralph/trust.jsonc.backup"),
        "trust-file siblings should not be hidden by ignored dirty-path filtering\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_symlinked_allowlisted_ralph_files() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let outside_dir = tempfile::tempdir().expect("create outside dir");
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
    std::fs::create_dir_all(repo_root.join(".ralph")).expect("create .ralph dir");
    std::fs::write(outside_dir.path().join("outside.md"), "outside\n")
        .expect("write outside markdown");
    symlink(
        outside_dir.path().join("outside.md"),
        repo_root.join(".ralph/README.md"),
    )
    .expect("create symlinked allowlisted .ralph readme");

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
        .args(["--skip-ci", "--skip-links", "--skip-secrets"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with symlinked allowlisted .ralph file");

    assert!(
        !output.status.success(),
        "pre-public-check should reject symlinked allowlisted .ralph files\nstdout:\n{}\nstderr:\n{}",
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
            && combined.contains(".ralph/README.md"),
        "symlinked allowlisted .ralph file rejection should explain the offending path\noutput:\n{}",
        combined
    );
}
