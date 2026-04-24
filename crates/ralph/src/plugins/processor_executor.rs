//! Processor plugin hook execution facade.
//!
//! Purpose:
//! - Processor plugin hook execution facade.
//!
//! Responsibilities:
//! - Invoke enabled processor plugins for supported hooks.
//! - Keep payload IO and hook dispatch split by concern.
//! - Preserve deterministic plugin chaining order through the registry.
//!
//! Not handled here:
//! - Plugin discovery or enable policy.
//! - Runner execution, CI gate orchestration, or queue mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Plugins are trusted and non-zero exit remains a hard failure.
//! - Hook payload files follow the processor protocol and stay UTF-8 text.

mod hook_dispatch;
mod io;

#[cfg(test)]
mod tests;

use std::path::Path;

use anyhow::Result;

use crate::contracts::Task;
use crate::plugins::registry::PluginRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessorHook {
    ValidateTask,
    PrePrompt,
    PostRun,
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

        let temp_path = self.write_task_payload(task)?;
        for (plugin_id, _) in chain {
            self.invoke_hook(
                plugin_id,
                ProcessorHook::ValidateTask,
                &task.id,
                temp_path.as_ref(),
            )?;
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

        let temp_path = self.write_text_payload("plugin", prompt, "pre_prompt")?;
        for (plugin_id, _) in chain {
            self.invoke_hook(
                plugin_id,
                ProcessorHook::PrePrompt,
                task_id,
                temp_path.as_ref(),
            )?;
        }
        self.read_text_payload(temp_path.as_ref(), "modified prompt")
    }

    /// Invoke post_run hooks for all enabled processor plugins.
    /// Non-zero exit from any plugin aborts with an actionable error.
    pub(crate) fn post_run(&self, task_id: &str, stdout: &str) -> Result<()> {
        let chain = self.build_processor_chain(ProcessorHook::PostRun);
        if chain.is_empty() {
            return Ok(());
        }

        let temp_path = self.write_text_payload("plugin", stdout, "post_run")?;
        for (plugin_id, _) in chain {
            self.invoke_hook(
                plugin_id,
                ProcessorHook::PostRun,
                task_id,
                temp_path.as_ref(),
            )?;
        }
        Ok(())
    }
}
