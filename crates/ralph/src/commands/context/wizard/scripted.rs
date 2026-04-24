//! Scripted prompt playback for AGENTS.md wizard tests.
//!
//! Purpose:
//! - Scripted prompt playback for AGENTS.md wizard tests.
//!
//! Responsibilities:
//! - Provide deterministic prompt responses for wizard unit tests.
//! - Validate scripted selections against the prompt option set.
//!
//! Not handled here:
//! - Interactive terminal prompting.
//! - Wizard init/update flow orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Responses are consumed in-order.
//! - Index-based selections must be in range for the prompt being answered.

use super::prompt::ContextPrompter;
use anyhow::Result;
use std::cell::Cell;

/// Scripted prompter for testing with predetermined responses.
#[derive(Debug)]
pub(crate) struct ScriptedPrompter {
    /// Queue of responses for different prompt types.
    pub(crate) responses: Vec<ScriptedResponse>,
    /// Current response index.
    index: Cell<usize>,
}

/// Types of scripted responses.
#[derive(Debug, Clone)]
pub(crate) enum ScriptedResponse {
    /// Single selection (index).
    Select(usize),
    /// Multiple selection (indices).
    MultiSelect(Vec<usize>),
    /// Confirmation (yes/no).
    Confirm(bool),
    /// Text input.
    Input(String),
    /// Editor result.
    Edit(String),
}

impl ScriptedPrompter {
    /// Create a new scripted prompter with the given responses.
    pub(crate) fn new(responses: Vec<ScriptedResponse>) -> Self {
        Self {
            responses,
            index: Cell::new(0),
        }
    }

    fn next_response(&self) -> Result<ScriptedResponse> {
        let idx = self.index.get();
        if idx >= self.responses.len() {
            anyhow::bail!(
                "Scripted prompter ran out of responses (requested #{}, have {})",
                idx + 1,
                self.responses.len()
            );
        }
        self.index.set(idx + 1);
        Ok(self.responses[idx].clone())
    }
}

impl ContextPrompter for ScriptedPrompter {
    fn select(&self, prompt: &str, items: &[String], _default: usize) -> Result<usize> {
        match self.next_response()? {
            ScriptedResponse::Select(idx) => {
                if idx >= items.len() {
                    anyhow::bail!(
                        "Scripted select index {} out of range for '{}' ({} items)",
                        idx,
                        prompt,
                        items.len()
                    );
                }
                Ok(idx)
            }
            other => anyhow::bail!("Expected Select response for '{}', got {:?}", prompt, other),
        }
    }

    fn multi_select(
        &self,
        prompt: &str,
        items: &[String],
        _defaults: &[bool],
    ) -> Result<Vec<usize>> {
        match self.next_response()? {
            ScriptedResponse::MultiSelect(indices) => {
                for &idx in &indices {
                    if idx >= items.len() {
                        anyhow::bail!(
                            "Scripted multi-select index {} out of range for '{}' ({} items)",
                            idx,
                            prompt,
                            items.len()
                        );
                    }
                }
                Ok(indices)
            }
            other => anyhow::bail!(
                "Expected MultiSelect response for '{}', got {:?}",
                prompt,
                other
            ),
        }
    }

    fn confirm(&self, prompt: &str, _default: bool) -> Result<bool> {
        match self.next_response()? {
            ScriptedResponse::Confirm(value) => Ok(value),
            other => anyhow::bail!(
                "Expected Confirm response for '{}', got {:?}",
                prompt,
                other
            ),
        }
    }

    fn input(&self, prompt: &str, _default: Option<&str>, _allow_empty: bool) -> Result<String> {
        match self.next_response()? {
            ScriptedResponse::Input(value) => Ok(value),
            other => anyhow::bail!("Expected Input response for '{}', got {:?}", prompt, other),
        }
    }

    fn edit(&self, prompt: &str, _initial: &str) -> Result<String> {
        match self.next_response()? {
            ScriptedResponse::Edit(value) => Ok(value),
            other => anyhow::bail!("Expected Edit response for '{}', got {:?}", prompt, other),
        }
    }
}
