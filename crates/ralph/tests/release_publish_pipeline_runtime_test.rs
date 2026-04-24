//! Release publish-pipeline runtime contract tests.
//!
//! Purpose:
//! - Release publish-pipeline runtime contract tests.
//!
//! Responsibilities:
//! - Exercise shell helpers in `scripts/lib/release_publish_pipeline.sh` with fake CLIs.
//! - Guard missing-release probing and crates.io publication probing against false positives.
//!
//! Not handled here:
//! - Real GitHub or crates.io interactions.
//! - End-to-end release execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Bash is available for sourcing the release publish pipeline.
//! - Tests can override `PATH` with fake `gh` and `cargo` executables.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn repo_root() -> PathBuf {
    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(OsStr::new("deps")) {
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

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    let mut permissions = fs::metadata(path)
        .unwrap_or_else(|err| panic!("stat {}: {err}", path.display()))
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .unwrap_or_else(|err| panic!("chmod {}: {err}", path.display()));
}

fn combined_path(temp_dir: &Path) -> String {
    let inherited_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", temp_dir.display(), inherited_path)
}

fn gh_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

mode="${FAKE_GH_MODE:-missing}"
if [ "${1:-}" != "release" ] || [ "${2:-}" != "view" ]; then
  echo "unexpected gh invocation: $*" >&2
  exit 64
fi

case "$mode" in
  missing)
    exit 1
    ;;
  draft)
    printf 'true\n'
    ;;
  published)
    printf 'false\n'
    ;;
  *)
    echo "unsupported FAKE_GH_MODE=$mode" >&2
    exit 65
    ;;
esac
"#
}

fn cargo_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

log_file="${FAKE_CARGO_LOG:-}"
repo_root="${FAKE_CARGO_REPO_ROOT:-}"
command="${1:-}"
shift || true

log() {
  if [ -n "$log_file" ]; then
    printf '%s\n' "$1" >> "$log_file"
  fi
}

case "$command" in
  info)
    log "info cwd=$PWD args=$*"
    if [ -n "$repo_root" ] && [ "$PWD" = "$repo_root" ]; then
      echo "cargo info ran from repo root unexpectedly" >&2
      exit 93
    fi
    case "${FAKE_CARGO_INFO_MODE:-missing}" in
      published)
        exit 0
        ;;
      missing)
        exit 1
        ;;
      *)
        echo "unsupported FAKE_CARGO_INFO_MODE=${FAKE_CARGO_INFO_MODE:-}" >&2
        exit 66
        ;;
    esac
    ;;
  package)
    log "package cwd=$PWD args=$*"
    exit 0
    ;;
  publish)
    log "publish cwd=$PWD args=$*"
    exit 0
    ;;
  *)
    echo "unexpected cargo invocation: $command $*" >&2
    exit 64
    ;;
esac
"#
}

#[test]
fn release_query_reports_missing_without_traceback_noise() {
    let temp_dir = TempDir::new().expect("create temp dir");
    write_executable(&temp_dir.path().join("gh"), gh_script());

    let script_path = repo_root().join("scripts/lib/release_publish_pipeline.sh");
    let output = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "source '{}' && VERSION=0.3.0 && release_query_github_release_state",
            script_path.display()
        ))
        .env("FAKE_GH_MODE", "missing")
        .env("PATH", combined_path(temp_dir.path()))
        .output()
        .expect("run release_query_github_release_state");

    assert!(
        output.status.success(),
        "expected helper to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "missing");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "stderr should be empty:\n{stderr}"
    );
    assert!(
        !stderr.contains("Traceback") && !stderr.contains("JSONDecodeError"),
        "stderr should not leak parser tracebacks:\n{stderr}"
    );
}

#[test]
fn release_query_maps_draft_and_published_states() {
    let temp_dir = TempDir::new().expect("create temp dir");
    write_executable(&temp_dir.path().join("gh"), gh_script());
    let script_path = repo_root().join("scripts/lib/release_publish_pipeline.sh");

    for (mode, expected) in [("draft", "draft"), ("published", "published")] {
        let output = Command::new("bash")
            .arg("-c")
            .arg(format!(
                "source '{}' && VERSION=0.3.0 && release_query_github_release_state",
                script_path.display()
            ))
            .env("FAKE_GH_MODE", mode)
            .env("PATH", combined_path(temp_dir.path()))
            .output()
            .expect("run release_query_github_release_state");

        assert!(
            output.status.success(),
            "expected helper to succeed for mode {mode}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
        assert!(
            String::from_utf8_lossy(&output.stderr).trim().is_empty(),
            "stderr should stay empty for mode {mode}"
        );
    }
}

#[test]
fn release_crate_probe_uses_isolated_directory() {
    let temp_dir = TempDir::new().expect("create temp dir");
    write_executable(&temp_dir.path().join("cargo"), cargo_script());

    let script_path = repo_root().join("scripts/lib/release_publish_pipeline.sh");
    let output = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "source '{}' && CRATE_PACKAGE_NAME=ralph-agent-loop && VERSION=0.3.0 && if release_crate_is_published; then echo published; else echo missing; fi",
            script_path.display()
        ))
        .env("FAKE_CARGO_INFO_MODE", "missing")
        .env("FAKE_CARGO_REPO_ROOT", repo_root())
        .env("PATH", combined_path(temp_dir.path()))
        .output()
        .expect("run release_crate_is_published");

    assert!(
        output.status.success(),
        "crate probe wrapper should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "missing");
    assert!(
        String::from_utf8_lossy(&output.stderr).trim().is_empty(),
        "stderr should stay empty for isolated crate probes"
    );
}

#[test]
fn release_publish_crate_rechecks_remote_state_before_skipping() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let fake_bin_dir = temp_dir.path().join("bin");
    fs::create_dir(&fake_bin_dir).expect("create fake bin dir");
    write_executable(&fake_bin_dir.join("cargo"), cargo_script());

    let repo_root = repo_root();
    let scripts_dir = repo_root.join("scripts");
    let release_state_path = repo_root.join("scripts/lib/release_state.sh");
    let release_publish_path = repo_root.join("scripts/lib/release_publish_pipeline.sh");
    let shell_path = repo_root.join("scripts/lib/ralph-shell.sh");
    let scratch_dir = temp_dir.path().join("scratch");
    fs::create_dir(&scratch_dir).expect("create scratch dir");
    let state_file = scratch_dir.join("state.env");
    let log_file = scratch_dir.join("cargo.log");
    let release_notes = scratch_dir.join("notes.md");
    fs::write(&release_notes, "notes\n").expect("write release notes");

    let command = format!(
        "SCRIPT_DIR='{scripts_dir}' \
         source '{shell_path}' && \
         source '{release_state_path}' && \
         source '{release_publish_path}' && \
         VERSION=0.3.0 && \
         CRATE_PACKAGE_NAME=ralph-agent-loop && \
         REPO_ROOT='{repo_root}' && \
         TRANSACTION_DIR='{transaction_dir}' && \
         STATE_FILE='{state_file}' && \
         RELEASE_NOTES_FILE='{release_notes}' && \
         STARTED_AT=2026-03-25T00:00:00Z && \
         RELEASE_STATUS=completed && \
         CRATE_PUBLISHED=1 && \
         release_publish_crate && \
         printf 'status=%s crate=%s\n' \"$RELEASE_STATUS\" \"$CRATE_PUBLISHED\"",
        scripts_dir = scripts_dir.display(),
        shell_path = shell_path.display(),
        release_state_path = release_state_path.display(),
        release_publish_path = release_publish_path.display(),
        repo_root = repo_root.display(),
        transaction_dir = scratch_dir.display(),
        state_file = state_file.display(),
        release_notes = release_notes.display(),
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(command)
        .env("FAKE_CARGO_INFO_MODE", "missing")
        .env("FAKE_CARGO_REPO_ROOT", repo_root.as_os_str())
        .env("FAKE_CARGO_LOG", log_file.as_os_str())
        .env("PATH", combined_path(&fake_bin_dir))
        .output()
        .expect("run release_publish_crate");

    assert!(
        output.status.success(),
        "release_publish_crate should recover from stale recorded state\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("status=crate_published crate=1"),
        "expected crate publish status update\nstdout:\n{stdout}"
    );

    let cargo_log = fs::read_to_string(&log_file).expect("read cargo log");
    assert!(
        cargo_log.contains("info cwd=") && cargo_log.contains("package cwd="),
        "expected probe and publish commands in cargo log\n{cargo_log}"
    );
    assert!(
        cargo_log.contains("publish cwd=") && cargo_log.matches("publish cwd=").count() >= 2,
        "expected dry-run and real publish invocations\n{cargo_log}"
    );

    let state_contents = fs::read_to_string(&state_file).expect("read release state file");
    assert!(
        state_contents.contains("CRATE_PUBLISHED=1")
            && state_contents.contains("RELEASE_STATUS=crate_published"),
        "expected persisted crate-published state\n{state_contents}"
    );
}

#[test]
fn release_publish_github_release_restores_completed_status_when_already_public() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let repo_root = repo_root();
    let scripts_dir = repo_root.join("scripts");
    let release_state_path = repo_root.join("scripts/lib/release_state.sh");
    let release_publish_path = repo_root.join("scripts/lib/release_publish_pipeline.sh");
    let shell_path = repo_root.join("scripts/lib/ralph-shell.sh");
    let scratch_dir = temp_dir.path().join("scratch");
    fs::create_dir(&scratch_dir).expect("create scratch dir");
    let state_file = scratch_dir.join("state.env");
    let release_notes = scratch_dir.join("notes.md");
    fs::write(&release_notes, "notes\n").expect("write release notes");

    let command = format!(
        "SCRIPT_DIR='{scripts_dir}' \
         source '{shell_path}' && \
         source '{release_state_path}' && \
         source '{release_publish_path}' && \
         VERSION=0.3.0 && \
         REPO_ROOT='{repo_root}' && \
         TRANSACTION_DIR='{transaction_dir}' && \
         STATE_FILE='{state_file}' && \
         RELEASE_NOTES_FILE='{release_notes}' && \
         STARTED_AT=2026-03-25T00:00:00Z && \
         RELEASE_STATUS=crate_published && \
         GITHUB_RELEASE_PUBLISHED=1 && \
         release_publish_github_release && \
         printf 'status=%s github=%s\n' \"$RELEASE_STATUS\" \"$GITHUB_RELEASE_PUBLISHED\"",
        scripts_dir = scripts_dir.display(),
        shell_path = shell_path.display(),
        release_state_path = release_state_path.display(),
        release_publish_path = release_publish_path.display(),
        repo_root = repo_root.display(),
        transaction_dir = scratch_dir.display(),
        state_file = state_file.display(),
        release_notes = release_notes.display(),
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()
        .expect("run release_publish_github_release");

    assert!(
        output.status.success(),
        "release_publish_github_release should normalize completed state\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("status=completed github=1"),
        "expected completed status when GitHub release is already public\nstdout:\n{stdout}"
    );

    let state_contents = fs::read_to_string(&state_file).expect("read release state file");
    assert!(
        state_contents.contains("RELEASE_STATUS=completed")
            && state_contents.contains("GITHUB_RELEASE_PUBLISHED=1"),
        "expected persisted completed state\n{state_contents}"
    );
}
