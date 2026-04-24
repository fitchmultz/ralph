//! Init-wizard flow for AGENTS.md generation.
//!
//! Purpose:
//! - Init-wizard flow for AGENTS.md generation.
//!
//! Responsibilities:
//! - Collect project type, output-path, and command customization hints.
//! - Return a structured init result for the workflow layer.
//!
//! Not handled here:
//! - Writing generated files.
//! - Rendering AGENTS.md templates.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Prompt order remains stable for scripted tests.
//! - Project-type index mapping stays aligned with displayed options.

use super::prompt::ContextPrompter;
use super::types::{ConfigHints, InitWizardResult};
use crate::cli::context::ProjectTypeHint;
use anyhow::Result;
use std::path::{Path, PathBuf};

const PROJECT_TYPE_LABELS: [&str; 5] = ["Rust", "Python", "TypeScript", "Go", "Generic"];

pub(crate) fn run_init_wizard(
    prompter: &dyn ContextPrompter,
    detected_type: ProjectTypeHint,
    default_output: &Path,
) -> Result<InitWizardResult> {
    let type_idx = prompter.select(
        "Select project type",
        &project_type_items(),
        default_project_type_index(detected_type),
    )?;

    let output_path = prompt_output_path(prompter, default_output)?;
    let config_hints = prompt_config_hints(prompter)?;
    let confirm_write = prompter.confirm("Preview and confirm before writing?", true)?;

    Ok(InitWizardResult {
        project_type: project_type_from_index(type_idx),
        output_path,
        config_hints,
        confirm_write,
    })
}

fn project_type_items() -> Vec<String> {
    PROJECT_TYPE_LABELS
        .iter()
        .map(|label| (*label).to_string())
        .collect()
}

fn default_project_type_index(detected_type: ProjectTypeHint) -> usize {
    match detected_type {
        ProjectTypeHint::Rust => 0,
        ProjectTypeHint::Python => 1,
        ProjectTypeHint::TypeScript => 2,
        ProjectTypeHint::Go => 3,
        ProjectTypeHint::Generic => 4,
    }
}

fn project_type_from_index(index: usize) -> ProjectTypeHint {
    match index {
        0 => ProjectTypeHint::Rust,
        1 => ProjectTypeHint::Python,
        2 => ProjectTypeHint::TypeScript,
        3 => ProjectTypeHint::Go,
        _ => ProjectTypeHint::Generic,
    }
}

fn prompt_output_path(
    prompter: &dyn ContextPrompter,
    default_output: &Path,
) -> Result<Option<PathBuf>> {
    let use_default_path = prompter.confirm(
        &format!("Use default output path ({})?", default_output.display()),
        true,
    )?;

    if use_default_path {
        return Ok(None);
    }

    let path = prompter.input(
        "Enter output path",
        Some(&default_output.to_string_lossy()),
        false,
    )?;
    Ok(Some(PathBuf::from(path)))
}

fn prompt_config_hints(prompter: &dyn ContextPrompter) -> Result<ConfigHints> {
    let mut config_hints = ConfigHints::default();

    if prompter.confirm("Customize build/test commands?", false)? {
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

    if prompter.confirm("Add a project description?", false)? {
        config_hints.project_description =
            Some(prompter.input("Project description", None, true)?);
    }

    Ok(config_hints)
}
