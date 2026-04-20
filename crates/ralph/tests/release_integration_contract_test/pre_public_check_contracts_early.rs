//! `pre-public-check.sh` contract coverage (early section).
//!
//! Responsibilities:
//! - Source snapshot / `--allow-no-git` behavior and markdown discovery wiring.
//! - CI surface and git worktree prerequisites.

use std::process::Command;

use super::support::{copy_repo_file, read_repo_file, write_file};

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
