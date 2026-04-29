//! Queue and workspace contract coverage for `ralph machine`.
//!
//! Purpose:
//! - Verify machine queue and workspace JSON documents exposed to app clients.
//!
//! Responsibilities:
//! - Assert queue read success and failure document shapes.
//! - Assert workspace overview bundles queue and config payloads together.
//! - Keep queue/workspace contract regressions isolated from task and recovery flows.
//!
//! Non-scope:
//! - Task mutation behavior.
//! - Parallel runtime or system contract coverage.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Tests execute against disposable repos initialized through the public CLI.
//! - Contract assertions preserve the historical flat suite behavior exactly.

use super::machine_contract_test_support::{run_in_dir, setup_ralph_repo, trust_project_commands};
use anyhow::{Context, Result};
use serde_json::Value;

const SENSITIVE_PROJECT_CONFIG: &str = r#"{
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "codex_bin": "codex"
  }
}"#;

#[test]
fn machine_queue_read_returns_versioned_snapshot() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        status.success(),
        "machine queue read failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 1);
    assert!(document["paths"]["queue_path"].is_string());
    assert!(document["active"]["tasks"].is_array());
    assert!(document["done"]["tasks"].is_array());
    Ok(())
}

#[test]
fn machine_queue_read_suppresses_invalid_dotenv_warning() -> Result<()> {
    let dir = setup_ralph_repo()?;
    std::fs::write(
        dir.path().join(".env"),
        "INVALID LINE WITHOUT EQUALS SIGN\nVALID_KEY=valid\n",
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        status.success(),
        "machine queue read failed with malformed .env\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.trim().is_empty(),
        "machine command must not emit prose stderr for malformed .env; stderr was:\n{stderr}"
    );
    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 1);
    Ok(())
}

#[test]
fn machine_queue_read_failure_returns_structured_error_document() -> Result<()> {
    let dir = tempfile::tempdir()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        !status.success(),
        "machine queue read should fail outside a Ralph repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "failure stdout should stay empty: {stdout}"
    );

    let document: Value = serde_json::from_str(&stderr)?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["code"], "queue_corrupted");
    assert_eq!(document["message"], "No Ralph queue file found.");
    assert_eq!(document["retryable"], false);
    assert!(
        document["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("queue.jsonc")
    );
    Ok(())
}

#[test]
fn machine_queue_read_gates_runnability_when_queue_validation_fails() -> Result<()> {
    let dir = setup_ralph_repo()?;
    let queue_path = dir.path().join(".ralph/queue.jsonc");
    std::fs::write(
        &queue_path,
        r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Missing created_at should stall queue read",
      "priority": "medium",
      "updated_at": "2026-04-01T00:00:00Z"
    }
  ]
}
"#,
    )?;

    let (validate_status, validate_stdout, validate_stderr) =
        run_in_dir(dir.path(), &["queue", "validate"]);
    assert!(
        !validate_status.success(),
        "queue validate should reject missing created_at\nstdout:\n{validate_stdout}\nstderr:\n{validate_stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "queue", "read"]);
    assert!(
        status.success(),
        "machine queue read should emit a validation-gated snapshot\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert!(
        document.get("next_runnable_task_id").is_none()
            || document["next_runnable_task_id"].is_null(),
        "invalid queue must not advertise a next runnable task: {stdout}"
    );
    assert!(document["runnability"]["selection"]["selected_task_id"].is_null());
    assert!(document["runnability"]["selection"]["selected_task_status"].is_null());
    assert_eq!(document["runnability"]["summary"]["runnable_candidates"], 0);
    let blocking = &document["runnability"]["summary"]["blocking"];
    assert_eq!(blocking["status"], "stalled");
    assert_eq!(blocking["reason"]["kind"], "operator_recovery");
    assert_eq!(blocking["reason"]["scope"], "queue_validate");
    assert_eq!(blocking["reason"]["reason"], "validation_failed");
    assert!(
        blocking["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("Missing created_at"),
        "blocking detail should include validation failure: {stdout}"
    );
    Ok(())
}

#[test]
fn machine_workspace_overview_returns_queue_and_config_in_one_document() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "workspace", "overview"]);
    assert!(
        status.success(),
        "machine workspace overview failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["queue"]["version"], 1);
    assert_eq!(document["config"]["version"], 5);
    assert!(document["queue"]["paths"]["queue_path"].is_string());
    assert!(document["queue"]["active"]["tasks"].is_array());
    assert!(document["config"]["paths"]["project_config_path"].is_string());
    assert!(document["config"]["config"].is_object());
    assert!(document["config"]["execution_controls"]["runners"].is_array());
    assert_eq!(
        document["config"]["execution_controls"]["parallel_workers"]["max"],
        255
    );
    Ok(())
}

#[test]
fn machine_config_resolve_succeeds_without_queue_file_and_omits_resume_preview() -> Result<()> {
    let dir = setup_ralph_repo()?;
    let queue_path = dir.path().join(".ralph/queue.jsonc");
    std::fs::remove_file(&queue_path)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);
    assert!(
        status.success(),
        "machine config resolve should succeed without a queue file\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.trim().is_empty(),
        "machine config resolve should not emit stderr on success: {stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 5);
    assert!(document["paths"]["queue_path"].is_string());
    assert!(document["config"].is_object());
    assert!(document["execution_controls"]["runners"].is_array());
    assert!(
        document.get("resume_preview").is_none() || document["resume_preview"].is_null(),
        "resume_preview should be omitted or null when queue file is unavailable: {stdout}"
    );
    assert!(
        !queue_path.exists(),
        "machine config resolve must not recreate missing queue files"
    );
    Ok(())
}

#[test]
fn machine_config_resolve_docs_example_matches_execution_controls_contract() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);
    assert!(
        status.success(),
        "machine config resolve failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let live: Value = serde_json::from_str(&stdout)?;
    let docs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("docs/features/session-management.md"),
    )?;
    let example = session_management_config_preview_example(&docs)
        .context("session-management config preview JSON example")?;

    assert_eq!(
        example["execution_controls"]["reasoning_efforts"],
        live["execution_controls"]["reasoning_efforts"],
        "session-management config preview reasoning efforts must match live machine config resolve output"
    );
    assert_eq!(
        example["execution_controls"]["parallel_workers"],
        live["execution_controls"]["parallel_workers"],
        "session-management config preview parallel worker bounds must match live machine config resolve output"
    );

    Ok(())
}

fn session_management_config_preview_example(docs: &str) -> Result<Value> {
    for block in docs.split("```json").skip(1) {
        let Some((json, _after)) = block.split_once("```") else {
            continue;
        };
        if json.contains("\"execution_controls\"") && json.contains("\"resume_preview\"") {
            return serde_json::from_str(json)
                .context("parse session-management config preview JSON");
        }
    }
    anyhow::bail!("session-management config preview JSON block not found")
}

#[test]
fn machine_config_resolve_reports_plugin_registry_load_failures_as_diagnostics() -> Result<()> {
    let dir = setup_ralph_repo()?;
    trust_project_commands(dir.path())?;
    let plugin_dir = dir.path().join(".ralph/plugins/broken.runner");
    std::fs::create_dir_all(&plugin_dir)?;
    std::fs::write(plugin_dir.join("plugin.json"), "{not valid json")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);
    assert!(
        status.success(),
        "machine config resolve should degrade successfully for malformed plugins\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.trim().is_empty(),
        "successful plugin-registry degradation should be stdout-structured, not stderr text: {stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(document["version"], 5);
    assert!(
        document["execution_controls"]["runners"]
            .as_array()
            .is_some_and(|runners| runners.iter().any(|runner| runner["id"] == "codex"))
    );
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["severity"],
        "warning"
    );
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["code"],
        "plugin_registry_load_failed"
    );
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["fallback"],
        "built_in_runners_only"
    );
    assert!(
        document["execution_controls"]["diagnostics"][0]
            .get("plugin_id")
            .is_none(),
        "whole-registry failures should not name one plugin id: {stdout}"
    );
    assert!(
        document["execution_controls"]["diagnostics"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("broken.runner"),
        "diagnostic detail should explain the malformed manifest path: {stdout}"
    );
    Ok(())
}

#[test]
fn machine_config_resolve_reports_plugin_runner_id_conflicts_as_diagnostics() -> Result<()> {
    let dir = setup_ralph_repo()?;
    trust_project_commands(dir.path())?;
    let plugin_dir = dir.path().join(".ralph/plugins/codex-shadow.runner");
    std::fs::create_dir_all(&plugin_dir)?;
    std::fs::write(
        plugin_dir.join("plugin.json"),
        r#"{
  "api_version": 1,
  "id": "CODEX",
  "version": "1.0.0",
  "name": "Codex Shadow Plugin",
  "runner": {
    "bin": "runner.sh"
  }
}"#,
    )?;
    std::fs::write(
        dir.path().join(".ralph/config.jsonc"),
        r#"{
  "version": 2,
  "plugins": {
    "plugins": {
      "CODEX": {
        "enabled": true
      }
    }
  }
}"#,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);
    assert!(
        status.success(),
        "machine config resolve should skip conflicting plugin runners without failing\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.trim().is_empty(),
        "successful plugin-runner conflict degradation should be stdout-structured, not stderr text: {stderr}"
    );

    let document: Value = serde_json::from_str(&stdout)?;
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["code"],
        "plugin_runner_id_conflict"
    );
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["plugin_id"],
        "CODEX"
    );
    assert_eq!(
        document["execution_controls"]["diagnostics"][0]["fallback"],
        "skipped_plugin_runner"
    );
    assert!(
        document["execution_controls"]["runners"]
            .as_array()
            .is_some_and(|runners| runners
                .iter()
                .filter(|runner| runner["id"]
                    .as_str()
                    .is_some_and(|id| id.eq_ignore_ascii_case("codex")))
                .count()
                == 1)
    );
    Ok(())
}

#[test]
fn machine_config_resolve_reports_untrusted_execution_settings_as_config_error() -> Result<()> {
    let dir = setup_ralph_repo()?;
    std::fs::remove_file(dir.path().join(".ralph/trust.jsonc"))?;
    std::fs::write(
        dir.path().join(".ralph/config.jsonc"),
        SENSITIVE_PROJECT_CONFIG,
    )?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);
    assert!(
        !status.success(),
        "machine config resolve should fail for untrusted execution-sensitive project config\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "failure stdout should stay empty: {stdout}"
    );

    let document: Value = serde_json::from_str(&stderr)?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["code"], "config_incompatible");
    assert_eq!(
        document["message"],
        "Project config defines execution-sensitive settings, but this repo is not trusted."
    );
    assert_eq!(document["retryable"], false);
    let detail = document["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("repo is not trusted")
            && detail.contains("ralph init")
            && detail.contains("ralph config trust init"),
        "detail should preserve trust remediation: {stderr}"
    );
    Ok(())
}

#[test]
fn machine_workspace_overview_still_fails_without_queue_file() -> Result<()> {
    let dir = setup_ralph_repo()?;
    std::fs::remove_file(dir.path().join(".ralph/queue.jsonc"))?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "workspace", "overview"]);
    assert!(
        !status.success(),
        "machine workspace overview should still fail without a queue file\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "failure stdout should stay empty: {stdout}"
    );

    let document: Value = serde_json::from_str(&stderr)?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["code"], "queue_corrupted");
    assert_eq!(document["message"], "No Ralph queue file found.");
    Ok(())
}

#[cfg(unix)]
#[test]
fn machine_config_resolve_fails_when_queue_path_metadata_is_inaccessible() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let dir = setup_ralph_repo()?;
    let restricted_dir = dir.path().join("restricted");
    let queue_path = restricted_dir.join("queue.jsonc");
    let config_path = dir.path().join(".ralph/config.jsonc");
    let config_contents = format!(
        "{{\n  \"queue\": {{\n    \"file\": {}\n  }}\n}}\n",
        serde_json::to_string(&queue_path.display().to_string())?
    );

    std::fs::create_dir(&restricted_dir)?;
    std::fs::write(&config_path, config_contents)?;
    std::fs::set_permissions(&restricted_dir, std::fs::Permissions::from_mode(0o000))?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "config", "resolve"]);

    std::fs::set_permissions(&restricted_dir, std::fs::Permissions::from_mode(0o755))?;

    assert!(
        !status.success(),
        "machine config resolve should fail when queue-path metadata cannot be inspected\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "failure stdout should stay empty: {stdout}"
    );

    let document: Value = serde_json::from_str(&stderr)?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["code"], "permission_denied");
    assert_eq!(document["message"], "Permission denied.");
    assert!(
        document["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("inspect queue file"),
        "structured detail should explain the failed queue-path inspection: {stderr}"
    );
    Ok(())
}
