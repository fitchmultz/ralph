//! Processor-hook selection and subprocess dispatch.
//!
//! Purpose:
//! - Processor-hook selection and subprocess dispatch.
//!
//! Responsibilities:
//! - Filter enabled processor plugins for a requested hook.
//! - Execute processor binaries with managed subprocess handling.
//! - Shape hook failures into actionable redacted errors.
//!
//! Not handled here:
//! - Temp-file payload creation.
//! - Plugin discovery or trust policy.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Processor chains run in deterministic ascending plugin-id order.
//! - stderr surfaced to users is redacted before being included in errors.

use std::path::Path;

use anyhow::{Context, Result};

use crate::plugins::discovery::DiscoveredPlugin;
use crate::runutil::{ManagedCommand, TimeoutClass, execute_managed_command};

use super::{ProcessorExecutor, ProcessorHook};

impl ProcessorHook {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ValidateTask => "validate_task",
            Self::PrePrompt => "pre_prompt",
            Self::PostRun => "post_run",
        }
    }
}

impl ProcessorExecutor<'_> {
    /// Build the list of enabled processor plugins that support the given hook.
    /// Returns plugins in ascending lexicographic order by plugin_id (deterministic).
    pub(super) fn build_processor_chain(
        &self,
        hook: ProcessorHook,
    ) -> Vec<(&String, &DiscoveredPlugin)> {
        let hook_str = hook.as_str();
        self.registry
            .discovered()
            .iter()
            .filter(|(id, discovered)| {
                if !self.registry.is_enabled(id) {
                    return false;
                }
                let Some(processors) = &discovered.manifest.processors else {
                    return false;
                };
                processors
                    .hooks
                    .iter()
                    .any(|supported| supported == hook_str)
            })
            .collect()
    }

    /// Invoke a single hook on a single plugin.
    pub(super) fn invoke_hook(
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

        let config_json = self
            .registry
            .plugin_config_blob(plugin_id)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "{}".to_string());

        let hook_str = hook.as_str();
        let mut command = std::process::Command::new(&bin_path);
        command
            .current_dir(self.repo_root)
            .arg(hook_str)
            .arg(task_id)
            .arg(file_path)
            .env("RALPH_PLUGIN_ID", plugin_id)
            .env("RALPH_PLUGIN_CONFIG_JSON", config_json);

        let output = execute_managed_command(ManagedCommand::new(
            command,
            format!("processor {plugin_id} {hook_str}"),
            TimeoutClass::PluginHook,
        ))
        .map(|managed_output| {
            if managed_output.stdout_truncated || managed_output.stderr_truncated {
                log::warn!("Processor hook capture truncated: plugin={plugin_id}, hook={hook_str}");
            }
            managed_output.into_output()
        })
        .with_context(|| format!("failed to execute processor {plugin_id} for hook {hook_str}"))?;

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
