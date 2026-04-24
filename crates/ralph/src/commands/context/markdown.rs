//! Markdown section parsing helpers for AGENTS.md updates and validation.
//!
//! Purpose:
//! - Markdown section parsing helpers for AGENTS.md updates and validation.
//!
//! Responsibilities:
//! - Parse second-level markdown sections into update payloads.
//! - Extract section titles for validation and update workflows.
//!
//! Not handled here:
//! - Merge policy or persistence.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

pub(super) fn parse_markdown_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_title = String::new();
    let mut current_content = Vec::new();

    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("## ") {
            if !current_title.is_empty() {
                sections.push((current_title, current_content.join("\n")));
            }
            current_title = stripped.trim().to_string();
            current_content = Vec::new();
        } else if !current_title.is_empty() {
            current_content.push(line.to_string());
        }
    }

    if !current_title.is_empty() {
        sections.push((current_title, current_content.join("\n")));
    }

    sections
}

pub(super) fn extract_section_titles(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix("## ").map(|s| s.trim().to_string()))
        .collect()
}
