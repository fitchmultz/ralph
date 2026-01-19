use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

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

#[test]
fn doctor_passes_in_clean_env() -> Result<()> {
    let dir = TempDir::new()?;
    // Setup valid repo
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    Command::new(ralph_bin())
        .current_dir(dir.path())
        .args(["init", "--force"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{}", String::from_utf8_lossy(&output.stderr));
    }

    // Missing upstream is now a warning, not a failure, so doctor should pass
    assert!(output.status.success());
    assert!(stdout.contains("[OK] git binary found"));
    assert!(stdout.contains("[OK] queue valid"));
    assert!(stdout.contains("[WARN] no upstream configured"));
    Ok(())
}

#[test]
fn doctor_fails_when_queue_missing() -> Result<()> {
    let dir = TempDir::new()?;
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // No ralph init

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[FAIL] queue file missing"));
    Ok(())
}

#[test]
fn doctor_warns_on_missing_upstream() -> Result<()> {
    let dir = TempDir::new()?;
    // Setup valid repo without upstream
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    Command::new(ralph_bin())
        .current_dir(dir.path())
        .args(["init", "--force"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{}", String::from_utf8_lossy(&output.stderr));
    }

    // Should succeed with a warning about missing upstream
    assert!(output.status.success());
    assert!(stdout.contains("[WARN] no upstream configured"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_runner_binary() -> Result<()> {
    let dir = TempDir::new()?;
    // Setup valid repo
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    // Setup ralph
    Command::new(ralph_bin())
        .current_dir(dir.path())
        .args(["init", "--force"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Configure a non-existent runner binary
    let config_path = dir.path().join(".ralph/config.yaml");
    let config_content = r#"version: 1
agent:
  runner: opencode
  opencode_bin: "this-binary-does-not-exist-xyz123"
"#;
    std::fs::write(&config_path, config_content)?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail
    assert!(!output.status.success());
    // Should report the failure with the binary name
    let combined_output = format!("{}\n{}", stdout, stderr);
    assert!(combined_output.contains("this-binary-does-not-exist-xyz123"));
    assert!(combined_output.contains("[FAIL]"));
    Ok(())
}

#[test]
fn doctor_fails_with_nonexistent_gemini_binary() -> Result<()> {
    let dir = TempDir::new()?;
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    Command::new(ralph_bin())
        .current_dir(dir.path())
        .args(["init", "--force"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    let config_path = dir.path().join(".ralph/config.yaml");
    let config_content = r#"version: 1
agent:
  runner: gemini
  gemini_bin: "this-gemini-does-not-exist-xyz123"
"#;
    std::fs::write(&config_path, config_content)?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    let combined_output = format!("{}\n{}", stdout, stderr);
    assert!(combined_output.contains("this-gemini-does-not-exist-xyz123"));
    assert!(combined_output.contains("[FAIL]"));
    Ok(())
}

#[test]
fn doctor_fails_with_invalid_done_archive() -> Result<()> {
    let dir = TempDir::new()?;
    Command::new("git")
        .current_dir(dir.path())
        .arg("init")
        .status()?;

    Command::new(ralph_bin())
        .current_dir(dir.path())
        .args(["init", "--force"])
        .status()?;

    // Setup Makefile
    std::fs::write(dir.path().join("Makefile"), "ci:\n\tcargo test\n")?;

    // Corrupt done.yaml
    let done_path = dir.path().join(".ralph/done.yaml");
    std::fs::write(&done_path, "invalid yaml: { [")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("doctor")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success());
    assert!(
        stdout.contains("[FAIL] done archive validation failed")
            || stdout.contains("[FAIL] failed to load done archive")
    );
    Ok(())
}
