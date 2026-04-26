//! File-size guard script contract tests.
//!
//! Purpose:
//! - Verify the behavior of `scripts/check-file-size-limits.sh` and its helper policy scanner.
//!
//! Responsibilities:
//! - Validate help output and argument error behavior.
//! - Validate pass/warn/fail outcomes for small, soft-limit, and hard-limit files.
//! - Validate exclude behavior for machine-owned/generated paths and configurable excludes.
//! - Validate that untracked monitored files are included in policy checks.
//!
//! Not handled here:
//! - Full end-to-end Makefile gate orchestration.
//! - Policy threshold decisions (sourced from AGENTS.md + script defaults).
//!
//! Usage:
//! - Executed as part of the Rust integration-test suite.
//!
//! Invariants/assumptions:
//! - Bash and git are available locally.
//! - Script paths remain stable at `scripts/check-file-size-limits.sh` and
//!   `scripts/lib/file_size_limits.py`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn write_lines(path: &Path, count: usize) {
    let mut body = String::new();
    for index in 0..count {
        body.push_str(&format!("line {index}\n"));
    }
    write_file(path, &body);
}

fn copy_repo_file(repo_root: &Path, temp_repo: &Path, relative_path: &str) {
    let source = repo_root.join(relative_path);
    let content = std::fs::read_to_string(&source)
        .unwrap_or_else(|err| panic!("read {}: {err}", source.display()));
    write_file(&temp_repo.join(relative_path), &content);
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

fn init_temp_repo() -> TempDir {
    let root = repo_root();
    let temp_repo = tempfile::tempdir().expect("create temp repo");
    let repo_path = temp_repo.path();

    copy_repo_file(&root, repo_path, "scripts/check-file-size-limits.sh");
    copy_repo_file(&root, repo_path, "scripts/lib/file_size_limits.py");

    write_file(&repo_path.join("README.md"), "# Temp repo\n");

    git(repo_path, &["init", "-b", "main"]);
    git(repo_path, &["config", "user.name", "Codex"]);
    git(repo_path, &["config", "user.email", "codex@example.com"]);
    git(repo_path, &["add", "."]);
    git(repo_path, &["commit", "-m", "initial"]);

    temp_repo
}

fn run_check_script(temp_repo: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(temp_repo.join("scripts/check-file-size-limits.sh"))
        .args(args)
        .current_dir(temp_repo)
        .output()
        .expect("run check-file-size-limits.sh")
}

fn output_text(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn check_file_size_limits_help_lists_usage_and_exit_codes() {
    let temp_repo = init_temp_repo();

    let output = run_check_script(temp_repo.path(), &["--help"]);
    let (stdout, stderr) = output_text(&output);

    assert!(
        output.status.success(),
        "expected --help to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("Usage:"), "missing usage block\n{stdout}");
    assert!(
        stdout.contains("Exit codes:"),
        "missing exit-codes block\n{stdout}"
    );
}

#[test]
fn check_file_size_limits_passes_when_all_files_are_within_limits() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join("crates/ralph/src/lib.rs"), 12);

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected success status\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("OK: file-size limits within policy"),
        "expected success marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_warns_on_soft_limit_without_failing() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join("docs/guides/large.md"), 801);

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected soft-limit warning to stay non-blocking\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("WARN: soft file-size limit exceeded:"),
        "missing soft warning header\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("docs/guides/large.md"),
        "expected offender path in soft warning\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_fails_on_hard_limit_violation() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join("crates/ralph/src/huge.rs"), 1001);

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(1),
        "expected hard-limit failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("ERROR: hard file-size limit exceeded:"),
        "missing hard-error header\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("crates/ralph/src/huge.rs"),
        "expected offender path in hard error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_ignores_default_generated_path_excludes() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join("schemas/config.schema.json"), 1500);

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "excluded schema path should not fail policy\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("schemas/config.schema.json"),
        "excluded path should not be listed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_ignores_default_ralph_bookkeeping_excludes() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join(".ralph/done.jsonc"), 1500);

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "excluded Ralph bookkeeping path should not fail policy\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains(".ralph/done.jsonc"),
        "excluded bookkeeping path should not be listed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_includes_untracked_monitored_files() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    let untracked_path = repo_path.join("scratch/oversized.md");
    write_lines(&untracked_path, 1001);

    let git_status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .expect("run git status --porcelain");
    assert!(git_status.status.success(), "git status should succeed");
    assert!(
        String::from_utf8_lossy(&git_status.stdout).contains("?? scratch"),
        "expected oversized file to remain untracked"
    );

    let output = run_check_script(repo_path, &[]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(1),
        "untracked oversized markdown should fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("scratch/oversized.md"),
        "expected untracked offender path in output\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_supports_configurable_exclude_glob() {
    let temp_repo = init_temp_repo();
    let repo_path = temp_repo.path();

    write_lines(&repo_path.join("generated/manual-long.md"), 1001);

    let output = run_check_script(repo_path, &["--exclude-glob", "generated/**"]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "custom exclude should suppress the generated path\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("generated/manual-long.md"),
        "custom-excluded path should not appear\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_file_size_limits_invalid_arg_exits_with_usage_error() {
    let temp_repo = init_temp_repo();

    let output = run_check_script(temp_repo.path(), &["--definitely-not-valid"]);
    let (stdout, stderr) = output_text(&output);

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid arguments should return usage exit code\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    assert!(
        combined.contains("usage:"),
        "expected usage text for invalid args\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
