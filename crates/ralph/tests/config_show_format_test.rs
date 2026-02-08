//! Integration tests for `ralph config show --format` flag.
//!
//! Responsibilities:
//! - Test that `--format json` and `--format yaml` produce valid parseable output.
//! - Test that both formats contain the same top-level keys for parity.
//! - Test that invalid format values fail with a clear error.
//!
//! Not handled here:
//! - Config resolution logic (see config_test.rs).
//! - Config file loading/saving (see config.rs tests).
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp directories to avoid user global config interference.
//! - The `RALPH_REPO_ROOT_OVERRIDE` env var is set to control repo root detection.

use serde_json::Value as JsonValue;
use std::collections::BTreeSet;

mod test_support;

/// Top-level keys expected in the resolved config output.
const EXPECTED_TOP_LEVEL_KEYS: &[&str] = &[
    "version",
    "project_type",
    "queue",
    "agent",
    "parallel",
    "plugins",
];

/// Setup an isolated ralph repo in a temp directory.
/// Returns the temp directory.
fn setup_isolated_repo() -> tempfile::TempDir {
    let _lock = test_support::env_lock().lock();
    let dir = test_support::temp_dir_outside_repo();

    // Initialize git repo
    test_support::git_init(dir.path()).expect("git init should succeed");

    // Initialize ralph (non-interactive)
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    assert!(
        status.success(),
        "ralph init should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    dir
}

#[test]
fn config_show_default_outputs_yaml() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    let output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show"])
        .output()
        .expect("failed to execute ralph config show");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "ralph config show should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Parse as YAML
    let yaml_v: JsonValue =
        serde_yaml::from_str(&stdout).expect("default output should be valid YAML");

    // Check top-level keys
    let obj = yaml_v
        .as_object()
        .expect("config should be a JSON/YAML object");
    for key in EXPECTED_TOP_LEVEL_KEYS {
        assert!(obj.contains_key(*key), "missing top-level key: {key}");
    }
}

#[test]
fn config_show_format_json_outputs_valid_json() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    let output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "json"])
        .output()
        .expect("failed to execute ralph config show --format json");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "ralph config show --format json should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Parse as JSON
    let json_v: JsonValue =
        serde_json::from_str(&stdout).expect("--format json output should be valid JSON");

    // Check top-level keys
    let obj = json_v.as_object().expect("config should be a JSON object");
    for key in EXPECTED_TOP_LEVEL_KEYS {
        assert!(obj.contains_key(*key), "missing top-level key: {key}");
    }
}

#[test]
fn config_show_format_yaml_outputs_valid_yaml() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    let output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "yaml"])
        .output()
        .expect("failed to execute ralph config show --format yaml");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "ralph config show --format yaml should succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Parse as YAML
    let yaml_v: JsonValue =
        serde_yaml::from_str(&stdout).expect("--format yaml output should be valid YAML");

    // Check top-level keys
    let obj = yaml_v.as_object().expect("config should be a YAML object");
    for key in EXPECTED_TOP_LEVEL_KEYS {
        assert!(obj.contains_key(*key), "missing top-level key: {key}");
    }
}

#[test]
fn config_show_yaml_and_json_have_same_top_level_keys() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    // Get YAML output
    let yaml_output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "yaml"])
        .output()
        .expect("failed to execute ralph config show --format yaml");

    assert!(
        yaml_output.status.success(),
        "ralph config show --format yaml should succeed"
    );

    let yaml_stdout = String::from_utf8_lossy(&yaml_output.stdout);
    let yaml_v: JsonValue = serde_yaml::from_str(&yaml_stdout).expect("yaml output should parse");

    // Get JSON output
    let json_output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "json"])
        .output()
        .expect("failed to execute ralph config show --format json");

    assert!(
        json_output.status.success(),
        "ralph config show --format json should succeed"
    );

    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    let json_v: JsonValue = serde_json::from_str(&json_stdout).expect("json output should parse");

    // Compare top-level keys
    let yaml_keys: BTreeSet<_> = yaml_v
        .as_object()
        .expect("yaml should be an object")
        .keys()
        .cloned()
        .collect();
    let json_keys: BTreeSet<_> = json_v
        .as_object()
        .expect("json should be an object")
        .keys()
        .cloned()
        .collect();

    assert_eq!(
        yaml_keys, json_keys,
        "YAML and JSON outputs should have the same top-level keys\nYAML keys: {yaml_keys:?}\nJSON keys: {json_keys:?}"
    );
}

#[test]
fn config_show_invalid_format_fails_with_error() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    let output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "not-a-format"])
        .output()
        .expect("failed to execute ralph config show --format not-a-format");

    // Should fail (non-zero exit code)
    assert!(
        !output.status.success(),
        "ralph config show --format not-a-format should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}", stderr);

    // Should contain helpful error message
    assert!(
        combined.contains("possible values") || combined.contains("invalid value"),
        "error message should mention possible values or invalid value: {combined}"
    );

    // Should mention valid options
    assert!(
        combined.contains("yaml") || combined.contains("json"),
        "error message should mention valid format options: {combined}"
    );
}

#[test]
fn config_show_text_alias_works_for_yaml() {
    let dir = setup_isolated_repo();
    let xdg_config_home = dir.path().join(".xdg_config");
    std::fs::create_dir_all(&xdg_config_home).expect("create xdg config dir");

    // Test "text" alias for yaml (for backward compatibility)
    let output = std::process::Command::new(test_support::ralph_bin())
        .current_dir(&dir)
        .env_remove("RUST_LOG")
        .env("RALPH_REPO_ROOT_OVERRIDE", dir.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .args(["config", "show", "--format", "text"])
        .output()
        .expect("failed to execute ralph config show --format text");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "ralph config show --format text should succeed (alias for yaml)\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Should parse as YAML
    let _: JsonValue =
        serde_yaml::from_str(&stdout).expect("--format text output should be valid YAML");
}
