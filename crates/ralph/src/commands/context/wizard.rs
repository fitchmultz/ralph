//! Interactive wizard for AGENTS.md context initialization and updates.
//!
//! Responsibilities:
//! - Collect user preferences for context initialization (project type, config hints).
//! - Guide users through updating existing AGENTS.md sections interactively.
//! - Provide testable prompt abstractions via the `ContextPrompter` trait.
//!
//! Not handled here:
//! - File I/O (handled by the main context module).
//! - Markdown merging logic (handled by the merge module).
//!
//! Invariants/assumptions:
//! - Wizard is only run in interactive TTY environments (enforced by CLI layer).
//! - User inputs are validated before returning wizard results.

use crate::cli::context::ProjectTypeHint;
use anyhow::{Context as _, Result};
use dialoguer::{Confirm, Input, MultiSelect, Select};

/// Trait for prompting user input, allowing testable implementations.
pub trait ContextPrompter {
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
pub struct DialoguerPrompter;

impl ContextPrompter for DialoguerPrompter {
    fn select(&self, prompt: &str, items: &[String], default: usize) -> Result<usize> {
        Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()
            .with_context(|| format!("failed to get selection for: {}", prompt))
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
            .with_context(|| format!("failed to get multi-selection for: {}", prompt))
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
        Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()
            .with_context(|| format!("failed to get confirmation for: {}", prompt))
    }

    fn input(&self, prompt: &str, default: Option<&str>, allow_empty: bool) -> Result<String> {
        let mut input = Input::new();
        input = input.with_prompt(prompt).allow_empty(allow_empty);
        if let Some(d) = default {
            input = input.default(d.to_string());
        }
        input
            .interact_text()
            .with_context(|| format!("failed to get input for: {}", prompt))
    }

    fn edit(&self, prompt: &str, initial: &str) -> Result<String> {
        // Use dialoguer's Editor for multi-line input
        dialoguer::Editor::new()
            .edit(initial)
            .with_context(|| format!("failed to edit content for: {}", prompt))?
            .ok_or_else(|| anyhow::anyhow!("Editor was cancelled"))
    }
}

/// Scripted prompter for testing with predetermined responses.
#[derive(Debug)]
pub struct ScriptedPrompter {
    /// Queue of responses for different prompt types
    pub responses: Vec<ScriptedResponse>,
    /// Current response index
    index: std::cell::Cell<usize>,
}

/// Types of scripted responses.
#[derive(Debug, Clone)]
pub enum ScriptedResponse {
    /// Single selection (index)
    Select(usize),
    /// Multiple selection (indices)
    MultiSelect(Vec<usize>),
    /// Confirmation (yes/no)
    Confirm(bool),
    /// Text input
    Input(String),
    /// Editor result
    Edit(String),
}

impl ScriptedPrompter {
    /// Create a new scripted prompter with the given responses.
    pub fn new(responses: Vec<ScriptedResponse>) -> Self {
        Self {
            responses,
            index: std::cell::Cell::new(0),
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
            ScriptedResponse::Confirm(val) => Ok(val),
            other => anyhow::bail!(
                "Expected Confirm response for '{}', got {:?}",
                prompt,
                other
            ),
        }
    }

    fn input(&self, prompt: &str, _default: Option<&str>, _allow_empty: bool) -> Result<String> {
        match self.next_response()? {
            ScriptedResponse::Input(val) => Ok(val),
            other => anyhow::bail!("Expected Input response for '{}', got {:?}", prompt, other),
        }
    }

    fn edit(&self, prompt: &str, _initial: &str) -> Result<String> {
        match self.next_response()? {
            ScriptedResponse::Edit(val) => Ok(val),
            other => anyhow::bail!("Expected Edit response for '{}', got {:?}", prompt, other),
        }
    }
}

/// Configuration hints collected during init wizard.
#[derive(Debug, Clone)]
pub struct ConfigHints {
    /// Project description to replace placeholder.
    pub project_description: Option<String>,
    /// CI command (default: make ci).
    pub ci_command: String,
    /// Build command (default: make build).
    pub build_command: String,
    /// Test command (default: make test).
    pub test_command: String,
    /// Lint command (default: make lint).
    pub lint_command: String,
    /// Format command (default: make format).
    pub format_command: String,
}

impl Default for ConfigHints {
    fn default() -> Self {
        Self {
            project_description: None,
            ci_command: "make ci".to_string(),
            build_command: "make build".to_string(),
            test_command: "make test".to_string(),
            lint_command: "make lint".to_string(),
            format_command: "make format".to_string(),
        }
    }
}

/// Result of the init wizard.
#[derive(Debug, Clone)]
pub struct InitWizardResult {
    /// Selected project type.
    pub project_type: ProjectTypeHint,
    /// Optional output path override.
    pub output_path: Option<std::path::PathBuf>,
    /// Config hints for customizing the generated content.
    pub config_hints: ConfigHints,
    /// Whether to confirm before writing.
    pub confirm_write: bool,
}

/// Run the interactive init wizard.
pub fn run_init_wizard(
    prompter: &dyn ContextPrompter,
    detected_type: ProjectTypeHint,
    default_output: &std::path::Path,
) -> Result<InitWizardResult> {
    // Project type selection
    let project_types = vec![
        "Rust".to_string(),
        "Python".to_string(),
        "TypeScript".to_string(),
        "Go".to_string(),
        "Generic".to_string(),
    ];

    let default_idx = match detected_type {
        ProjectTypeHint::Rust => 0,
        ProjectTypeHint::Python => 1,
        ProjectTypeHint::TypeScript => 2,
        ProjectTypeHint::Go => 3,
        ProjectTypeHint::Generic => 4,
    };

    let type_idx = prompter.select("Select project type", &project_types, default_idx)?;

    let project_type = match type_idx {
        0 => ProjectTypeHint::Rust,
        1 => ProjectTypeHint::Python,
        2 => ProjectTypeHint::TypeScript,
        3 => ProjectTypeHint::Go,
        _ => ProjectTypeHint::Generic,
    };

    // Output path
    let use_custom_path = prompter.confirm(
        &format!("Use default output path ({})?", default_output.display()),
        true,
    )?;

    let output_path = if use_custom_path {
        None
    } else {
        let path_str: String = prompter.input(
            "Enter output path",
            Some(&default_output.to_string_lossy()),
            false,
        )?;
        Some(std::path::PathBuf::from(path_str))
    };

    // Config hints
    let customize = prompter.confirm("Customize build/test commands?", false)?;

    let mut config_hints = ConfigHints::default();

    if customize {
        config_hints.ci_command =
            prompter.input("CI command", Some(&config_hints.ci_command), false)?;
        config_hints.build_command =
            prompter.input("Build command", Some(&config_hints.build_command), false)?;
        config_hints.test_command =
            prompter.input("Test command", Some(&config_hints.test_command), false)?;
        config_hints.lint_command =
            prompter.input("Lint command", Some(&config_hints.lint_command), false)?;
        config_hints.format_command =
            prompter.input("Format command", Some(&config_hints.format_command), false)?;
    }

    // Project description
    let add_description = prompter.confirm("Add a project description?", false)?;

    if add_description {
        config_hints.project_description =
            Some(prompter.input("Project description", None, true)?);
    }

    // Confirm before write
    let confirm_write = prompter.confirm("Preview and confirm before writing?", true)?;

    Ok(InitWizardResult {
        project_type,
        output_path,
        config_hints,
        confirm_write,
    })
}

/// Result of the update wizard: section name -> new content.
pub type UpdateWizardResult = Vec<(String, String)>;

/// Run the interactive update wizard.
///
/// Presents existing sections for selection, then prompts for new content for each.
pub fn run_update_wizard(
    prompter: &dyn ContextPrompter,
    existing_sections: &[String],
    _existing_content: &str,
) -> Result<UpdateWizardResult> {
    if existing_sections.is_empty() {
        anyhow::bail!("No sections found in existing AGENTS.md");
    }

    // Let user select sections to update
    let items: Vec<String> = existing_sections.iter().map(|s| s.to_string()).collect();
    let defaults = vec![false; items.len()];

    let selected_indices = prompter.multi_select(
        "Select sections to update (Space to select, Enter to confirm)",
        &items,
        &defaults,
    )?;

    if selected_indices.is_empty() {
        anyhow::bail!("No sections selected for update");
    }

    let mut updates = Vec::new();

    // For each selected section, prompt for new content
    for idx in selected_indices {
        let section_name = &existing_sections[idx];

        let input_method = prompter.select(
            &format!("How would you like to add content to '{}'?", section_name),
            &[
                "Type in editor (multi-line)".to_string(),
                "Type single line".to_string(),
            ],
            0,
        )?;

        let new_content = if input_method == 0 {
            // Use editor
            let initial = format!(
                "\n\n<!-- Enter your new content for '{}' above this line -->\n",
                section_name
            );
            prompter.edit(
                &format!("Adding to '{}' - save and close when done", section_name),
                &initial,
            )?
        } else {
            // Single line input
            prompter.input(&format!("New content for '{}'", section_name), None, true)?
        };

        // Clean up the content (remove the placeholder comment if present)
        let new_content = new_content
            .replace(
                &format!(
                    "<!-- Enter your new content for '{}' above this line -->\n",
                    section_name
                ),
                "",
            )
            .trim()
            .to_string();

        if !new_content.is_empty() {
            updates.push((section_name.clone(), new_content));
        }
    }

    // Confirm before applying
    if updates.is_empty() {
        anyhow::bail!("No content was entered for any section");
    }

    let proceed = prompter.confirm(
        &format!("Update {} section(s) with new content?", updates.len()),
        true,
    )?;

    if !proceed {
        anyhow::bail!("Update cancelled by user");
    }

    Ok(updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_prompter_works() {
        let prompter = ScriptedPrompter::new(vec![
            ScriptedResponse::Select(1),      // Select Python
            ScriptedResponse::Confirm(true),  // Use default path
            ScriptedResponse::Confirm(false), // Don't customize commands
            ScriptedResponse::Confirm(false), // Don't add description
            ScriptedResponse::Confirm(false), // Don't confirm before write
        ]);

        let result = run_init_wizard(
            &prompter,
            ProjectTypeHint::Generic,
            std::path::Path::new("AGENTS.md"),
        );

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(matches!(result.project_type, ProjectTypeHint::Python));
    }

    #[test]
    fn scripted_prompter_out_of_responses() {
        let prompter = ScriptedPrompter::new(vec![]);

        let result = run_init_wizard(
            &prompter,
            ProjectTypeHint::Generic,
            std::path::Path::new("AGENTS.md"),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("ran out of responses")
        );
    }

    #[test]
    fn scripted_prompter_type_mismatch() {
        let prompter = ScriptedPrompter::new(vec![
            ScriptedResponse::Confirm(true), // Wrong type - should be Select
        ]);

        let result = run_init_wizard(
            &prompter,
            ProjectTypeHint::Generic,
            std::path::Path::new("AGENTS.md"),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Expected Select"));
    }

    #[test]
    fn update_wizard_no_sections() {
        let prompter = ScriptedPrompter::new(vec![]);

        let result = run_update_wizard(&prompter, &[], "");

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No sections found")
        );
    }

    #[test]
    fn update_wizard_selects_sections() {
        let prompter = ScriptedPrompter::new(vec![
            ScriptedResponse::MultiSelect(vec![0, 2]), // Select sections 0 and 2
            ScriptedResponse::Select(0),               // Use editor for first section
            ScriptedResponse::Edit("New content for section 1".to_string()),
            ScriptedResponse::Select(1), // Use single line for second section
            ScriptedResponse::Input("New content for section 3".to_string()),
            ScriptedResponse::Confirm(true), // Proceed with update
        ]);

        let sections = vec![
            "Section 1".to_string(),
            "Section 2".to_string(),
            "Section 3".to_string(),
        ];

        let result = run_update_wizard(&prompter, &sections, "");

        assert!(result.is_ok());
        let updates = result.unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].0, "Section 1");
        assert_eq!(updates[0].1, "New content for section 1");
        assert_eq!(updates[1].0, "Section 3");
        assert_eq!(updates[1].1, "New content for section 3");
    }

    #[test]
    fn update_wizard_cancellation() {
        let prompter = ScriptedPrompter::new(vec![
            ScriptedResponse::MultiSelect(vec![0]),
            ScriptedResponse::Select(1), // Single line
            ScriptedResponse::Input("Content".to_string()),
            ScriptedResponse::Confirm(false), // Cancel
        ]);

        let sections = vec!["Section 1".to_string()];

        let result = run_update_wizard(&prompter, &sections, "");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn config_hints_default() {
        let hints = ConfigHints::default();
        assert_eq!(hints.ci_command, "make ci");
        assert_eq!(hints.build_command, "make build");
        assert_eq!(hints.test_command, "make test");
        assert_eq!(hints.lint_command, "make lint");
        assert_eq!(hints.format_command, "make format");
        assert!(hints.project_description.is_none());
    }
}
