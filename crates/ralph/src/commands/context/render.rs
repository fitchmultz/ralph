//! AGENTS.md template rendering helpers.
//!
//! Purpose:
//! - AGENTS.md template rendering helpers.
//!
//! Responsibilities:
//! - Select the correct embedded template for the detected project type.
//! - Build repository-map placeholders from repo structure.
//! - Fill template placeholders using resolved config and wizard hints.
//!
//! Not handled here:
//! - Interactive prompting or file writes.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::types::DetectedProjectType;
use super::wizard;
use crate::config;
use crate::constants::versions::TEMPLATE_VERSION;
use anyhow::Result;

const TEMPLATE_GENERIC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/agents_templates/generic.md"
));
const TEMPLATE_RUST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/agents_templates/rust.md"
));
const TEMPLATE_PYTHON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/agents_templates/python.md"
));
const TEMPLATE_TYPESCRIPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/agents_templates/typescript.md"
));
const TEMPLATE_GO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/agents_templates/go.md"
));

fn format_rfc3339_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

pub(super) fn generate_agents_md(
    resolved: &config::Resolved,
    project_type: DetectedProjectType,
) -> Result<String> {
    generate_agents_md_with_hints(resolved, project_type, None)
}

pub(super) fn generate_agents_md_with_hints(
    resolved: &config::Resolved,
    project_type: DetectedProjectType,
    hints: Option<&wizard::ConfigHints>,
) -> Result<String> {
    let template = match project_type {
        DetectedProjectType::Rust => TEMPLATE_RUST,
        DetectedProjectType::Python => TEMPLATE_PYTHON,
        DetectedProjectType::TypeScript => TEMPLATE_TYPESCRIPT,
        DetectedProjectType::Go => TEMPLATE_GO,
        DetectedProjectType::Generic => TEMPLATE_GENERIC,
    };

    let repo_map = build_repository_map(resolved)?;
    let project_name = resolved
        .repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project")
        .to_string();
    let id_prefix = resolved.id_prefix.clone();

    let project_description = hints
        .and_then(|h| h.project_description.as_deref())
        .unwrap_or("Add a brief description of your project here.");
    let ci_command = hints.map(|h| h.ci_command.as_str()).unwrap_or("make ci");
    let build_command = hints
        .map(|h| h.build_command.as_str())
        .unwrap_or("make build");
    let test_command = hints
        .map(|h| h.test_command.as_str())
        .unwrap_or("make test");
    let lint_command = hints
        .map(|h| h.lint_command.as_str())
        .unwrap_or("make lint");
    let format_command = hints
        .map(|h| h.format_command.as_str())
        .unwrap_or("make format");

    Ok(template
        .replace("{project_name}", &project_name)
        .replace("{project_description}", project_description)
        .replace("{repository_map}", &repo_map)
        .replace("{ci_command}", ci_command)
        .replace("{build_command}", build_command)
        .replace("{test_command}", test_command)
        .replace("{lint_command}", lint_command)
        .replace("{format_command}", format_command)
        .replace(
            "{package_name}",
            &project_name.to_lowercase().replace(" ", "-"),
        )
        .replace(
            "{module_name}",
            &project_name.to_lowercase().replace(" ", "_"),
        )
        .replace("{id_prefix}", &id_prefix)
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{timestamp}", &format_rfc3339_now())
        .replace("{template_version}", TEMPLATE_VERSION))
}

fn build_repository_map(resolved: &config::Resolved) -> Result<String> {
    let mut entries = Vec::new();

    let dirs_to_check = [
        ("src", "Source code"),
        ("lib", "Library code"),
        ("bin", "Binary/executable code"),
        ("tests", "Tests"),
        ("docs", "Documentation"),
        ("crates", "Rust workspace crates"),
        ("packages", "Package subdirectories"),
        ("scripts", "Utility scripts"),
        (".ralph", "Ralph runtime state (queue, config)"),
    ];

    for (dir, desc) in &dirs_to_check {
        if resolved.repo_root.join(dir).exists() {
            entries.push(format!("- `{}/`: {}", dir, desc));
        }
    }

    let files_to_check = [
        ("README.md", "Project overview"),
        ("Makefile", "Build automation"),
        ("Cargo.toml", "Rust package manifest"),
        ("pyproject.toml", "Python package manifest"),
        ("package.json", "Node.js package manifest"),
        ("go.mod", "Go module definition"),
    ];

    for (file, desc) in &files_to_check {
        if resolved.repo_root.join(file).exists() {
            entries.push(format!("- `{}`: {}", file, desc));
        }
    }

    if entries.is_empty() {
        entries.push("- Add your repository structure here".to_string());
    }

    Ok(entries.join("\n"))
}
