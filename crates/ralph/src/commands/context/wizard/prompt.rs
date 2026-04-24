//! Prompt abstractions for the AGENTS.md wizard.
//!
//! Purpose:
//! - Prompt abstractions for the AGENTS.md wizard.
//!
//! Responsibilities:
//! - Define the prompt operations required by the init and update wizards.
//! - Provide the dialoguer-backed interactive implementation.
//!
//! Not handled here:
//! - Scripted test prompt playback.
//! - Wizard step orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Prompt failures are surfaced with prompt-specific context.
//! - Callers provide already-validated prompt defaults and option lists.

use anyhow::{Context as _, Result};
use dialoguer::{Confirm, Input, MultiSelect, Select};

/// Trait for prompting user input, allowing testable implementations.
pub(crate) trait ContextPrompter {
    /// Select a single item from a list. Returns the index of the selected item.
    fn select(&self, prompt: &str, items: &[String], default: usize) -> Result<usize>;

    /// Select multiple items from a list. Returns indices of selected items.
    fn multi_select(&self, prompt: &str, items: &[String], defaults: &[bool])
    -> Result<Vec<usize>>;

    /// Confirm a yes/no question.
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool>;

    /// Get text input from user.
    fn input(&self, prompt: &str, default: Option<&str>, allow_empty: bool) -> Result<String>;

    /// Edit multi-line text in an editor.
    fn edit(&self, prompt: &str, initial: &str) -> Result<String>;
}

/// Dialoguer-based prompter for interactive terminal use.
pub(crate) struct DialoguerPrompter;

impl ContextPrompter for DialoguerPrompter {
    fn select(&self, prompt: &str, items: &[String], default: usize) -> Result<usize> {
        Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()
            .with_context(|| format!("failed to get selection for: {prompt}"))
    }

    fn multi_select(
        &self,
        prompt: &str,
        items: &[String],
        defaults: &[bool],
    ) -> Result<Vec<usize>> {
        MultiSelect::new()
            .with_prompt(prompt)
            .items(items)
            .defaults(defaults)
            .interact()
            .with_context(|| format!("failed to get multi-selection for: {prompt}"))
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
        Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .with_context(|| format!("failed to get confirmation for: {prompt}"))
    }

    fn input(&self, prompt: &str, default: Option<&str>, allow_empty: bool) -> Result<String> {
        let mut input = Input::new();
        input = input.with_prompt(prompt).allow_empty(allow_empty);
        if let Some(default_value) = default {
            input = input.default(default_value.to_string());
        }
        input
            .interact_text()
            .with_context(|| format!("failed to get input for: {prompt}"))
    }

    fn edit(&self, prompt: &str, initial: &str) -> Result<String> {
        dialoguer::Editor::new()
            .edit(initial)
            .with_context(|| format!("failed to edit content for: {prompt}"))?
            .ok_or_else(|| anyhow::anyhow!("Editor was cancelled"))
    }
}
