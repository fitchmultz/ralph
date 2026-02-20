//! Contract tests for `ralph doctor` output and diagnostics.

use anyhow::Result;
use std::process::Command;

mod test_support;

fn ralph_cmd() -> Command {
    let mut cmd = Command::new(test_support::ralph_bin());
    cmd.env_remove("RUST_LOG");
    cmd
}

/// Create a ralph command scoped to the given directory.
fn ralph_cmd_in_dir(dir: &std::path::Path) -> Command {
    let mut cmd = ralph_cmd();
    cmd.current_dir(dir);
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

#[test]
fn doctor_json_output_format() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON from stdout (log output goes to stderr)
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("JSON should be valid. Got stdout: {}", stdout));

    // Verify structure
    assert!(
        json.get("success").is_some(),
        "JSON should have 'success' field"
    );
    assert!(
        json.get("checks").is_some(),
        "JSON should have 'checks' field"
    );
    assert!(
        json.get("summary").is_some(),
        "JSON should have 'summary' field"
    );

    // Verify summary fields
    let summary = json.get("summary").unwrap();
    assert!(
        summary.get("total").is_some(),
        "summary should have 'total' field"
    );
    assert!(
        summary.get("passed").is_some(),
        "summary should have 'passed' field"
    );
    assert!(
        summary.get("warnings").is_some(),
        "summary should have 'warnings' field"
    );
    assert!(
        summary.get("errors").is_some(),
        "summary should have 'errors' field"
    );
    assert!(
        summary.get("fixes_applied").is_some(),
        "summary should have 'fixes_applied' field"
    );
    assert!(
        summary.get("fixes_failed").is_some(),
        "summary should have 'fixes_failed' field"
    );

    // Verify checks is an array
    let checks = json
        .get("checks")
        .unwrap()
        .as_array()
        .expect("checks should be an array");
    assert!(!checks.is_empty(), "should have at least one check");

    // Verify check structure
    let first_check = &checks[0];
    assert!(
        first_check.get("category").is_some(),
        "check should have 'category' field"
    );
    assert!(
        first_check.get("check").is_some(),
        "check should have 'check' field"
    );
    assert!(
        first_check.get("severity").is_some(),
        "check should have 'severity' field"
    );
    assert!(
        first_check.get("message").is_some(),
        "check should have 'message' field"
    );
    assert!(
        first_check.get("fix_available").is_some(),
        "check should have 'fix_available' field"
    );

    Ok(())
}

#[test]
fn doctor_json_output_with_failed_check() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Don't run ralph init - so queue file will be missing

    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON from stdout (log output goes to stderr)
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("JSON should be valid");

    // Verify failure is reported
    assert_eq!(
        json["success"], false,
        "success should be false when checks fail"
    );
    assert!(
        json["summary"]["errors"].as_u64().unwrap_or(0) > 0,
        "should have errors"
    );

    // Find the queue check error
    let checks = json["checks"].as_array().unwrap();
    let queue_error = checks
        .iter()
        .find(|c| c["category"] == "queue" && c["severity"] == "Error");
    assert!(queue_error.is_some(), "should have a queue error check");

    Ok(())
}

#[test]
fn doctor_auto_fix_removes_orphaned_locks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create an orphaned lock directory with a dead PID
    let lock_dir = dir.path().join(".ralph/lock/orphaned-test-lock");
    std::fs::create_dir_all(&lock_dir)?;
    let owner_file = lock_dir.join("owner");
    std::fs::write(&owner_file, "pid:999999\nstarted:1234567890\n")?;

    // Verify lock directory exists before running doctor
    assert!(
        lock_dir.exists(),
        "lock directory should exist before doctor run"
    );

    // Run doctor with --auto-fix
    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--auto-fix"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should report orphaned locks were found and fixed
    assert!(
        combined.contains("orphaned") || combined.contains("lock"),
        "should mention orphaned locks. Output: {}",
        combined
    );

    // Lock directory should be removed after auto-fix
    assert!(
        !lock_dir.exists(),
        "orphaned lock directory should be removed after auto-fix"
    );

    Ok(())
}

#[test]
fn doctor_auto_fix_without_flag_reports_but_does_not_remove() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create an orphaned lock directory with a dead PID
    let lock_dir = dir.path().join(".ralph/lock/orphaned-test-lock-no-fix");
    std::fs::create_dir_all(&lock_dir)?;
    let owner_file = lock_dir.join("owner");
    std::fs::write(&owner_file, "pid:999998\nstarted:1234567890\n")?;

    // Run doctor WITHOUT --auto-fix
    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should warn about orphaned locks
    assert!(
        combined.contains("orphaned") || combined.contains("WARN"),
        "should warn about orphaned locks. Output: {}",
        combined
    );

    // Lock directory should STILL EXIST (no auto-fix)
    assert!(
        lock_dir.exists(),
        "lock directory should still exist without --auto-fix"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&lock_dir);

    Ok(())
}

#[test]
fn doctor_json_output_with_auto_fix() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create an orphaned lock directory with a dead PID
    let lock_dir = dir.path().join(".ralph/lock/orphaned-test-lock-json");
    std::fs::create_dir_all(&lock_dir)?;
    let owner_file = lock_dir.join("owner");
    std::fs::write(&owner_file, "pid:999997\nstarted:1234567890\n")?;

    // Run doctor with --format json --auto-fix
    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json", "--auto-fix"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON from stdout (log output goes to stderr)
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("JSON should be valid");

    // Verify fixes_applied is tracked
    let fixes_applied = json["summary"]["fixes_applied"].as_u64().unwrap_or(0);
    assert!(
        fixes_applied > 0,
        "should have fixes_applied > 0 when auto-fix removes locks"
    );

    // Find the lock check and verify fix_applied is set
    let checks = json["checks"].as_array().unwrap();
    let lock_check = checks
        .iter()
        .find(|c| c["category"] == "lock" && c["check"] == "orphaned_locks");

    if let Some(check) = lock_check {
        assert_eq!(
            check["fix_applied"], true,
            "fix_applied should be true for orphaned locks"
        );
    }

    Ok(())
}

#[test]
fn doctor_auto_fix_repairs_invalid_queue() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Create an invalid queue file (task with empty title - fails validation)
    let invalid_queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "title": "",
      "status": "todo",
      "priority": "medium",
      "tags": [],
      "scope": [],
      "depends_on": [],
      "evidence": [],
      "plan": [],
      "notes": [],
      "created_at": "2026-01-01T00:00:00Z",
      "updated_at": "2026-01-01T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(dir.path().join(".ralph/queue.json"), invalid_queue)?;

    // Run doctor without auto-fix - should report error
    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // Should fail with validation error
    assert!(
        !output.status.success(),
        "doctor should fail with invalid queue"
    );
    assert!(
        combined.contains("queue validation failed") || combined.contains("FAIL"),
        "should report queue validation failed. Output: {}",
        combined
    );

    // Run doctor with auto-fix - should repair
    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--auto-fix"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    // After auto-fix, doctor should pass
    assert!(
        output.status.success(),
        "doctor should pass after auto-fix. Output: {}",
        combined
    );
    assert!(
        combined.contains("queue valid")
            || combined.contains("repair")
            || combined.contains("FIXED"),
        "should report queue was repaired or is now valid. Output: {}",
        combined
    );

    Ok(())
}

#[test]
fn doctor_detects_missing_ralph_logs_gitignore() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph (which adds .ralph/logs/ to .gitignore)
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Overwrite .gitignore to intentionally omit .ralph/logs/
    std::fs::write(
        dir.path().join(".gitignore"),
        ".ralph/lock\n.ralph/cache/\n",
    )?;

    // Run doctor with JSON output
    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("JSON should be valid. Got stdout: {}", stdout));

    // Should fail because .ralph/logs/ is not gitignored
    assert_eq!(
        json["success"], false,
        "doctor should fail when .ralph/logs/ is not gitignored"
    );

    // Find the gitignore_ralph_logs check
    let checks = json["checks"].as_array().unwrap();
    let logs_check = checks
        .iter()
        .find(|c| c["category"] == "project" && c["check"] == "gitignore_ralph_logs");

    assert!(
        logs_check.is_some(),
        "should have a gitignore_ralph_logs check. Checks: {:?}",
        checks
    );
    let logs_check = logs_check.unwrap();

    assert_eq!(logs_check["severity"], "Error", "should be Error severity");
    assert_eq!(
        logs_check["fix_available"], true,
        "should have fix_available=true"
    );
    assert!(
        logs_check["suggested_fix"]
            .as_str()
            .unwrap_or("")
            .contains(".ralph/logs/"),
        "suggested_fix should mention .ralph/logs/"
    );

    Ok(())
}

#[test]
fn doctor_auto_fix_adds_ralph_logs_gitignore() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph (which adds .ralph/logs/ to .gitignore)
    ralph_cmd_in_dir(dir.path())
        .current_dir(dir.path())
        .args(["init", "--force", "--non-interactive"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Overwrite .gitignore to intentionally omit .ralph/logs/
    std::fs::write(
        dir.path().join(".gitignore"),
        ".ralph/lock\n.ralph/cache/\n",
    )?;

    // Run doctor with --auto-fix and JSON output
    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json", "--auto-fix"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("JSON should be valid. Got stdout: {}", stdout));

    // Find the gitignore_ralph_logs check
    let checks = json["checks"].as_array().unwrap();
    let logs_check = checks
        .iter()
        .find(|c| c["category"] == "project" && c["check"] == "gitignore_ralph_logs");

    assert!(
        logs_check.is_some(),
        "should have a gitignore_ralph_logs check"
    );
    let logs_check = logs_check.unwrap();

    // Verify fix_applied is true
    assert_eq!(
        logs_check["fix_applied"], true,
        "fix_applied should be true after auto-fix"
    );

    // Verify .gitignore now contains .ralph/logs/
    let gitignore_content = std::fs::read_to_string(dir.path().join(".gitignore"))?;
    assert!(
        gitignore_content.contains(".ralph/logs/"),
        ".gitignore should now contain .ralph/logs/. Content: {}",
        gitignore_content
    );

    Ok(())
}
