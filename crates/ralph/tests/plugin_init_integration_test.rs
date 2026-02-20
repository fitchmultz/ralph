//! Integration tests for `ralph plugin init` command.
//!
//! Tests cover:
//! - Scaffold default (project) + validate
//! - Dry-run writes nothing
//! - Invalid id rejected
//! - Target exists (force semantics)
//! - Global scope requires HOME

use anyhow::Result;
use std::path::Path;
use std::process::Command;

mod test_support;

fn git_init(dir: &Path) -> Result<()> {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args(["init", "--quiet"])
        .status()?;
    anyhow::ensure!(status.success(), "git init failed");

    // Create a minimal gitignore
    let gitignore_path = dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        ".ralph/lock\n.ralph/cache/\n.ralph/logs/\n",
    )?;

    Ok(())
}

fn ralph_init(dir: &Path) -> Result<()> {
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir)
        .env_remove("RUST_LOG")
        .args(["init", "--force", "--non-interactive"])
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "ralph init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn run_in_dir(dir: &Path, args: &[&str]) -> (std::process::ExitStatus, String, String) {
    let output = Command::new(test_support::ralph_bin())
        .current_dir(dir)
        .env_remove("RUST_LOG")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn plugin_init_scaffold_default_and_validate() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run plugin init
    let (status, stdout, stderr) =
        run_in_dir(temp_dir.path(), &["plugin", "init", "acme.test_plugin"]);
    assert!(status.success(), "plugin init failed: {}", stderr);
    assert!(
        stdout.contains("Created plugin acme.test_plugin"),
        "expected success message"
    );

    // Verify files exist
    let plugin_dir = temp_dir.path().join(".ralph/plugins/acme.test_plugin");
    assert!(plugin_dir.exists(), "plugin directory should exist");
    assert!(
        plugin_dir.join("plugin.json").exists(),
        "plugin.json should exist"
    );
    assert!(
        plugin_dir.join("runner.sh").exists(),
        "runner.sh should exist"
    );
    assert!(
        plugin_dir.join("processor.sh").exists(),
        "processor.sh should exist"
    );

    // Validate the plugin
    let (status, stdout, stderr) = run_in_dir(
        temp_dir.path(),
        &["plugin", "validate", "--id", "acme.test_plugin"],
    );
    assert!(status.success(), "plugin validate failed: {}", stderr);
    assert!(
        stdout.contains("validated successfully"),
        "expected validation success"
    );

    Ok(())
}

#[test]
fn plugin_init_dry_run_writes_nothing() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run plugin init with --dry-run
    let (status, stdout, stderr) = run_in_dir(
        temp_dir.path(),
        &["plugin", "init", "dry.plugin", "--dry-run"],
    );
    assert!(status.success(), "plugin init --dry-run failed: {}", stderr);
    assert!(stdout.contains("Would create"), "expected dry-run output");

    // Verify directory was NOT created
    let plugin_dir = temp_dir.path().join(".ralph/plugins/dry.plugin");
    assert!(
        !plugin_dir.exists(),
        "plugin directory should not exist in dry-run mode"
    );

    Ok(())
}

#[test]
fn plugin_init_rejects_invalid_id_with_path_separator() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Test forward slash
    let (status, _, stderr) = run_in_dir(temp_dir.path(), &["plugin", "init", "foo/bar"]);
    assert!(!status.success(), "should fail with forward slash in id");
    assert!(
        stderr.contains("path separators") || stderr.contains("path"),
        "expected path separator error, got: {}",
        stderr
    );

    // Test backslash
    let (status, _, stderr) = run_in_dir(temp_dir.path(), &["plugin", "init", "foo\\bar"]);
    assert!(!status.success(), "should fail with backslash in id");
    assert!(
        stderr.contains("path separators") || stderr.contains("path"),
        "expected path separator error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn plugin_init_target_exists_requires_force() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // First init should succeed
    let (status, _, stderr) = run_in_dir(temp_dir.path(), &["plugin", "init", "exists.test"]);
    assert!(status.success(), "first init failed: {}", stderr);

    // Second init without --force should fail
    let (status, _, stderr) = run_in_dir(temp_dir.path(), &["plugin", "init", "exists.test"]);
    assert!(!status.success(), "second init should fail without --force");
    assert!(
        stderr.contains("already exists") || stderr.contains("force"),
        "expected 'already exists' error, got: {}",
        stderr
    );

    // Third init with --force should succeed
    let (status, stdout, stderr) = run_in_dir(
        temp_dir.path(),
        &["plugin", "init", "exists.test", "--force"],
    );
    assert!(status.success(), "init with --force failed: {}", stderr);
    assert!(
        stdout.contains("Created plugin"),
        "expected success message"
    );

    Ok(())
}

#[test]
fn plugin_init_global_scope_requires_home() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run with HOME removed from environment
    let output = Command::new(test_support::ralph_bin())
        .current_dir(temp_dir.path())
        .env_remove("HOME")
        .env_remove("RUST_LOG")
        .args(["plugin", "init", "x.y", "--scope", "global"])
        .output()?;

    assert!(!output.status.success(), "should fail without HOME");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("HOME") || stderr.to_lowercase().contains("home environment variable"),
        "expected HOME error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn plugin_init_with_runner_only() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run with --with-runner only
    let (status, _stdout, stderr) = run_in_dir(
        temp_dir.path(),
        &["plugin", "init", "runner.only", "--with-runner"],
    );
    assert!(status.success(), "plugin init failed: {}", stderr);

    // Verify runner.sh exists but processor.sh does not
    let plugin_dir = temp_dir.path().join(".ralph/plugins/runner.only");
    assert!(plugin_dir.join("plugin.json").exists());
    assert!(plugin_dir.join("runner.sh").exists());
    assert!(!plugin_dir.join("processor.sh").exists());

    // Verify manifest has runner section only
    let manifest_content = std::fs::read_to_string(plugin_dir.join("plugin.json"))?;
    assert!(
        manifest_content.contains("runner"),
        "manifest should have runner section"
    );
    assert!(
        !manifest_content.contains("processors"),
        "manifest should not have processors section"
    );

    Ok(())
}

#[test]
fn plugin_init_with_processor_only() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run with --with-processor only
    let (status, _stdout, stderr) = run_in_dir(
        temp_dir.path(),
        &["plugin", "init", "processor.only", "--with-processor"],
    );
    assert!(status.success(), "plugin init failed: {}", stderr);

    // Verify processor.sh exists but runner.sh does not
    let plugin_dir = temp_dir.path().join(".ralph/plugins/processor.only");
    assert!(plugin_dir.join("plugin.json").exists());
    assert!(!plugin_dir.join("runner.sh").exists());
    assert!(plugin_dir.join("processor.sh").exists());

    // Verify manifest has processors section only
    let manifest_content = std::fs::read_to_string(plugin_dir.join("plugin.json"))?;
    assert!(
        manifest_content.contains("processors"),
        "manifest should have processors section"
    );
    assert!(
        !manifest_content.contains("runner"),
        "manifest should not have runner section"
    );

    Ok(())
}

#[test]
fn plugin_init_custom_path() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Create a custom directory
    let custom_dir = temp_dir.path().join("custom_plugins");
    std::fs::create_dir_all(&custom_dir)?;

    // Run with --path
    let (status, _, stderr) = run_in_dir(
        temp_dir.path(),
        &[
            "plugin",
            "init",
            "custom.test",
            "--path",
            custom_dir.join("my-plugin").to_str().unwrap(),
        ],
    );
    assert!(
        status.success(),
        "plugin init with --path failed: {}",
        stderr
    );

    // Verify plugin was created in custom path
    let plugin_dir = custom_dir.join("my-plugin");
    assert!(plugin_dir.exists(), "plugin should be in custom path");
    assert!(plugin_dir.join("plugin.json").exists());

    Ok(())
}

#[test]
fn plugin_init_with_custom_metadata() -> Result<()> {
    let temp_dir = test_support::temp_dir_outside_repo();
    git_init(temp_dir.path())?;
    ralph_init(temp_dir.path())?;

    // Run with custom metadata
    let (status, _, stderr) = run_in_dir(
        temp_dir.path(),
        &[
            "plugin",
            "init",
            "meta.test",
            "--name",
            "My Custom Plugin",
            "--version",
            "2.0.0",
            "--description",
            "A test plugin description",
        ],
    );
    assert!(
        status.success(),
        "plugin init with metadata failed: {}",
        stderr
    );

    // Verify manifest content
    let manifest_path = temp_dir.path().join(".ralph/plugins/meta.test/plugin.json");
    let manifest_content = std::fs::read_to_string(&manifest_path)?;

    assert!(
        manifest_content.contains("My Custom Plugin"),
        "manifest should contain custom name"
    );
    assert!(
        manifest_content.contains("2.0.0"),
        "manifest should contain custom version"
    );
    assert!(
        manifest_content.contains("A test plugin description"),
        "manifest should contain custom description"
    );

    Ok(())
}
