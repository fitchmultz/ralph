//! Rust toolchain verification script contract tests.
//!
//! Purpose:
//! - Rust toolchain verification script contract tests.
//!
//! Responsibilities:
//! - Verify the toolchain script exposes the documented operator surface.
//! - Guard the checked-in Rust baseline success path.
//! - Guard the failure path for crate rust-version drift from rust-toolchain.toml.
//!
//! Not handled here:
//! - Exhaustive rustup behavior across every host platform.
//! - Networked toolchain installation or global stable update management.
//!
//! Usage:
//! - Used through the integration test harness with `cargo test --test rust_toolchain_script_test`.
//!
//! Invariants/assumptions:
//! - The repository has rustup, the pinned toolchain, and required components installed when local CI runs.
//! - Temporary fixture repositories only need enough structure for scripts/check-rust-toolchain.sh to resolve sources of truth.

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

fn copy_repo_file(repo_root: &Path, temp_repo: &Path, relative_path: &str) {
    let src = repo_root.join(relative_path);
    let dest = temp_repo.join(relative_path);
    let content =
        std::fs::read_to_string(&src).unwrap_or_else(|err| panic!("read {}: {err}", src.display()));
    write_file(&dest, &content);
}

fn script_path(repo_root: &Path) -> PathBuf {
    repo_root.join("scripts/check-rust-toolchain.sh")
}

#[test]
fn rust_toolchain_script_help_documents_operator_contract() {
    let repo_root = repo_root();
    let output = Command::new("bash")
        .arg(script_path(&repo_root))
        .arg("--help")
        .current_dir(&repo_root)
        .output()
        .expect("run toolchain script help");

    assert!(
        output.status.success(),
        "help failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--fail-on-global-stable-drift"));
    assert!(stdout.contains("make rust-toolchain-check"));
    assert!(stdout.contains("make rust-toolchain-drift-check"));
    assert!(stdout.contains("Exit codes:"));
}

#[test]
fn rust_toolchain_script_passes_for_checked_in_baseline() {
    let repo_root = repo_root();
    let output = Command::new("bash")
        .arg(script_path(&repo_root))
        .current_dir(&repo_root)
        .output()
        .expect("run toolchain script");

    assert!(
        output.status.success(),
        "toolchain script failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Repo Rust baseline is internally consistent"));
    assert!(stdout.contains("Skipped global stable drift check"));
}

#[test]
fn rust_toolchain_script_fails_when_crate_rust_version_drifts() {
    let repo_root = repo_root();
    let temp_repo = TempDir::new().expect("create temp repo");
    let fixture = temp_repo.path();

    copy_repo_file(&repo_root, fixture, "scripts/check-rust-toolchain.sh");
    copy_repo_file(&repo_root, fixture, "scripts/lib/ralph-shell.sh");
    write_file(
        &fixture.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.95.0\"\ncomponents = [\"rustfmt\", \"clippy\"]\nprofile = \"minimal\"\n",
    );
    write_file(
        &fixture.join("crates/ralph/Cargo.toml"),
        "[package]\nname = \"ralph-agent-loop\"\nrust-version = \"1.94\"\n",
    );

    let output = Command::new("bash")
        .arg(fixture.join("scripts/check-rust-toolchain.sh"))
        .current_dir(fixture)
        .output()
        .expect("run toolchain script against fixture");

    assert!(
        !output.status.success(),
        "drifted fixture should fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("crate rust-version drifted"),
        "expected rust-version drift diagnostic\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
