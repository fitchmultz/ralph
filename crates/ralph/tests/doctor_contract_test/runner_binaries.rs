//! Runner binary contract tests for doctor.
//!
//! Purpose:
//! - Runner binary contract tests for doctor.
//!
//! Responsibilities:
//! - Verify doctor reports missing or invalid runner binaries clearly.
//! - Exercise runner probe fallbacks for alternate version/help flags.
//! - Ensure diagnostics mention the relevant config keys and file locations.
//!
//! Not handled here:
//! - Doctor JSON formatting or repo-hygiene checks.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Repo-config runner overrides use trusted seeded fixtures.
//! - Global-config runner overrides use explicit HOME overrides instead of PATH mutation.
//! - Fake runners are created through shared executable helpers rather than ad hoc chmod logic.

use super::*;

#[test]
fn doctor_blocks_untrusted_project_runner_override() -> Result<()> {
    let dir = setup_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"runner":"opencode","opencode_bin":"/tmp/fake-opencode"}}"#,
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(!output.status.success());
    assert!(combined.contains("execution-sensitive runner override"));
    assert!(combined.contains("repo is not trusted"));
    assert!(combined.contains(".ralph/trust.jsonc"));
    Ok(())
}

#[test]
fn doctor_allows_trusted_project_runner_override_to_probe_normally() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;
    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "trusted-opencode",
        r#"#!/bin/bash
case "$1" in
  --version|--help) echo "trusted-opencode 1.0.0"; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    )?;

    write_repo_config(
        dir.path(),
        &format!(
            r#"{{"version":2,"agent":{{"runner":"opencode","opencode_bin":"{}"}}}}"#,
            runner_path.display()
        ),
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(output.status.success(), "doctor output:\n{combined}");
    assert!(combined.contains("runner binary") && combined.contains("found"));
    assert!(!combined.contains("project_runner_override_untrusted"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_runner_binary() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"runner":"opencode","opencode_bin":"this-binary-does-not-exist-xyz123"}}"#,
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined_output = format!("{}\n{}", stdout, stderr);

    assert!(!output.status.success());
    assert!(combined_output.contains("this-binary-does-not-exist-xyz123"));
    assert!(combined_output.contains("FAIL"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_gemini_binary() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"runner":"gemini","gemini_bin":"this-gemini-does-not-exist-xyz123"}}"#,
    )?;

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
    let dir = setup_trusted_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"runner":"claude","claude_bin":"this-claude-does-not-exist-xyz123"}}"#,
    )?;

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
    let dir = setup_trusted_doctor_repo()?;

    let done_path = dir.path().join(".ralph/done.jsonc");
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
    let dir = setup_trusted_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"instruction_files":["missing-global-agents.md"]}}"#,
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(output.status.success());
    assert!(combined.contains("WARN") && combined.contains("instruction_files"));
    Ok(())
}

#[test]
fn doctor_reports_agents_md_success_when_configured() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;
    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "agents-success-runner",
        r#"#!/bin/bash
case "$1" in
  --version|--help) echo "agents-success-runner 1.0.0"; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    )?;
    std::fs::write(dir.path().join("AGENTS.md"), "# Repo instructions\n")?;
    write_repo_config(
        dir.path(),
        &format!(
            r#"{{"version":2,"agent":{{"runner":"opencode","opencode_bin":"{}","instruction_files":["AGENTS.md"]}}}}"#,
            runner_path.display()
        ),
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(output.status.success(), "doctor output:\n{combined}");
    assert!(combined.contains("AGENTS.md configured and readable"));
    Ok(())
}

#[test]
fn doctor_warns_when_repo_agents_md_exists_but_is_not_configured() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;
    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "agents-warning-runner",
        r#"#!/bin/bash
case "$1" in
  --version|--help) echo "agents-warning-runner 1.0.0"; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    )?;
    std::fs::write(dir.path().join("AGENTS.md"), "# Repo instructions\n")?;
    write_repo_config(
        dir.path(),
        &format!(
            r#"{{"version":2,"agent":{{"runner":"opencode","opencode_bin":"{}","instruction_files":["docs/other.md"]}}}}"#,
            runner_path.display()
        ),
    )?;
    std::fs::create_dir_all(dir.path().join("docs"))?;
    std::fs::write(dir.path().join("docs/other.md"), "# Other instructions\n")?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(output.status.success(), "doctor output:\n{combined}");
    assert!(combined.contains("WARN"));
    assert!(combined.contains("exists at repo root but is not configured"));
    Ok(())
}

#[test]
fn doctor_passes_with_runner_that_only_supports_help() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;

    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "test-runner-help-only",
        r#"#!/bin/bash
case "$1" in
  --help) echo "Usage: test-runner [options]"; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    )?;

    let home_dir = dir.path().join("home");
    write_global_config(
        &home_dir,
        &format!(
            r#"{{"version":2,"agent":{{"runner":"opencode","opencode_bin":"{}"}}}}"#,
            runner_path.display()
        ),
    )?;

    let output = ralph_cmd_in_dir(dir.path())
        .env("HOME", &home_dir)
        .env_remove("XDG_CONFIG_HOME")
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

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
    let dir = setup_trusted_doctor_repo()?;

    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "test-runner-v-only",
        r#"#!/bin/bash
case "$1" in
  -V) echo "test-runner 1.0.0"; exit 0 ;;
  --version) exit 1 ;;
  --help) exit 1 ;;
  *) exit 1 ;;
esac
"#,
    )?;

    let home_dir = dir.path().join("home");
    write_global_config(
        &home_dir,
        &format!(
            r#"{{"version":2,"agent":{{"runner":"claude","claude_bin":"{}"}}}}"#,
            runner_path.display()
        ),
    )?;

    let output = ralph_cmd_in_dir(dir.path())
        .env("HOME", &home_dir)
        .env_remove("XDG_CONFIG_HOME")
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

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
    let dir = setup_trusted_doctor_repo()?;

    let runner_path = test_support::create_fake_runner(
        dir.path(),
        "test-runner-no-flags",
        "#!/bin/bash\nexit 1\n",
    )?;

    write_repo_config(
        dir.path(),
        &format!(
            r#"{{"version":2,"agent":{{"runner":"gemini","gemini_bin":"{}"}}}}"#,
            runner_path.display()
        ),
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

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
    let dir = setup_doctor_repo()?;

    write_repo_config(
        dir.path(),
        r#"{"version":2,"agent":{"runner":"codex","codex_bin":"/nonexistent/path/codex"}}"#,
    )?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    assert!(!output.status.success());
    assert!(
        combined.contains("codex_bin"),
        "error should mention codex_bin config key"
    );
    assert!(
        combined.contains(".ralph/config.jsonc"),
        "error should mention config file location"
    );
    Ok(())
}
