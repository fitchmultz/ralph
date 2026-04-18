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

use std::path::{Path, PathBuf};
use std::process::Command;

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

fn public_readiness_scan_shell_helper_path() -> PathBuf {
    repo_root().join("scripts/lib/public_readiness_scan.sh")
}

fn public_readiness_scan_python_path() -> PathBuf {
    repo_root().join("scripts/lib/public_readiness_scan.py")
}

fn copy_repo_file(relative_path: &str, destination_root: &Path) {
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

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|err| panic!("create {}: {err}", parent.display()));
    }
    std::fs::write(path, content).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

fn init_git_repo(repo_root: &Path) {
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

fn copy_pre_public_check_fixture(repo_root: &Path) {
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

fn break_git_index(repo_root: &Path) {
    let index_path = repo_root.join(".git/index");
    if index_path.exists() {
        std::fs::remove_file(&index_path)
            .unwrap_or_else(|err| panic!("remove {}: {err}", index_path.display()));
    }
    std::fs::create_dir(&index_path)
        .unwrap_or_else(|err| panic!("create {}: {err}", index_path.display()));
}

fn swift_file_names(relative_dir: &str) -> Vec<String> {
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

#[test]
fn pre_public_check_uses_repo_wide_markdown_discovery() {
    let script = read_repo_file("scripts/pre-public-check.sh");
    let focused_scan_helper = read_repo_file("scripts/lib/public_readiness_scan.sh");
    assert!(
        script.contains("bash \"$SCRIPT_DIR/lib/public_readiness_scan.sh\" links"),
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
    assert!(
        script.contains("--allow-no-git"),
        "pre-public check should expose a source-snapshot safety mode for check-env-safety"
    );
}

#[test]
fn pre_public_check_requires_git_worktree() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_repo_file("scripts/pre-public-check.sh", repo_root);
    copy_repo_file("scripts/lib/ralph-shell.sh", repo_root);
    copy_repo_file("scripts/lib/release_policy.sh", repo_root);

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .arg("--skip-ci")
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check outside git worktree");

    assert!(
        !output.status.success(),
        "pre-public-check should fail when git metadata is unavailable"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("require a git worktree"),
        "pre-public-check should explain that git-backed readiness checks are unavailable\nstderr:\n{}",
        stderr
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_symlinked_required_files_in_source_snapshots() {
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
    std::fs::remove_file(repo_root.join("LICENSE")).expect("remove copied license");
    std::fs::write(outside_dir.path().join("LICENSE.txt"), "external license\n")
        .expect("write external license");
    symlink(
        outside_dir.path().join("LICENSE.txt"),
        repo_root.join("LICENSE"),
    )
    .expect("create symlinked license");

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args(["--skip-ci", "--skip-clean", "--allow-no-git"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with symlinked required file in source snapshot");

    assert!(
        !output.status.success(),
        "pre-public-check should reject symlinked required files in source snapshots\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Required file must be a regular repo file, not a symlink: LICENSE"),
        "symlinked required file rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_symlinked_required_files_in_git_snapshots() {
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
    std::fs::create_dir_all(repo_root.join(".ralph")).expect("create .ralph dir");
    std::fs::write(repo_root.join(".ralph/trust.json"), "{}\n").expect("write trust file");
    std::fs::remove_file(repo_root.join("LICENSE")).expect("remove copied license");
    symlink(
        repo_root.join(".ralph/trust.json"),
        repo_root.join("LICENSE"),
    )
    .expect("create symlinked license");

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
        .args(["--skip-ci"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with symlinked required file in git snapshot");

    assert!(
        !output.status.success(),
        "pre-public-check should reject symlinked required files in git snapshots\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Required file must be a regular repo file, not a symlink: LICENSE"),
        "tracked symlinked required file rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_allow_no_git_supports_source_snapshot_safety_mode() {
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

    for relative_path in [
        "Makefile",
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
    ] {
        copy_repo_file(relative_path, repo_root);
    }

    let wrapper_makefile = repo_root.join("OracleAgentCI.mk");
    write_file(
        &wrapper_makefile,
        "include Makefile\n\n# Test-only stubs so the contract test exercises routing instead of full toolchains.\ntarget/tmp/stamps/ralph-release-build.stamp:\n\t@mkdir -p target/tmp/stamps\n\t@touch $@\n\t@echo stub-release-stamp\n\ndeps format-check type-check lint test build generate install-verify install macos-preflight macos-build macos-test macos-test-contracts:\n\t@echo stub-$@\n",
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

#[cfg(unix)]
#[test]
fn pre_public_check_allow_no_git_rejects_non_directory_ralph_roots() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let outside_dir = tempfile::tempdir().expect("create outside dir");

    let cases = [
        "broken-symlink",
        "internal-symlink",
        "external-symlink",
        "regular-file",
    ];
    for case_name in cases {
        let repo_root = temp_dir.path().join(case_name);
        std::fs::create_dir_all(&repo_root).expect("create case repo root");

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
            copy_repo_file(relative_path, &repo_root);
        }

        match case_name {
            "broken-symlink" => symlink("DOES_NOT_EXIST", repo_root.join(".ralph"))
                .expect("create broken .ralph symlink"),
            "internal-symlink" => {
                std::fs::create_dir_all(repo_root.join("internal-ralph"))
                    .expect("create internal .ralph target");
                symlink("internal-ralph", repo_root.join(".ralph"))
                    .expect("create internal .ralph symlink");
            }
            "external-symlink" => symlink(outside_dir.path(), repo_root.join(".ralph"))
                .expect("create external .ralph symlink"),
            "regular-file" => {
                write_file(&repo_root.join(".ralph"), "not a directory\n");
            }
            _ => unreachable!("unexpected case"),
        }

        let output = Command::new("bash")
            .arg(repo_root.join("scripts/pre-public-check.sh"))
            .args([
                "--skip-ci",
                "--skip-links",
                "--skip-clean",
                "--allow-no-git",
            ])
            .current_dir(&repo_root)
            .output()
            .unwrap_or_else(|err| panic!("run source-snapshot safety mode for {case_name}: {err}"));

        assert!(
            !output.status.success(),
            "source-snapshot safety mode should reject {case_name} .ralph roots\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("local/runtime artifacts") && stderr.contains(".ralph"),
            "{case_name} .ralph root rejection should explain the offending path\nstderr:\n{}",
            stderr
        );
    }
}

#[test]
fn pre_public_check_allow_no_git_rejects_virtualenv_directory() {
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

#[cfg(unix)]
#[test]
fn pre_public_check_allow_no_git_rejects_symlinked_allowlisted_ralph_files() {
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
        .expect("run source-snapshot safety mode with symlinked allowlisted .ralph file");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject symlinked allowlisted .ralph files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains(".ralph/README.md"),
        "symlinked allowlisted .ralph file rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_unallowlisted_ralph_paths() {
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
        &repo_root.join(".ralph/plugins/test.plugin/plugin.json"),
        "{\"name\":\"test.plugin\"}\n",
    );
    write_file(
        &repo_root.join(".ralph/trust.json"),
        "{\"allow_project_commands\":true}\n",
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
        .expect("run source-snapshot safety mode with unallowlisted .ralph paths");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject unallowlisted .ralph paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(".ralph/plugins/test.plugin/plugin.json")
            && stderr.contains(".ralph/trust.json"),
        "unallowlisted .ralph rejection should enumerate the offending paths\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_rejects_tracked_local_only_files() {
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
    write_file(&repo_root.join(".scratchpad.md"), "local operator notes\n");

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
        .args(["add", "-A"])
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
        .args(["--skip-ci", "--skip-links"])
        .current_dir(repo_root)
        .output()
        .expect("run pre-public-check with tracked local-only file");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked local-only files\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Tracked local-only files detected") && stderr.contains(".scratchpad.md"),
        "tracked local-only file rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_release_context_rejects_dirty_paths_with_control_characters() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    init_git_repo(repo_root);
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    write_file(
        &repo_root.join("CHANGELOG.md\nREADME.md"),
        "dirty filename payload\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/pre-public-check.sh"))
        .args([
            "--skip-ci",
            "--skip-links",
            "--skip-secrets",
            "--release-context",
        ])
        .current_dir(repo_root)
        .output()
        .expect("run release-context pre-public-check with dirty control-character path");

    assert!(
        !output.status.success(),
        "release-context pre-public-check should reject dirty paths with control characters\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unsupported control characters")
            && combined.contains("CHANGELOG.md")
            && combined.contains("README.md"),
        "dirty control-character path rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_rejects_git_status_collection_failures() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    init_git_repo(repo_root);
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");
    break_git_index(repo_root);

    let shell = format!(
        "SCRIPT_DIR={script_dir:?}\nREPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\nrelease_collect_dirty_lines \"$REPO_ROOT\"\n",
        script_dir = repo_root.join("scripts"),
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release dirty collection with broken git status");

    assert!(
        !output.status.success(),
        "release dirty collection should fail closed when git status fails\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("git status --porcelain=v1 -z failed"),
        "git status collection failure should be reported\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_rejects_path_validator_failures() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);

    let shell = format!(
        "SCRIPT_DIR={script_dir:?}\nREPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\nrelease_path_has_control_characters() {{ return 7; }}\nrelease_require_safe_publication_path 'Fixture' 'safe-path.txt'\n",
        script_dir = repo_root.join("scripts"),
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release path validation with injected validator failure");

    assert!(
        !output.status.success(),
        "release path validation should fail closed when the validator helper errors\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("path validation failed") && combined.contains("safe-path.txt"),
        "validator failure rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_git_ls_files_failures() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".scratchpad.md"),
        "tracked local-only payload\n",
    );

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
    break_git_index(repo_root);

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
        .expect("run pre-public-check with broken git ls-files");

    assert!(
        !output.status.success(),
        "pre-public-check should fail closed when git ls-files fails\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("git ls-files -z failed"),
        "git ls-files failure should be reported\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_tracked_local_only_control_character_paths() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".env\nREADME.md"),
        "tracked local-only newline path\n",
    );

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
        .expect("run pre-public-check with tracked local-only control-character path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked local-only control-character paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unsupported control characters")
            && combined.contains(".env")
            && combined.contains("README.md"),
        "tracked local-only control-character rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_allow_no_git_rejects_target_directory() {
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
        &repo_root.join("target/leakdir/out.txt"),
        "local build output\n",
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
        .expect("run source-snapshot safety mode with target directory");

    assert!(
        !output.status.success(),
        "source-snapshot safety mode should reject target directory contents\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local/runtime artifacts") && stderr.contains("target"),
        "target rejection should explain the offending path\nstderr:\n{}",
        stderr
    );
}

#[test]
fn pre_public_check_rejects_tracked_local_only_directory_contents() {
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
        &repo_root.join(".env.local/secret.txt"),
        "tracked local-only directory payload\n",
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
        .args(["add", "-A"])
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
        .expect("run pre-public-check with tracked local-only directory contents");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked local-only directory contents\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Tracked local-only files detected")
            && combined.contains(".env.local/secret.txt"),
        "tracked local-only directory rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[test]
fn pre_public_check_rejects_tracked_target_artifacts() {
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
        &repo_root.join("target/debug/ralph"),
        "built binary placeholder\n",
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
        .expect("run pre-public-check with tracked target artifact");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked target artifacts\nstdout:\n{}\nstderr:\n{}",
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
            && combined.contains("target/debug/ralph"),
        "tracked target artifact rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_tracked_runtime_artifact_control_character_paths() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join("target/evil\nREADME.md"),
        "tracked runtime artifact newline path\n",
    );

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
        .expect("run pre-public-check with tracked runtime control-character path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked runtime control-character paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unsupported control characters")
            && combined.contains("target/evil")
            && combined.contains("README.md"),
        "tracked runtime control-character rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

#[cfg(unix)]
#[test]
fn pre_public_check_rejects_tracked_ralph_control_character_paths() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    copy_pre_public_check_fixture(repo_root);
    write_file(
        &repo_root.join(".ralph/bad\nqueue.jsonc"),
        "tracked .ralph newline path\n",
    );

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
        .expect("run pre-public-check with tracked .ralph control-character path");

    assert!(
        !output.status.success(),
        "pre-public-check should reject tracked .ralph control-character paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("unsupported control characters")
            && combined.contains(".ralph/bad")
            && combined.contains("queue.jsonc"),
        "tracked .ralph control-character rejection should explain the offending path\noutput:\n{}",
        combined
    );
}

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

#[test]
fn public_readiness_scan_rejects_missing_repo_root() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let missing_repo_root = temp_dir.path().join("missing-repo-root");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&missing_repo_root)
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan scanner should reject a missing repo root"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("repository root does not exist or is not a directory"),
        "scanner should explain why the provided repo root was rejected"
    );
}

#[test]
fn public_readiness_scan_rejects_markdown_targets_outside_repo() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    std::fs::write(repo_root.join("README.md"), "[outside](../outside.md)\n")
        .expect("write markdown fixture");
    std::fs::write(temp_dir.path().join("outside.md"), "outside\n")
        .expect("write escaped target fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should reject markdown targets that escape the repo root"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("target escapes repo root"),
        "scanner should explain why escaped markdown targets are invalid"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_ignores_symlinked_repo_files_that_escape_repo() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let outside_markdown = temp_dir.path().join("outside.md");
    std::fs::write(&outside_markdown, "[outside](../outside.md)\n")
        .expect("write symlink target fixture");
    symlink(&outside_markdown, repo_root.join("README.md")).expect("create markdown symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(0),
        "public-readiness scan should skip symlinked files instead of following them outside the repo"
    );
    assert!(
        output.stdout.is_empty(),
        "skipped symlinked files should not produce findings"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_skips_symlinks_into_excluded_repo_paths() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let excluded_dir = repo_root.join(".ralph/cache");
    std::fs::create_dir_all(&excluded_dir).expect("create excluded dir");
    let secret_value = ["sk_live_", "abcdefghijklmnop"].concat();
    std::fs::write(
        excluded_dir.join("secret.md"),
        format!("{}\n", secret_value),
    )
    .expect("write excluded secret fixture");
    symlink(excluded_dir.join("secret.md"), repo_root.join("README.md"))
        .expect("create excluded-path symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", ".ralph/cache/")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(0),
        "public-readiness scan should not follow symlinks into excluded repo paths"
    );
    assert!(
        output.stdout.is_empty(),
        "excluded symlink targets should not produce findings"
    );
}

#[cfg(unix)]
#[test]
fn public_readiness_scan_scans_symlinked_repo_files_that_resolve_within_repo() {
    use std::os::unix::fs::symlink;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir(&repo_root).expect("create temp repo root");
    let docs_dir = repo_root.join("docs");
    std::fs::create_dir(&docs_dir).expect("create docs dir");
    std::fs::write(docs_dir.join("source.txt"), "[broken](missing.md)\n")
        .expect("write symlinked markdown source");
    std::fs::write(repo_root.join("missing.md"), "present\n")
        .expect("write misleading repo-root target");
    symlink(docs_dir.join("source.txt"), repo_root.join("README.md"))
        .expect("create in-repo markdown symlink");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("links")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should still inspect symlinked files that resolve within the repo"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "README.md: missing target -> missing.md",
        "scanner should resolve symlinked markdown links from the file's canonical location"
    );
}

#[test]
fn public_readiness_scan_scans_allowlisted_ralph_markdown_links() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/public_readiness_scan.sh",
        "scripts/lib/public_readiness_scan.py",
        "scripts/lib/release_policy.sh",
        "scripts/lib/ralph-shell.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    write_file(
        &repo_root.join(".ralph/README.md"),
        "[broken](./definitely-missing-file.md)\n",
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("links")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness link scan over allowlisted .ralph file");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should inspect allowlisted .ralph markdown files"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(".ralph/README.md: missing target -> ./definitely-missing-file.md"),
        "link scan should report missing targets inside allowlisted .ralph files\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_scans_allowlisted_ralph_files_for_secrets() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();
    let secret_token = ["gh", "p_12345678901234567890"].concat();

    for relative_path in [
        "scripts/lib/public_readiness_scan.sh",
        "scripts/lib/public_readiness_scan.py",
        "scripts/lib/release_policy.sh",
        "scripts/lib/ralph-shell.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    write_file(
        &repo_root.join(".ralph/config.jsonc"),
        &format!("token: {secret_token}\n"),
    );

    let output = Command::new("bash")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.sh"))
        .arg("secrets")
        .current_dir(repo_root)
        .output()
        .expect("run public-readiness secret scan over allowlisted .ralph file");

    assert_eq!(
        output.status.code(),
        Some(1),
        "public-readiness scan should inspect allowlisted .ralph files for secrets"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = format!(".ralph/config.jsonc:1: github_classic_token: {secret_token}");
    assert!(
        stdout.contains(&expected),
        "secret scan should report secrets inside allowlisted .ralph files\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_injected_secret_in_scan_helper_source() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("scripts/lib")).expect("create scripts/lib dir");
    let scan_source = read_repo_file("scripts/lib/public_readiness_scan.py");
    std::fs::write(
        repo_root.join("scripts/lib/public_readiness_scan.py"),
        format!("# {secret_token}\n{scan_source}"),
    )
    .expect("write injected scan helper source");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.py"))
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper against injected source");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should not file-wide allowlist the scan helper source"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/lib/public_readiness_scan.py:") && stdout.contains(&secret_token),
        "injected helper secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_same_line_secret_in_security_docs_allowlist() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("docs/features")).expect("create docs/features dir");
    let aws_example = ["AKIA", "IOSFODNN7EXAMPLE"].concat();
    let exact_allowlisted_line =
        format!("| **AWS Keys** | AKIA-prefixed access keys | `{aws_example}` → `[REDACTED]` |");
    std::fs::write(
        repo_root.join("docs/features/security.md"),
        format!("{exact_allowlisted_line} {secret_token}\n"),
    )
    .expect("write security.md fixture");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan over same-line injected security docs secret");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject same-line injected secrets in allowlisted docs lines"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("docs/features/security.md:") && stdout.contains(&secret_token),
        "same-line injected docs secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_same_line_secret_in_scan_helper_source() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    let secret_token = ["gh", "p_12345678901234567890"].concat();
    std::fs::create_dir_all(repo_root.join("scripts/lib")).expect("create scripts/lib dir");

    let scan_source = read_repo_file("scripts/lib/public_readiness_scan.py");
    let target_line = "AWS_DOCS_ALLOWLIST_LINE = (";
    let injected_line = format!("{target_line}  # {secret_token}");
    let injected_source = scan_source.replacen(target_line, &injected_line, 1);
    assert_ne!(
        injected_source, scan_source,
        "fixture should replace the targeted scan-helper source line"
    );
    std::fs::write(
        repo_root.join("scripts/lib/public_readiness_scan.py"),
        injected_source,
    )
    .expect("write injected scan helper source");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(repo_root.join("scripts/lib/public_readiness_scan.py"))
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan helper against same-line injected source");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject same-line injected secrets in scan-helper source"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/lib/public_readiness_scan.py:") && stdout.contains(&secret_token),
        "same-line injected scan-helper secret should be reported\nstdout:\n{}",
        stdout
    );
}

#[test]
fn public_readiness_scan_rejects_private_key_in_pre_public_check_script() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path().join("repo");
    std::fs::create_dir_all(repo_root.join("scripts")).expect("create scripts dir");
    std::fs::write(
        repo_root.join("scripts/pre-public-check.sh"),
        format!("-----BEGIN {} PRIVATE KEY-----\n", "RSA"),
    )
    .expect("write pre-public-check fixture");
    std::fs::write(repo_root.join("README.md"), "ok\n").expect("write readme fixture");

    let output = Command::new("python3")
        .arg(public_readiness_scan_python_path())
        .arg("secrets")
        .arg(&repo_root)
        .env("RALPH_PUBLIC_SCAN_EXCLUDES", "")
        .output()
        .expect("run public-readiness scan over pre-public-check fixture");

    assert_eq!(
        output.status.code(),
        Some(1),
        "secret scan should reject private keys in pre-public-check.sh"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!(
            "scripts/pre-public-check.sh:1: private_key: BEGIN {} PRIVATE KEY",
            "RSA"
        )),
        "private key in pre-public-check.sh should be reported\nstdout:\n{}",
        stdout
    );
}

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

#[test]
fn release_policy_rejects_rename_from_disallowed_path_to_release_metadata() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/ralph-shell.sh",
        "scripts/lib/release_policy.sh",
        "scripts/pre-public-check.sh",
        "CHANGELOG.md",
    ] {
        copy_repo_file(relative_path, repo_root);
    }

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
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    std::fs::remove_file(repo_root.join("CHANGELOG.md")).expect("remove changelog destination");
    Command::new("git")
        .args(["mv", "scripts/pre-public-check.sh", "CHANGELOG.md"])
        .current_dir(repo_root)
        .output()
        .expect("rename script into changelog path");

    let shell = format!(
        "REPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\ndirty=$(release_collect_dirty_lines {root:?})\nrelease_assert_dirty_paths_allowed \"$dirty\"\n",
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run release metadata assertion over rename");

    assert!(
        !output.status.success(),
        "release metadata assertion should reject renames from disallowed paths\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("scripts/pre-public-check.sh"),
        "rename rejection should keep the disallowed source path visible\noutput:\n{}",
        combined
    );
}

#[test]
fn release_policy_keeps_rename_into_ignored_dirty_paths_visible() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let repo_root = temp_dir.path();

    for relative_path in [
        "scripts/lib/ralph-shell.sh",
        "scripts/lib/release_policy.sh",
        "scripts/pre-public-check.sh",
    ] {
        copy_repo_file(relative_path, repo_root);
    }
    std::fs::create_dir_all(repo_root.join(".ralph")).expect("create .ralph dir");

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
        .args(["add", "-A"])
        .current_dir(repo_root)
        .output()
        .expect("stage repo");
    Command::new("git")
        .args(["commit", "-m", "fixture"])
        .current_dir(repo_root)
        .output()
        .expect("commit fixture repo");

    Command::new("git")
        .args(["mv", "scripts/pre-public-check.sh", ".ralph/trust.json"])
        .current_dir(repo_root)
        .output()
        .expect("rename script into ignored dirty path");

    let shell = format!(
        "REPO_ROOT={root:?}\nsource {shell_path:?}\nsource {policy_path:?}\ndirty=$(release_collect_dirty_lines {root:?})\nrelease_filter_dirty_lines \"$dirty\"\n",
        root = repo_root,
        shell_path = repo_root.join("scripts/lib/ralph-shell.sh"),
        policy_path = repo_root.join("scripts/lib/release_policy.sh"),
    );
    let output = Command::new("bash")
        .arg("-lc")
        .arg(shell)
        .current_dir(repo_root)
        .output()
        .expect("run dirty-line filter over rename into ignored path");

    assert!(
        output.status.success(),
        "dirty-line filter command should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scripts/pre-public-check.sh"),
        "rename filtering should keep the disallowed source path visible even when destination is ignored\nstdout:\n{}",
        stdout
    );
}

#[test]
fn release_scripts_do_not_blanket_ignore_all_ralph_paths_in_cleanliness_checks() {
    let verify_pipeline = read_repo_file("scripts/lib/release_verify_pipeline.sh");
    let release_pipeline = read_repo_file("scripts/lib/release_pipeline.sh");

    for script in [&verify_pipeline, &release_pipeline] {
        assert!(
            !script.contains("grep -vE '^..[[:space:]]+\\.ralph/'"),
            "release cleanliness checks should not blanket-ignore all .ralph paths"
        );
        assert!(
            script.contains("release_filter_dirty_lines"),
            "release cleanliness checks should reuse the shared dirty-path filter"
        );
    }
}

#[test]
fn public_readiness_scan_rejects_help_with_extra_args() {
    let output = Command::new("bash")
        .arg(public_readiness_scan_shell_helper_path())
        .arg("--help")
        .arg("extra")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan helper should reject unexpected positional arguments"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:"),
        "helper should print usage for invalid argument combinations"
    );
}

#[test]
fn public_readiness_scan_rejects_links_with_extra_args() {
    let output = Command::new("bash")
        .arg(public_readiness_scan_shell_helper_path())
        .arg("links")
        .arg("extra")
        .output()
        .expect("run public-readiness scan helper");

    assert_eq!(
        output.status.code(),
        Some(2),
        "public-readiness scan helper should reject extra args for normal modes"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:"),
        "helper should print usage for invalid argument combinations"
    );
}
