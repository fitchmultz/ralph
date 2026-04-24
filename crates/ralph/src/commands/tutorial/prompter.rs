//! Testable prompt abstraction for tutorial phases.
//!
//! Purpose:
//! - Testable prompt abstraction for tutorial phases.
//!
//! Responsibilities:
//! - Define prompt operations needed by tutorial phases.
//! - Provide Dialoguer implementation for interactive use.
//! - Provide Scripted implementation for automated testing.
//!
//! Not handled here:
//! - Tutorial phase logic (see phases.rs).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Context, Result};

/// Trait for tutorial user prompts, allowing testable implementations.
pub trait TutorialPrompter {
    /// Pause and wait for user to press Enter to continue.
    fn pause(&self, message: &str) -> Result<()>;

    /// Ask a yes/no confirmation question.
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool>;

    /// Select from a list of options.
    fn select(&self, prompt: &str, items: &[&str], default: usize) -> Result<usize>;

    /// Display informational text (no input required).
    fn info(&self, message: &str);
}

/// Dialoguer-based prompter for interactive terminal use.
pub struct DialoguerTutorialPrompter;

impl TutorialPrompter for DialoguerTutorialPrompter {
    fn pause(&self, message: &str) -> Result<()> {
        dialoguer::Confirm::new()
            .with_prompt(message)
            .default(true)
            .show_default(false)
            .interact()
            .context("failed to get pause confirmation")?;
        Ok(())
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
        dialoguer::Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .context("failed to get confirmation")
    }

    fn select(&self, prompt: &str, items: &[&str], default: usize) -> Result<usize> {
        dialoguer::Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()
            .context("failed to get selection")
    }

    fn info(&self, message: &str) {
        println!("{}", message);
    }
}

/// Scripted prompter for testing with predetermined responses.
#[derive(Debug)]
pub struct ScriptedTutorialPrompter {
    /// Queue of responses
    pub responses: Vec<ScriptedResponse>,
    /// Current index
    index: std::cell::Cell<usize>,
    /// Captured info messages
    pub info_messages: std::cell::RefCell<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum ScriptedResponse {
    Pause,
    Confirm(bool),
    Select(usize),
}

impl ScriptedTutorialPrompter {
    pub fn new(responses: Vec<ScriptedResponse>) -> Self {
        Self {
            responses,
            index: std::cell::Cell::new(0),
            info_messages: std::cell::RefCell::new(Vec::new()),
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

impl TutorialPrompter for ScriptedTutorialPrompter {
    fn pause(&self, _message: &str) -> Result<()> {
        match self.next_response()? {
            ScriptedResponse::Pause => Ok(()),
            other => anyhow::bail!("Expected Pause response, got {:?}", other),
        }
    }

    fn confirm(&self, _prompt: &str, _default: bool) -> Result<bool> {
        match self.next_response()? {
            ScriptedResponse::Confirm(val) => Ok(val),
            other => anyhow::bail!("Expected Confirm response, got {:?}", other),
        }
    }

    fn select(&self, _prompt: &str, items: &[&str], _default: usize) -> Result<usize> {
        match self.next_response()? {
            ScriptedResponse::Select(idx) => {
                if idx >= items.len() {
                    anyhow::bail!("Select index {} out of range ({} items)", idx, items.len());
                }
                Ok(idx)
            }
            other => anyhow::bail!("Expected Select response, got {:?}", other),
        }
    }

    fn info(&self, message: &str) {
        self.info_messages.borrow_mut().push(message.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_prompter_handles_pause() {
        let prompter = ScriptedTutorialPrompter::new(vec![ScriptedResponse::Pause]);
        assert!(prompter.pause("test").is_ok());
    }

    #[test]
    fn scripted_prompter_handles_confirm() {
        let prompter = ScriptedTutorialPrompter::new(vec![ScriptedResponse::Confirm(true)]);
        assert!(prompter.confirm("test", false).unwrap());
    }

    #[test]
    fn scripted_prompter_handles_select() {
        let prompter = ScriptedTutorialPrompter::new(vec![ScriptedResponse::Select(1)]);
        assert_eq!(prompter.select("test", &["a", "b", "c"], 0).unwrap(), 1);
    }

    #[test]
    fn scripted_prompter_select_out_of_range_errors() {
        let prompter = ScriptedTutorialPrompter::new(vec![ScriptedResponse::Select(5)]);
        assert!(prompter.select("test", &["a", "b"], 0).is_err());
    }

    #[test]
    fn scripted_prompter_runs_out_of_responses() {
        let prompter = ScriptedTutorialPrompter::new(vec![]);
        assert!(prompter.pause("test").is_err());
    }

    #[test]
    fn scripted_prompter_captures_info_messages() {
        let prompter = ScriptedTutorialPrompter::new(vec![]);
        prompter.info("message 1");
        prompter.info("message 2");
        let messages = prompter.info_messages.borrow();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "message 1");
        assert_eq!(messages[1], "message 2");
    }
}
