//! Processor-executor regression tests.
//!
//! Purpose:
//! - Processor-executor regression tests.
//!
//! Responsibilities:
//! - Verify hook filtering, deterministic chaining, and hook payload mutation.
//! - Verify processor failures surface actionable errors.
//!
//! Not handled here:
//! - Plugin discovery internals beyond the executor contract.
//! - Queue or runner integration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests use trusted temp repos with project-local plugins.
//! - Unix fixtures keep processor scripts executable.

use std::io::Write;
use std::path::Path;

use tempfile::TempDir;

use crate::contracts::{Config, Task, TaskPriority, TaskStatus};
use crate::plugins::manifest::{PluginManifest, ProcessorPlugin};
use crate::plugins::registry::PluginRegistry;

use super::ProcessorExecutor;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn trust_repo(repo_root: &Path) {
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).unwrap();
    std::fs::write(
        ralph_dir.join("trust.jsonc"),
        r#"{"allow_project_commands": true}"#,
    )
    .unwrap();
}

fn create_test_task(id: &str) -> Task {
    Task {
        id: id.to_string(),
        status: TaskStatus::Todo,
        title: "Test Task".to_string(),
        description: None,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

fn create_processor_plugin(
    dir: &Path,
    id: &str,
    hooks: Vec<&str>,
    script_content: &str,
) -> anyhow::Result<()> {
    let manifest = PluginManifest {
        api_version: crate::plugins::PLUGIN_API_VERSION,
        id: id.to_string(),
        version: "1.0.0".to_string(),
        name: format!("Plugin {id}"),
        description: None,
        runner: None,
        processors: Some(ProcessorPlugin {
            bin: "processor.sh".to_string(),
            hooks: hooks.iter().map(|hook| hook.to_string()).collect(),
        }),
    };

    std::fs::create_dir_all(dir)?;
    let manifest_path = dir.join("plugin.json");
    let mut file = std::fs::File::create(&manifest_path)?;
    file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

    let script_path = dir.join("processor.sh");
    let mut script_file = std::fs::File::create(&script_path)?;
    script_file.write_all(script_content.as_bytes())?;

    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    Ok(())
}

#[test]
fn test_no_enabled_processors_is_noop() {
    let tmp = TempDir::new().unwrap();
    let cfg = Config::default();
    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();

    let exec = ProcessorExecutor::new(tmp.path(), &registry);
    let task = create_test_task("RQ-0001");
    exec.validate_task(&task).unwrap();
}

#[test]
fn test_validate_task_invokes_processor() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());
    let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

    let script = r#"#!/bin/bash
HOOK="$1"
TASK_ID="$2"
FILE="$3"

if [ "$HOOK" = "validate_task" ]; then
    grep '"id": "RQ-0001"' "$FILE" > /dev/null || exit 1
fi
exit 0
"#;
    create_processor_plugin(&plugin_dir, "test.plugin", vec!["validate_task"], script).unwrap();

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    assert!(registry.discovered().contains_key("test.plugin"));
    assert!(registry.is_enabled("test.plugin"));

    let exec = ProcessorExecutor::new(tmp.path(), &registry);
    let task = create_test_task("RQ-0001");
    exec.validate_task(&task).unwrap();
}

#[test]
fn test_pre_prompt_mutates_prompt() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());
    let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

    let script = r#"#!/bin/bash
HOOK="$1"
TASK_ID="$2"
FILE="$3"

if [ "$HOOK" = "pre_prompt" ]; then
    echo " [PROCESSED BY test.plugin]" >> "$FILE"
fi
exit 0
"#;
    create_processor_plugin(&plugin_dir, "test.plugin", vec!["pre_prompt"], script).unwrap();
    let plugin_root = tmp.path().join(".ralph/plugins");
    let discovered = crate::plugins::discovery::discover_plugins(tmp.path()).unwrap();
    assert!(
        discovered.contains_key("test.plugin"),
        "plugin root exists={}, manifest exists={}, root entries={:?}, manifest={}",
        plugin_root.is_dir(),
        plugin_dir.join("plugin.json").is_file(),
        std::fs::read_dir(&plugin_root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        std::fs::read_to_string(plugin_dir.join("plugin.json")).unwrap()
    );

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    assert!(registry.discovered().contains_key("test.plugin"));
    assert!(registry.is_enabled("test.plugin"));
    let exec = ProcessorExecutor::new(tmp.path(), &registry);

    let original_prompt = "Original prompt";
    let final_prompt = exec.pre_prompt("RQ-0001", original_prompt).unwrap();

    assert!(final_prompt.contains("Original prompt"));
    assert!(final_prompt.contains("[PROCESSED BY test.plugin]"));
}

#[test]
fn test_multiple_processors_chain_in_order() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());

    let plugin_a_dir = tmp.path().join(".ralph/plugins/a.plugin");
    let plugin_b_dir = tmp.path().join(".ralph/plugins/b.plugin");

    let script_a = r#"#!/bin/bash
HOOK="$1"
FILE="$3"
if [ "$HOOK" = "pre_prompt" ]; then
    echo -n "A" >> "$FILE"
fi
exit 0
"#;
    let script_b = r#"#!/bin/bash
HOOK="$1"
FILE="$3"
if [ "$HOOK" = "pre_prompt" ]; then
    echo -n "B" >> "$FILE"
fi
exit 0
"#;

    create_processor_plugin(&plugin_a_dir, "a.plugin", vec!["pre_prompt"], script_a).unwrap();
    create_processor_plugin(&plugin_b_dir, "b.plugin", vec!["pre_prompt"], script_b).unwrap();

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "a.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );
    cfg.plugins.plugins.insert(
        "b.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    assert!(registry.discovered().contains_key("a.plugin"));
    assert!(registry.discovered().contains_key("b.plugin"));
    assert!(registry.is_enabled("a.plugin"));
    assert!(registry.is_enabled("b.plugin"));
    let exec = ProcessorExecutor::new(tmp.path(), &registry);

    let final_prompt = exec.pre_prompt("RQ-0001", "X").unwrap();
    assert_eq!(final_prompt, "XAB");
}

#[test]
fn test_hook_filtering_plugin_without_hook_not_invoked() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());
    let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

    let script = r#"#!/bin/bash
echo "CALLED" > /tmp/should_not_exist.txt
exit 0
"#;
    create_processor_plugin(&plugin_dir, "test.plugin", vec!["validate_task"], script).unwrap();

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    assert!(registry.discovered().contains_key("test.plugin"));
    assert!(registry.is_enabled("test.plugin"));
    let exec = ProcessorExecutor::new(tmp.path(), &registry);

    let _ = exec.pre_prompt("RQ-0001", "test").unwrap();
    assert!(!std::path::Path::new("/tmp/should_not_exist.txt").exists());
}

#[test]
fn test_non_zero_exit_surfaces_error() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());
    let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

    let script = r#"#!/bin/bash
echo "Validation failed!" >&2
exit 1
"#;
    create_processor_plugin(&plugin_dir, "test.plugin", vec!["validate_task"], script).unwrap();

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    assert!(registry.discovered().contains_key("test.plugin"));
    assert!(registry.is_enabled("test.plugin"));
    let exec = ProcessorExecutor::new(tmp.path(), &registry);

    let task = create_test_task("RQ-0001");
    let err = exec.validate_task(&task).unwrap_err();
    let err_str = err.to_string();
    assert!(err_str.contains("test.plugin"));
    assert!(err_str.contains("validate_task"));
    assert!(err_str.contains("exit_code=1"));
}

#[test]
fn test_processor_uses_manifest_bin() {
    let tmp = TempDir::new().unwrap();
    trust_repo(tmp.path());
    let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

    let script = r#"#!/bin/bash
echo "manifest" >> "$3"
exit 0
"#;
    create_processor_plugin(&plugin_dir, "test.plugin", vec!["pre_prompt"], script).unwrap();

    let mut cfg = Config::default();
    cfg.plugins.plugins.insert(
        "test.plugin".to_string(),
        crate::contracts::PluginConfig {
            enabled: Some(true),
            ..Default::default()
        },
    );

    let registry = PluginRegistry::load(tmp.path(), &cfg).unwrap();
    let exec = ProcessorExecutor::new(tmp.path(), &registry);

    let final_prompt = exec.pre_prompt("RQ-0001", "").unwrap();
    assert_eq!(final_prompt.trim(), "manifest");
}
