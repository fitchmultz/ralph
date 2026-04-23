//! Shared CLI bundle script contract tests.
//!
//! Purpose:
//! - Verify `scripts/ralph-cli-bundle.sh` remains the canonical CLI build entrypoint.
//!
//! Responsibilities:
//! - Confirm the bundling script rebuilds stale binaries and reuses fresh ones.
//! - Exercise the pinned-toolchain path using fake `rustup`/`cargo` fixtures.
//! - Guard against regressions that silently reuse stale binaries.
//!
//! Scope:
//! - Script behavior only; not full app bundling or release packaging.
//!
//! Usage:
//! - Executed as part of the Rust integration-test suite.
//!
//! Invariants/assumptions:
//! - The repo-local bundle script lives at `scripts/ralph-cli-bundle.sh`.
//! - PATH-mutating fake toolchain tests must hold the shared integration-test env lock.

mod test_support;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

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

fn bundle_script() -> PathBuf {
    repo_root().join("scripts").join("ralph-cli-bundle.sh")
}

#[cfg(unix)]
fn run_bundle_script(args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new("/bin/bash")
        .arg(bundle_script())
        .args(args)
        .output()
        .expect("execute ralph-cli-bundle.sh");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[cfg(unix)]
fn target_binary_path(target_triple: &str) -> PathBuf {
    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    repo_root()
        .join("target")
        .join(target_triple)
        .join("debug")
        .join(bin_name)
}

#[cfg(unix)]
fn write_fake_toolchain(bin_dir: &Path, cargo_log: &Path) {
    test_support::create_executable_script(
        bin_dir,
        "cargo",
        &format!(
            "#!/bin/sh\nprintf '%s\n' \"$*\" >> '{}'\nexit 0\n",
            cargo_log.display()
        ),
    )
    .expect("write fake cargo");
    test_support::create_executable_script(bin_dir, "rustc", "#!/bin/sh\nexit 0\n")
        .expect("write fake rustc");
    test_support::create_executable_script(
        bin_dir,
        "rustup",
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"which\" ] && [ \"$2\" = \"rustc\" ]; then\n  printf '%s\\n' '{}'\n  exit 0\nfi\necho \"unexpected rustup args: $*\" >&2\nexit 1\n",
            bin_dir.join("rustc").display()
        ),
    )
    .expect("write fake rustup");
}

#[cfg(unix)]
#[test]
fn bundle_script_rebuilds_even_when_binary_already_exists() {
    let _lock = test_support::env_lock().lock().expect("env lock");
    let temp = TempDir::new().expect("create temp dir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create fake toolchain dir");
    let cargo_log = temp.path().join("cargo.log");
    write_fake_toolchain(&bin_dir, &cargo_log);

    let target_triple = format!(
        "cli-bundle-script-test-{}",
        temp.path()
            .file_name()
            .expect("temp dir file name")
            .to_string_lossy()
    );
    let binary_path = target_binary_path(&target_triple);
    if let Some(parent) = binary_path.parent() {
        std::fs::create_dir_all(parent).expect("create test target dir");
        test_support::create_executable_script(parent, "ralph", "#!/bin/sh\nexit 0\n")
            .expect("create stale binary fixture");
    }
    let touch_status = Command::new("touch")
        .arg("-t")
        .arg("200001010101")
        .arg(&binary_path)
        .status()
        .expect("mark binary as older than repo inputs");
    assert!(touch_status.success(), "touch should succeed");

    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        run_bundle_script(&[
            "--configuration",
            "Debug",
            "--target",
            &target_triple,
            "--print-path",
        ])
    });

    let _ = std::fs::remove_dir_all(repo_root().join("target").join(&target_triple));

    assert!(
        status.success(),
        "expected bundle script to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let printed_path = stdout
        .lines()
        .last()
        .expect("bundle script should print the resolved path");
    assert_eq!(printed_path.trim(), binary_path.display().to_string());

    let cargo_invocations = std::fs::read_to_string(&cargo_log).expect("read fake cargo log");
    assert!(
        cargo_invocations.contains("build -p ralph-agent-loop --locked --target"),
        "expected fake cargo to receive a build invocation\nstdout:\n{stdout}\nstderr:\n{stderr}\nlog:\n{cargo_invocations}"
    );
    assert!(
        cargo_invocations.contains(&target_triple),
        "expected fake cargo to receive the requested target triple\nlog:\n{cargo_invocations}"
    );
}

#[cfg(unix)]
#[test]
fn bundle_script_reuses_fresh_binary_without_rebuilding() {
    let _lock = test_support::env_lock().lock().expect("env lock");
    let temp = TempDir::new().expect("create temp dir");
    let bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create fake toolchain dir");
    let cargo_log = temp.path().join("cargo.log");
    write_fake_toolchain(&bin_dir, &cargo_log);

    let target_triple = format!(
        "cli-bundle-script-fresh-{}",
        temp.path()
            .file_name()
            .expect("temp dir file name")
            .to_string_lossy()
    );
    let binary_path = target_binary_path(&target_triple);
    if let Some(parent) = binary_path.parent() {
        std::fs::create_dir_all(parent).expect("create test target dir");
        test_support::create_executable_script(parent, "ralph", "#!/bin/sh\nexit 0\n")
            .expect("create fresh binary fixture");
    }
    let touch_status = Command::new("touch")
        .arg("-t")
        .arg("209901010101")
        .arg(&binary_path)
        .status()
        .expect("mark binary as newer than repo inputs");
    assert!(touch_status.success(), "touch should succeed");

    let (status, stdout, stderr) = test_support::with_prepend_path(&bin_dir, || {
        run_bundle_script(&[
            "--configuration",
            "Debug",
            "--target",
            &target_triple,
            "--print-path",
        ])
    });

    let _ = std::fs::remove_dir_all(repo_root().join("target").join(&target_triple));

    assert!(
        status.success(),
        "expected bundle script to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let printed_path = stdout
        .lines()
        .last()
        .expect("bundle script should print the resolved path");
    assert_eq!(printed_path.trim(), binary_path.display().to_string());

    let cargo_invocations = std::fs::read_to_string(&cargo_log).unwrap_or_default();
    assert!(
        cargo_invocations.trim().is_empty(),
        "expected fresh binary reuse to skip cargo\nstdout:\n{stdout}\nstderr:\n{stderr}\nlog:\n{cargo_invocations}"
    );
}
