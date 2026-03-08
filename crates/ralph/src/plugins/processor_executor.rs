//! Processor plugin hook execution.
//!
//! Responsibilities:
//! - Invoke enabled processor plugins for supported hooks (validate_task, pre_prompt, post_run).
//! - Enforce deterministic chaining order (ascending plugin id).
//! - Marshal hook payloads via temp files following the processor.sh protocol.
//!
//! Not handled here:
//! - Plugin discovery/enable policy (handled by PluginRegistry).
//! - Runner execution / CI gate / queue mutation.
//!
//! Invariants/assumptions:
//! - Plugins are trusted (not sandboxed). Non-zero exit is treated as failure.
//! - Hook payload files are UTF-8 text (task JSON, prompt text, stdout NDJSON).

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::contracts::Task;
use crate::plugins::registry::PluginRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessorHook {
    ValidateTask,
    PrePrompt,
    PostRun,
}

impl ProcessorHook {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ValidateTask => "validate_task",
            Self::PrePrompt => "pre_prompt",
            Self::PostRun => "post_run",
        }
    }
}

pub(crate) struct ProcessorExecutor<'a> {
    repo_root: &'a Path,
    registry: &'a PluginRegistry,
}

impl<'a> ProcessorExecutor<'a> {
    pub(crate) fn new(repo_root: &'a Path, registry: &'a PluginRegistry) -> Self {
        Self {
            repo_root,
            registry,
        }
    }

    /// Invoke validate_task hooks for all enabled processor plugins.
    /// Non-zero exit from any plugin aborts with an actionable error.
    pub(crate) fn validate_task(&self, task: &Task) -> Result<()> {
        let chain = self.build_processor_chain(ProcessorHook::ValidateTask);
        if chain.is_empty() {
            return Ok(());
        }

        // Write task JSON to temp file
        let task_json =
            serde_json::to_string_pretty(task).context("serialize task for validate_task hook")?;
        let mut temp_file = crate::fsutil::create_ralph_temp_file("plugin")
            .context("create temp file for validate_task")?;
        temp_file
            .write_all(task_json.as_bytes())
            .context("write task JSON to temp file")?;
        let temp_path = temp_file.into_temp_path();

        for (plugin_id, _discovered) in chain {
            self.invoke_hook(plugin_id, ProcessorHook::ValidateTask, &task.id, &temp_path)?;
        }

        Ok(())
    }

    /// Invoke pre_prompt hooks for all enabled processor plugins.
    /// Each plugin can modify the prompt file in place.
    /// Returns the final (possibly modified) prompt text.
    pub(crate) fn pre_prompt(&self, task_id: &str, prompt: &str) -> Result<String> {
        let chain = self.build_processor_chain(ProcessorHook::PrePrompt);
        if chain.is_empty() {
            return Ok(prompt.to_string());
        }

        // Write prompt to temp file
        let mut temp_file = crate::fsutil::create_ralph_temp_file("plugin")
            .context("create temp file for pre_prompt")?;
        temp_file
            .write_all(prompt.as_bytes())
            .context("write prompt to temp file")?;
        let temp_path = temp_file.into_temp_path();

        for (plugin_id, _discovered) in chain {
            self.invoke_hook(plugin_id, ProcessorHook::PrePrompt, task_id, &temp_path)?;
        }

        // Read back the (possibly modified) prompt
        let final_prompt =
            std::fs::read_to_string(&temp_path).context("read modified prompt from temp file")?;

        Ok(final_prompt)
    }

    /// Invoke post_run hooks for all enabled processor plugins.
    /// Non-zero exit from any plugin aborts with an actionable error.
    pub(crate) fn post_run(&self, task_id: &str, stdout: &str) -> Result<()> {
        let chain = self.build_processor_chain(ProcessorHook::PostRun);
        if chain.is_empty() {
            return Ok(());
        }

        // Write stdout (NDJSON) to temp file
        let mut temp_file = crate::fsutil::create_ralph_temp_file("plugin")
            .context("create temp file for post_run")?;
        temp_file
            .write_all(stdout.as_bytes())
            .context("write stdout to temp file")?;
        let temp_path = temp_file.into_temp_path();

        for (plugin_id, _discovered) in chain {
            self.invoke_hook(plugin_id, ProcessorHook::PostRun, task_id, &temp_path)?;
        }

        Ok(())
    }

    /// Build the list of enabled processor plugins that support the given hook.
    /// Returns plugins in ascending lexicographic order by plugin_id (deterministic).
    fn build_processor_chain(
        &self,
        hook: ProcessorHook,
    ) -> Vec<(&String, &crate::plugins::discovery::DiscoveredPlugin)> {
        let hook_str = hook.as_str();
        self.registry
            .discovered()
            .iter()
            .filter(|(id, discovered)| {
                // Plugin must be enabled
                if !self.registry.is_enabled(id) {
                    return false;
                }
                // Must have processors section
                let Some(processors) = &discovered.manifest.processors else {
                    return false;
                };
                // Must support the requested hook
                processors.hooks.iter().any(|h| h == hook_str)
            })
            .collect()
    }

    /// Invoke a single hook on a single plugin.
    fn invoke_hook(
        &self,
        plugin_id: &str,
        hook: ProcessorHook,
        task_id: &str,
        file_path: &Path,
    ) -> Result<()> {
        let bin_path = self
            .registry
            .resolve_processor_bin(plugin_id)
            .with_context(|| format!("resolve processor binary for plugin {plugin_id}"))?;

        // Get plugin config blob for env var
        let config_json = self
            .registry
            .plugin_config_blob(plugin_id)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());

        let hook_str = hook.as_str();
        let output = std::process::Command::new(&bin_path)
            .current_dir(self.repo_root)
            .arg(hook_str)
            .arg(task_id)
            .arg(file_path)
            .env("RALPH_PLUGIN_ID", plugin_id)
            .env("RALPH_PLUGIN_CONFIG_JSON", config_json)
            .output()
            .with_context(|| {
                format!("failed to execute processor {plugin_id} for hook {hook_str}")
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let redacted_stderr = crate::redaction::redact_text(&stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            anyhow::bail!(
                "Processor hook failed: plugin={plugin_id}, hook={hook_str}, exit_code={exit_code}\n\
                 stderr: {redacted_stderr}"
            );
        }

        log::debug!(
            "Processor hook succeeded: plugin={plugin_id}, hook={hook_str}, task={task_id}"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, Task, TaskPriority, TaskStatus};
    use crate::plugins::manifest::{PluginManifest, ProcessorPlugin};
    use std::io::Write;
    use tempfile::TempDir;

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
            name: format!("Plugin {}", id),
            description: None,
            runner: None,
            processors: Some(ProcessorPlugin {
                bin: "processor.sh".to_string(),
                hooks: hooks.iter().map(|s| s.to_string()).collect(),
            }),
        };

        std::fs::create_dir_all(dir)?;
        let manifest_path = dir.join("plugin.json");
        let mut file = std::fs::File::create(&manifest_path)?;
        file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;

        let script_path = dir.join("processor.sh");
        let mut script_file = std::fs::File::create(&script_path)?;
        script_file.write_all(script_content.as_bytes())?;

        // Make executable (Unix only; Windows uses file extensions)
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

        // Should succeed with no plugins
        exec.validate_task(&task).unwrap();
    }

    #[test]
    fn test_pre_prompt_mutates_prompt() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

        // Create a processor that appends a marker to the prompt
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

        // Create two plugins with IDs that will be sorted: a.plugin and b.plugin
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

        let original_prompt = "X";
        let final_prompt = exec.pre_prompt("RQ-0001", original_prompt).unwrap();

        // Should be "XAB" because a.plugin runs before b.plugin (lexicographic order)
        assert_eq!(final_prompt, "XAB");
    }

    #[test]
    fn test_hook_filtering_plugin_without_hook_not_invoked() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/test.plugin");

        // Create a processor that only supports validate_task (not pre_prompt)
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

        // Call pre_prompt - the plugin should not be invoked
        let _ = exec.pre_prompt("RQ-0001", "test").unwrap();

        // The file should not exist because pre_prompt is not supported
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

    // Import needed for tests
    use crate::plugins::registry::PluginRegistry;
}
