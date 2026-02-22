//! Integration test for README version marker error handling.
//!
//! Verifies that malformed version markers fail fast with actionable errors.

use anyhow::Result;
use ralph::commands::init::{
    InitOptions, ReadmeVersionError, check_readme_current_from_root, extract_readme_version,
    run_init,
};
use ralph::config::Resolved;
use ralph::contracts::Config;
use std::fs;
use tempfile::TempDir;

fn resolved_for(dir: &TempDir) -> Resolved {
    let repo_root = dir.path().to_path_buf();
    let queue_path = repo_root.join(".ralph/queue.jsonc");
    let done_path = repo_root.join(".ralph/done.jsonc");
    let project_config_path = Some(repo_root.join(".ralph/config.jsonc"));
    Resolved {
        config: Config::default(),
        repo_root,
        queue_path,
        done_path,
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path,
    }
}

#[test]
fn extract_readme_version_fails_on_non_numeric_version() {
    let content = "<!-- RALPH_README_VERSION: abc -->\n# Test";
    let result = extract_readme_version(content);
    assert!(matches!(result, Err(ReadmeVersionError::ParseError { value }) if value == "abc"));
}

#[test]
fn extract_readme_version_fails_on_missing_end_delimiter() {
    let content = "<!-- RALPH_README_VERSION: 5\n# Test";
    let result = extract_readme_version(content);
    assert!(matches!(result, Err(ReadmeVersionError::InvalidFormat)));
}

#[test]
fn check_readme_current_propagates_malformed_marker_error() -> Result<()> {
    let dir = TempDir::new()?;
    let repo_root = dir.path();

    // Create the .ralph directory and prompts that reference README
    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    let prompt_content = "This prompt references .ralph/README.md for context";
    fs::write(repo_root.join(".ralph/prompts/worker.md"), prompt_content)?;

    // Create a README with malformed version marker
    let malformed_readme = "<!-- RALPH_README_VERSION: not-a-number -->\n# Test README";
    fs::write(repo_root.join(".ralph/README.md"), malformed_readme)?;

    // Check that the error is propagated
    let result = check_readme_current_from_root(repo_root);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("malformed") || err_msg.contains("invalid"),
        "Error message should indicate malformed marker: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn check_readme_current_handles_legacy_no_marker() -> Result<()> {
    let dir = TempDir::new()?;
    let repo_root = dir.path();

    // Create the .ralph directory and prompts that reference README
    fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
    let prompt_content = "This prompt references .ralph/README.md for context";
    fs::write(repo_root.join(".ralph/prompts/worker.md"), prompt_content)?;

    // Create a README without any version marker (legacy file)
    let legacy_readme = "# Test README\nSome content";
    fs::write(repo_root.join(".ralph/README.md"), legacy_readme)?;

    // This should succeed and treat it as version 1
    let result = check_readme_current_from_root(repo_root)?;
    // Result will be Current(1) or Outdated depending on README_VERSION constant
    match result {
        ralph::commands::init::ReadmeCheckResult::Current(v) => {
            assert_eq!(v, 1);
        }
        ralph::commands::init::ReadmeCheckResult::Outdated {
            current_version, ..
        } => {
            assert_eq!(current_version, 1);
        }
        _ => panic!("Unexpected result for legacy README: {:?}", result),
    }

    Ok(())
}

#[test]
fn init_fails_on_malformed_readme_version() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for(&dir);

    // Set up existing files
    fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
    fs::create_dir_all(resolved.repo_root.join(".ralph/prompts"))?;
    fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
    fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
    fs::write(
        resolved.project_config_path.as_ref().unwrap(),
        r#"{"version":1}"#,
    )?;

    // Create prompt files that reference the README (so README check is triggered)
    let prompt_content = "This prompt references .ralph/README.md for context";
    fs::write(
        resolved.repo_root.join(".ralph/prompts/worker.md"),
        prompt_content,
    )?;

    // Create a README with malformed version marker
    let malformed_readme = "<!-- RALPH_README_VERSION: invalid-version -->\n# Test";
    fs::write(
        resolved.repo_root.join(".ralph/README.md"),
        malformed_readme,
    )?;

    // Init with update_readme=true should fail with an error about the malformed marker
    // because it triggers the version check path that validates the version marker
    let result = run_init(
        &resolved,
        InitOptions {
            force: false,
            force_lock: false,
            interactive: false,
            update_readme: true, // This triggers the version check
        },
    );

    assert!(
        result.is_err(),
        "Init should fail on malformed README version marker"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("malformed") || err_msg.contains("invalid") || err_msg.contains("version"),
        "Error should mention malformed version: {}",
        err_msg
    );

    Ok(())
}

#[test]
fn init_succeeds_on_legacy_readme_without_marker() -> Result<()> {
    let dir = TempDir::new()?;
    let resolved = resolved_for(&dir);

    // Set up existing files
    fs::create_dir_all(resolved.repo_root.join(".ralph"))?;
    fs::create_dir_all(resolved.repo_root.join(".ralph/prompts"))?;
    fs::write(&resolved.queue_path, r#"{"version":1,"tasks":[]}"#)?;
    fs::write(&resolved.done_path, r#"{"version":1,"tasks":[]}"#)?;
    fs::write(
        resolved.project_config_path.as_ref().unwrap(),
        r#"{"version":1}"#,
    )?;

    // Create prompt files that reference the README (so README check is triggered)
    let prompt_content = "This prompt references .ralph/README.md for context";
    fs::write(
        resolved.repo_root.join(".ralph/prompts/worker.md"),
        prompt_content,
    )?;

    // Create a legacy README without version marker
    let legacy_readme = "# Test README\nSome content";
    fs::write(resolved.repo_root.join(".ralph/README.md"), legacy_readme)?;

    // Init should succeed (treats as version 1)
    let result = run_init(
        &resolved,
        InitOptions {
            force: false,
            force_lock: false,
            interactive: false,
            update_readme: false,
        },
    );

    assert!(
        result.is_ok(),
        "Init should succeed on legacy README without marker"
    );
    Ok(())
}
