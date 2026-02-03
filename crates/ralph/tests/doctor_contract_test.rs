//! Contract tests for `ralph doctor` output and diagnostics.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

mod test_support;

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

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

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn ralph_cmd() -> Command {
    let mut cmd = Command::new(ralph_bin());
    cmd.env_remove("RUST_LOG");
    // Remove repo root override so child processes don't inherit the parent's repo
    cmd.env_remove("RALPH_REPO_ROOT_OVERRIDE");
    cmd
}

/// Create a ralph command with repo root override set to the given directory.
/// This ensures proper isolation when running in temp directories that may be
/// nested within the actual repo tree.
fn ralph_cmd_in_dir(dir: &std::path::Path) -> Command {
    let mut cmd = ralph_cmd();
    cmd.env("RALPH_REPO_ROOT_OVERRIDE", dir);
    cmd
}

#[test]
fn doctor_passes_in_clean_env() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    // Setup valid repo
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    // Missing upstream is now a warning, not a failure, so doctor should pass
    assert!(output.status.success());
    assert!(combined.contains("OK") && combined.contains("git binary found"));
    assert!(combined.contains("OK") && combined.contains("queue valid"));
    assert!(combined.contains("WARN") && combined.contains("no upstream configured"));
    Ok(())
}

#[test]
fn doctor_fails_when_queue_missing() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // No ralph init

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(combined.contains("FAIL") && combined.contains("queue file missing"));
    Ok(())
}

#[test]
fn doctor_warns_on_missing_upstream() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    // Setup valid repo without upstream
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    // Should succeed with a warning about missing upstream
    assert!(output.status.success());
    assert!(combined.contains("WARN") && combined.contains("no upstream configured"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_runner_binary() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    // Setup valid repo
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Configure a non-existent runner binary
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = r#"{"version":1,"agent":{"runner":"opencode","opencode_bin":"this-binary-does-not-exist-xyz123"}}"#;
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    // Should fail
    assert!(!output.status.success());
    // Should report the failure with the binary name
    assert!(combined_output.contains("this-binary-does-not-exist-xyz123"));
    assert!(combined_output.contains("FAIL"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_gemini_binary() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let config_path = dir.path().join(".ralph/config.json");
    let config_content = r#"{"version":1,"agent":{"runner":"gemini","gemini_bin":"this-gemini-does-not-exist-xyz123"}}"#;
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    assert!(!output.status.success());
    assert!(combined_output.contains("this-gemini-does-not-exist-xyz123"));
    assert!(combined_output.contains("FAIL"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_claude_binary() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let config_path = dir.path().join(".ralph/config.json");
    let config_content = r#"{"version":1,"agent":{"runner":"claude","claude_bin":"this-claude-does-not-exist-xyz123"}}"#;
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    assert!(!output.status.success());
    assert!(combined_output.contains("this-claude-does-not-exist-xyz123"));
    assert!(combined_output.contains("FAIL"));
    Ok(())
}

#[test]
fn doctor_fails_with_invalid_done_archive() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Corrupt done.json
    let done_path = dir.path().join(".ralph/done.json");
    std::fs::write(&done_path, "invalid json: { [")?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(!output.status.success());
    assert!(
        combined.contains("FAIL")
            && (combined.contains("done archive validation failed")
                || combined.contains("failed to load done archive"))
    );
    Ok(())
}

#[test]
fn doctor_warns_when_instruction_files_missing() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    // Setup valid repo
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Configure instruction file injection with a missing path.
    let config_path = dir.path().join(".ralph/config.json");
    let config_content =
        r#"{"version":1,"agent":{"instruction_files":["missing-global-agents.md"]}}"#;
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(output.status.success());
    assert!(combined.contains("WARN") && combined.contains("instruction_files"));
    Ok(())
}

#[test]
fn doctor_passes_with_runner_that_only_supports_help() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create a stub runner that only supports --help (not --version)
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_script = r#"#!/bin/bash
case "$1" in
  --help) echo "Usage: test-runner [options]"; exit 0 ;;
  *) exit 1 ;;
esac
"#;
    let runner_path = bin_dir.join("test-runner-help-only");
    std::fs::write(&runner_path, runner_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    // Configure the stub runner
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"opencode","opencode_bin":"{}"}}}}"#,
        runner_path.display()
    );
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    // Should pass because --help works even though --version doesn't
    assert!(
        output.status.success(),
        "doctor should pass when runner supports --help"
    );
    assert!(
        combined.contains("runner binary") && combined.contains("found"),
        "should report runner as found"
    );
    Ok(())
}

#[test]
fn doctor_passes_with_runner_that_only_supports_v_flag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create a stub runner that only supports -V (not --version)
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_script = r#"#!/bin/bash
case "$1" in
  -V) echo "test-runner 1.0.0"; exit 0 ;;
  --version) exit 1 ;;
  --help) exit 1 ;;
  *) exit 1 ;;
esac
"#;
    let runner_path = bin_dir.join("test-runner-v-only");
    std::fs::write(&runner_path, runner_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    // Configure the stub runner
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"claude","claude_bin":"{}"}}}}"#,
        runner_path.display()
    );
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    // Should pass because -V works even though --version doesn't
    assert!(
        output.status.success(),
        "doctor should pass when runner supports -V"
    );
    assert!(
        combined.contains("runner binary") && combined.contains("found"),
        "should report runner as found"
    );
    Ok(())
}

#[test]
fn doctor_fails_with_runner_that_has_no_valid_flags() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create a stub runner that rejects all version/help flags
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_script = r#"#!/bin/bash
exit 1
"#;
    let runner_path = bin_dir.join("test-runner-no-flags");
    std::fs::write(&runner_path, runner_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    // Configure the stub runner
    let config_path = dir.path().join(".ralph/config.json");
    let config_content = format!(
        r#"{{"version":1,"agent":{{"runner":"gemini","gemini_bin":"{}"}}}}"#,
        runner_path.display()
    );
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should fail because no flags work
    assert!(
        !output.status.success(),
        "doctor should fail when runner has no valid flags"
    );
    assert!(combined.contains("FAIL"), "should report failure");
    assert!(
        combined.contains("gemini_bin"),
        "error should mention the config key"
    );
    Ok(())
}

#[test]
fn doctor_error_includes_config_key_hint() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Configure a non-existent runner binary
    let config_path = dir.path().join(".ralph/config.json");
    let config_content =
        r#"{"version":1,"agent":{"runner":"codex","codex_bin":"/nonexistent/path/codex"}}"#;
    std::fs::write(&config_path, config_content)?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should fail and include the config key in the guidance
    assert!(!output.status.success());
    assert!(
        combined.contains("codex_bin"),
        "error should mention codex_bin config key"
    );
    assert!(
        combined.contains(".ralph/config.json"),
        "error should mention config file location"
    );
    Ok(())
}
