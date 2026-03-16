//! Update-wizard flow for AGENTS.md section edits.
//!
//! Responsibilities:
//! - Let users choose existing sections to update.
//! - Collect replacement content for selected sections.
//! - Confirm the final batch of updates before returning it.
//!
//! Not handled here:
//! - Markdown parsing or merge logic.
//! - Writing merged content back to disk.
//!
//! Invariants/assumptions:
//! - Selected section indices were validated by the prompter.
//! - Empty edited content is discarded before final confirmation.

use super::prompt::ContextPrompter;
use super::types::UpdateWizardResult;
use anyhow::Result;

const INPUT_METHOD_OPTIONS: [&str; 2] = ["Type in editor (multi-line)", "Type single line"];

pub(crate) fn run_update_wizard(
    prompter: &dyn ContextPrompter,
    existing_sections: &[String],
    _existing_content: &str,
) -> Result<UpdateWizardResult> {
    if existing_sections.is_empty() {
        anyhow::bail!("No sections found in existing AGENTS.md");
    }

    let section_items = existing_sections.to_vec();
    let section_defaults = vec![false; section_items.len()];
    let selected_indices = prompter.multi_select(
        "Select sections to update (Space to select, Enter to confirm)",
        &section_items,
        &section_defaults,
    )?;

    if selected_indices.is_empty() {
        anyhow::bail!("No sections selected for update");
    }

    let mut updates = Vec::new();
    for idx in selected_indices {
        let section_name = &existing_sections[idx];
        let new_content = prompt_section_content(prompter, section_name)?;
        if !new_content.is_empty() {
            updates.push((section_name.clone(), new_content));
        }
    }

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

fn prompt_section_content(prompter: &dyn ContextPrompter, section_name: &str) -> Result<String> {
    let input_method = prompter.select(
        &format!("How would you like to add content to '{}'?", section_name),
        &input_method_items(),
        0,
    )?;

    let content = if input_method == 0 {
        prompter.edit(
            &format!("Adding to '{}' - save and close when done", section_name),
            &editor_initial_content(section_name),
        )?
    } else {
        prompter.input(&format!("New content for '{}'", section_name), None, true)?
    };

    Ok(clean_section_content(section_name, &content))
}

fn input_method_items() -> Vec<String> {
    INPUT_METHOD_OPTIONS
        .iter()
        .map(|option| (*option).to_string())
        .collect()
}

fn editor_initial_content(section_name: &str) -> String {
    format!(
        "\n\n<!-- Enter your new content for '{}' above this line -->\n",
        section_name
    )
}

fn clean_section_content(section_name: &str, content: &str) -> String {
    content
        .replace(
            &format!(
                "<!-- Enter your new content for '{}' above this line -->\n",
                section_name
            ),
            "",
        )
        .trim()
        .to_string()
}
