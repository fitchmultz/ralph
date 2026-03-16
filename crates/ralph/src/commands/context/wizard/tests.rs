//! Wizard-specific unit tests.
//!
//! Responsibilities:
//! - Verify scripted prompt playback and validation errors.
//! - Cover init and update wizard result shaping.
//! - Keep wizard regression coverage adjacent to the split helper modules.
//!
//! Not handled here:
//! - End-to-end context command workflow coverage.
//!
//! Invariants/assumptions:
//! - Tests preserve prompt ordering expected by scripted helpers.
//! - Wizard defaults remain aligned with rendered AGENTS.md placeholders.

use super::init::run_init_wizard;
use super::scripted::{ScriptedPrompter, ScriptedResponse};
use super::types::ConfigHints;
use super::update::run_update_wizard;
use crate::cli::context::ProjectTypeHint;
use std::path::Path;

#[test]
fn scripted_prompter_works() {
    let prompter = ScriptedPrompter::new(vec![
        ScriptedResponse::Select(1),      // Select Python
        ScriptedResponse::Confirm(true),  // Use default path
        ScriptedResponse::Confirm(false), // Don't customize commands
        ScriptedResponse::Confirm(false), // Don't add description
        ScriptedResponse::Confirm(false), // Don't confirm before write
    ]);

    let result = run_init_wizard(&prompter, ProjectTypeHint::Generic, Path::new("AGENTS.md"));

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(matches!(result.project_type, ProjectTypeHint::Python));
}

#[test]
fn scripted_prompter_out_of_responses() {
    let prompter = ScriptedPrompter::new(vec![]);

    let result = run_init_wizard(&prompter, ProjectTypeHint::Generic, Path::new("AGENTS.md"));

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

    let result = run_init_wizard(&prompter, ProjectTypeHint::Generic, Path::new("AGENTS.md"));

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
fn update_wizard_strips_editor_placeholder() {
    let prompter = ScriptedPrompter::new(vec![
        ScriptedResponse::MultiSelect(vec![0]),
        ScriptedResponse::Select(0),
        ScriptedResponse::Edit(
            "New guidance\n<!-- Enter your new content for 'Section 1' above this line -->\n"
                .to_string(),
        ),
        ScriptedResponse::Confirm(true),
    ]);

    let result = run_update_wizard(&prompter, &["Section 1".to_string()], "").unwrap();

    assert_eq!(
        result,
        vec![("Section 1".to_string(), "New guidance".to_string())]
    );
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
