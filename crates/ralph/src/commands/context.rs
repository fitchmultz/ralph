//! Project context (AGENTS.md) generation and management.
//!
//! Responsibilities:
//! - Generate initial AGENTS.md from project type detection.
//! - Update AGENTS.md with new learnings.
//! - Validate AGENTS.md against project structure.
//!
//! Not handled here:
//! - CLI argument parsing (see `cli::context`).
//! - Interactive prompts (see `wizard` module).
//!
//! Invariants/assumptions:
//! - Templates are embedded at compile time.
//! - Project type detection uses simple file-based heuristics.
//! - AGENTS.md updates preserve manual edits (section-based merging).

use crate::cli::context::ProjectTypeHint;
use crate::config;
use crate::constants::agents_md::{RECOMMENDED_SECTIONS, REQUIRED_SECTIONS};
use crate::constants::versions::TEMPLATE_VERSION;
use crate::fsutil;

pub mod merge;
pub mod wizard;

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use wizard::ContextPrompter;

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

/// Detected project type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedProjectType {
    Rust,
    Python,
    TypeScript,
    Go,
    Generic,
}

/// Format current time as RFC3339 using the `time` crate
fn format_rfc3339_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    // Format as RFC3339: 2026-01-28T12:34:56Z
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

impl std::fmt::Display for DetectedProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectedProjectType::Rust => write!(f, "rust"),
            DetectedProjectType::Python => write!(f, "python"),
            DetectedProjectType::TypeScript => write!(f, "typescript"),
            DetectedProjectType::Go => write!(f, "go"),
            DetectedProjectType::Generic => write!(f, "generic"),
        }
    }
}

/// Status of file initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInitStatus {
    Created,
    Valid,
}

/// Options for context init command
pub struct ContextInitOptions {
    pub force: bool,
    pub project_type_hint: Option<ProjectTypeHint>,
    pub output_path: std::path::PathBuf,
    pub interactive: bool,
}

/// Options for context update command
pub struct ContextUpdateOptions {
    pub sections: Vec<String>,
    pub file: Option<std::path::PathBuf>,
    pub interactive: bool,
    pub dry_run: bool,
    pub output_path: std::path::PathBuf,
}

/// Options for context validate command
pub struct ContextValidateOptions {
    pub strict: bool,
    pub path: std::path::PathBuf,
}

/// Report from init command
pub struct InitReport {
    pub status: FileInitStatus,
    pub detected_project_type: DetectedProjectType,
    pub output_path: std::path::PathBuf,
}

/// Report from update command
pub struct UpdateReport {
    pub sections_updated: Vec<String>,
    pub dry_run: bool,
}

/// Report from validate command
pub struct ValidateReport {
    pub valid: bool,
    pub missing_sections: Vec<String>,
    pub outdated_sections: Vec<String>,
}

/// Run the context init command
pub fn run_context_init(
    resolved: &config::Resolved,
    opts: ContextInitOptions,
) -> Result<InitReport> {
    // Check if file exists and we're not forcing (only in non-interactive mode)
    // In interactive mode, we let the user decide
    if !opts.interactive && opts.output_path.exists() && !opts.force {
        let detected = opts
            .project_type_hint
            .map(hint_to_detected)
            .unwrap_or_else(|| detect_project_type(&resolved.repo_root));
        return Ok(InitReport {
            status: FileInitStatus::Valid,
            detected_project_type: detected,
            output_path: opts.output_path,
        });
    }

    // Determine project type (for non-interactive or as default for interactive)
    let detected_type = opts
        .project_type_hint
        .map(hint_to_detected)
        .unwrap_or_else(|| detect_project_type(&resolved.repo_root));

    // Interactive mode: run wizard
    let (project_type, output_path, content) = if opts.interactive {
        if !is_tty() {
            anyhow::bail!("Interactive mode requires a TTY terminal");
        }

        let prompter = wizard::DialoguerPrompter;
        let wizard_result = wizard::run_init_wizard(
            &prompter,
            detected_type_to_hint(detected_type),
            &opts.output_path,
        )
        .context("interactive wizard failed")?;

        let project_type = hint_to_detected(wizard_result.project_type);
        let output_path = wizard_result
            .output_path
            .unwrap_or_else(|| opts.output_path.clone());

        // Generate content with hints
        let content = generate_agents_md_with_hints(
            resolved,
            project_type,
            Some(&wizard_result.config_hints),
        )?;

        // Preview and confirm if requested
        if wizard_result.confirm_write {
            println!("\n{}", "─".repeat(60));
            println!(
                "{}",
                colored::Colorize::bold("Preview of generated AGENTS.md:")
            );
            println!("{}", "─".repeat(60));
            println!("{}", content);
            println!("{}", "─".repeat(60));

            let proceed = prompter
                .confirm("Write this AGENTS.md?", true)
                .context("failed to get confirmation")?;

            if !proceed {
                anyhow::bail!("AGENTS.md creation cancelled by user");
            }
        }

        (project_type, output_path, content)
    } else {
        // Non-interactive mode
        let content = generate_agents_md(resolved, detected_type)?;
        (detected_type, opts.output_path.clone(), content)
    };

    // Write file
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    fsutil::write_atomic(&output_path, content.as_bytes())
        .with_context(|| format!("write AGENTS.md {}", output_path.display()))?;

    Ok(InitReport {
        status: FileInitStatus::Created,
        detected_project_type: project_type,
        output_path,
    })
}

/// Run the context update command
pub fn run_context_update(
    _resolved: &config::Resolved,
    opts: ContextUpdateOptions,
) -> Result<UpdateReport> {
    // Check if file exists
    if !opts.output_path.exists() {
        anyhow::bail!(
            "AGENTS.md does not exist at {}. Run `ralph context init` first.",
            opts.output_path.display()
        );
    }

    // Read existing content
    let existing_content =
        fs::read_to_string(&opts.output_path).context("read existing AGENTS.md")?;

    // Parse existing document
    let existing_doc = merge::parse_markdown_document(&existing_content);
    let existing_sections = existing_doc
        .section_titles()
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

    let mut updates: Vec<(String, String)> = Vec::new();

    // Handle interactive mode
    if opts.interactive {
        if !is_tty() {
            anyhow::bail!("Interactive mode requires a TTY terminal");
        }

        let prompter = wizard::DialoguerPrompter;
        updates = wizard::run_update_wizard(&prompter, &existing_sections, &existing_content)
            .context("interactive wizard failed")?;
    }
    // Handle file-based update
    else if let Some(file_path) = &opts.file {
        let new_content = fs::read_to_string(file_path).context("read update file")?;
        let parsed = parse_markdown_sections(&new_content);

        for (section_name, section_content) in parsed {
            if opts.sections.is_empty() || opts.sections.contains(&section_name) {
                updates.push((section_name, section_content));
            }
        }
    }
    // Handle section-specific updates (non-interactive, no file)
    else {
        anyhow::bail!(
            "No update source specified. Use --interactive, --file, or specify sections with content."
        );
    }

    // If no updates, return early
    if updates.is_empty() {
        return Ok(UpdateReport {
            sections_updated: Vec::new(),
            dry_run: opts.dry_run,
        });
    }

    // Merge updates into existing document
    let (merged_doc, sections_updated) = merge::merge_section_updates(&existing_doc, &updates);

    // If dry run, preview changes without writing
    if opts.dry_run {
        println!("\n{}", "─".repeat(60));
        println!(
            "{}",
            colored::Colorize::bold("Dry run - changes that would be made:")
        );
        println!("{}", "─".repeat(60));
        for section in &sections_updated {
            println!("  • Update section: {}", section);
        }
        println!("{}", "─".repeat(60));
        return Ok(UpdateReport {
            sections_updated,
            dry_run: true,
        });
    }

    // Write merged content back
    let merged_content = merged_doc.to_content();
    fsutil::write_atomic(&opts.output_path, merged_content.as_bytes())
        .with_context(|| format!("write AGENTS.md {}", opts.output_path.display()))?;

    Ok(UpdateReport {
        sections_updated,
        dry_run: false,
    })
}

/// Run the context validate command
pub fn run_context_validate(
    _resolved: &config::Resolved,
    opts: ContextValidateOptions,
) -> Result<ValidateReport> {
    // Check if file exists
    if !opts.path.exists() {
        return Ok(ValidateReport {
            valid: false,
            missing_sections: REQUIRED_SECTIONS.iter().map(|s| s.to_string()).collect(),
            outdated_sections: Vec::new(),
        });
    }

    // Read content
    let content = fs::read_to_string(&opts.path).context("read AGENTS.md")?;

    // Parse sections
    let sections = extract_section_titles(&content);
    let section_set: HashSet<_> = sections.iter().map(|s| s.as_str()).collect();

    // Check for missing required sections
    let missing_sections: Vec<String> = REQUIRED_SECTIONS
        .iter()
        .filter(|s| !section_set.contains(**s))
        .map(|s| s.to_string())
        .collect();

    // In strict mode, also check recommended sections
    let missing_recommended: Vec<String> = if opts.strict {
        RECOMMENDED_SECTIONS
            .iter()
            .filter(|s| !section_set.contains(**s))
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    // Check for outdated template version (if present in file)
    let outdated_sections = Vec::new();

    let valid = missing_sections.is_empty() && (missing_recommended.is_empty() || !opts.strict);

    Ok(ValidateReport {
        valid,
        missing_sections: if opts.strict {
            missing_recommended
        } else {
            missing_sections
        },
        outdated_sections,
    })
}

/// Convert CLI hint to detected type
fn hint_to_detected(hint: ProjectTypeHint) -> DetectedProjectType {
    match hint {
        ProjectTypeHint::Rust => DetectedProjectType::Rust,
        ProjectTypeHint::Python => DetectedProjectType::Python,
        ProjectTypeHint::TypeScript => DetectedProjectType::TypeScript,
        ProjectTypeHint::Go => DetectedProjectType::Go,
        ProjectTypeHint::Generic => DetectedProjectType::Generic,
    }
}

/// Convert detected type to CLI hint
fn detected_type_to_hint(detected: DetectedProjectType) -> ProjectTypeHint {
    match detected {
        DetectedProjectType::Rust => ProjectTypeHint::Rust,
        DetectedProjectType::Python => ProjectTypeHint::Python,
        DetectedProjectType::TypeScript => ProjectTypeHint::TypeScript,
        DetectedProjectType::Go => ProjectTypeHint::Go,
        DetectedProjectType::Generic => ProjectTypeHint::Generic,
    }
}

/// Check if stdin and stdout are both TTYs
fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Detect project type based on files in repo root
fn detect_project_type(repo_root: &Path) -> DetectedProjectType {
    // Check for Rust
    if repo_root.join("Cargo.toml").exists() {
        return DetectedProjectType::Rust;
    }
    // Check for Python
    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("setup.py").exists()
        || repo_root.join("requirements.txt").exists()
    {
        return DetectedProjectType::Python;
    }
    // Check for TypeScript/JavaScript
    if repo_root.join("package.json").exists() {
        return DetectedProjectType::TypeScript;
    }
    // Check for Go
    if repo_root.join("go.mod").exists() {
        return DetectedProjectType::Go;
    }
    DetectedProjectType::Generic
}

/// Generate AGENTS.md content for the given project type
fn generate_agents_md(
    resolved: &config::Resolved,
    project_type: DetectedProjectType,
) -> Result<String> {
    generate_agents_md_with_hints(resolved, project_type, None)
}

/// Generate AGENTS.md content with optional config hints
fn generate_agents_md_with_hints(
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

    // Build repository map
    let repo_map = build_repository_map(resolved)?;

    // Get project name from directory name
    let project_name = resolved
        .repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project")
        .to_string();

    // Get id_prefix from config
    let id_prefix = resolved.id_prefix.clone();

    // Use hints if provided, otherwise defaults
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

    // Replace placeholders
    let content = template
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
        .replace("{template_version}", TEMPLATE_VERSION);

    Ok(content)
}

/// Build a repository map string based on detected structure
fn build_repository_map(resolved: &config::Resolved) -> Result<String> {
    let mut entries = Vec::new();

    // Check for common directories
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

    // Check for key files
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

/// Parse markdown content into sections
fn parse_markdown_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_title = String::new();
    let mut current_content = Vec::new();

    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("## ") {
            // Save previous section if exists
            if !current_title.is_empty() {
                sections.push((current_title, current_content.join("\n")));
            }
            // Start new section
            current_title = stripped.trim().to_string();
            current_content = Vec::new();
        } else if !current_title.is_empty() {
            current_content.push(line.to_string());
        }
    }

    // Save last section
    if !current_title.is_empty() {
        sections.push((current_title, current_content.join("\n")));
    }

    sections
}

/// Extract section titles from markdown content
fn extract_section_titles(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix("## ").map(|s| s.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_resolved(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        config::Resolved {
            config: crate::contracts::Config::default(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
            repo_root,
        }
    }

    #[test]
    fn detect_project_type_finds_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Rust);
    }

    #[test]
    fn detect_project_type_finds_python() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Python);
    }

    #[test]
    fn detect_project_type_finds_typescript() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect_project_type(dir.path()),
            DetectedProjectType::TypeScript
        );
    }

    #[test]
    fn detect_project_type_finds_go() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module test").unwrap();
        assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Go);
    }

    #[test]
    fn detect_project_type_defaults_to_generic() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            detect_project_type(dir.path()),
            DetectedProjectType::Generic
        );
    }

    #[test]
    fn init_creates_agents_md() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);
        fs::create_dir_all(resolved.repo_root.join("src"))?;

        let output_path = resolved.repo_root.join("AGENTS.md");
        let report = run_context_init(
            &resolved,
            ContextInitOptions {
                force: false,
                project_type_hint: None,
                output_path: output_path.clone(),
                interactive: false,
            },
        )?;

        assert_eq!(report.status, FileInitStatus::Created);
        assert!(output_path.exists());

        let content = fs::read_to_string(&output_path)?;
        assert!(content.contains("# Repository Guidelines"));
        assert!(content.contains("Non-Negotiables"));
        assert!(content.contains("Repository Map"));

        Ok(())
    }

    #[test]
    fn init_skips_existing_without_force() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        let output_path = resolved.repo_root.join("AGENTS.md");
        fs::write(&output_path, "existing content")?;

        let report = run_context_init(
            &resolved,
            ContextInitOptions {
                force: false,
                project_type_hint: None,
                output_path: output_path.clone(),
                interactive: false,
            },
        )?;

        assert_eq!(report.status, FileInitStatus::Valid);
        let content = fs::read_to_string(&output_path)?;
        assert_eq!(content, "existing content");

        Ok(())
    }

    #[test]
    fn init_overwrites_with_force() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        let output_path = resolved.repo_root.join("AGENTS.md");
        fs::write(&output_path, "existing content")?;

        let report = run_context_init(
            &resolved,
            ContextInitOptions {
                force: true,
                project_type_hint: None,
                output_path: output_path.clone(),
                interactive: false,
            },
        )?;

        assert_eq!(report.status, FileInitStatus::Created);
        let content = fs::read_to_string(&output_path)?;
        assert!(content.contains("# Repository Guidelines"));

        Ok(())
    }

    #[test]
    fn validate_fails_when_file_missing() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        let report = run_context_validate(
            &resolved,
            ContextValidateOptions {
                strict: false,
                path: resolved.repo_root.join("AGENTS.md"),
            },
        )?;

        assert!(!report.valid);
        assert!(!report.missing_sections.is_empty());

        Ok(())
    }

    #[test]
    fn validate_passes_for_valid_file() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        // Create a valid AGENTS.md
        let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
        fs::write(resolved.repo_root.join("AGENTS.md"), content)?;

        let report = run_context_validate(
            &resolved,
            ContextValidateOptions {
                strict: false,
                path: resolved.repo_root.join("AGENTS.md"),
            },
        )?;

        assert!(report.valid);
        assert!(report.missing_sections.is_empty());

        Ok(())
    }

    #[test]
    fn validate_strict_fails_for_missing_recommended() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        // Create an AGENTS.md missing recommended sections
        let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
        fs::write(resolved.repo_root.join("AGENTS.md"), content)?;

        let report = run_context_validate(
            &resolved,
            ContextValidateOptions {
                strict: true,
                path: resolved.repo_root.join("AGENTS.md"),
            },
        )?;

        // Should fail in strict mode due to missing recommended sections
        assert!(!report.valid);
        assert!(!report.missing_sections.is_empty());

        Ok(())
    }

    #[test]
    fn extract_section_titles_finds_all_sections() {
        let content = r#"# Title

## Section One

Content one.

## Section Two

Content two.

### Subsection

More content.
"#;
        let titles = extract_section_titles(content);
        assert_eq!(titles, vec!["Section One", "Section Two"]);
    }

    #[test]
    fn parse_markdown_sections_extracts_content() {
        let content = r#"# Title

## Section One

Content one.

More content.

## Section Two

Content two.
"#;
        let sections = parse_markdown_sections(content);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, "Section One");
        assert!(sections[0].1.contains("Content one."));
        assert_eq!(sections[1].0, "Section Two");
    }

    #[test]
    fn update_fails_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let resolved = create_test_resolved(&dir);

        let result = run_context_update(
            &resolved,
            ContextUpdateOptions {
                sections: vec!["troubleshooting".to_string()],
                file: None,
                interactive: false,
                dry_run: false,
                output_path: resolved.repo_root.join("AGENTS.md"),
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn update_returns_sections_updated() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = create_test_resolved(&dir);

        // Create initial AGENTS.md with a section to update
        fs::write(
            resolved.repo_root.join("AGENTS.md"),
            "# Repository Guidelines\n\n## Non-Negotiables\n\nRules.\n",
        )?;

        // Create an update file to use for the test
        fs::write(
            resolved.repo_root.join("update.md"),
            "## Non-Negotiables\n\nAdditional rules.\n",
        )?;

        let report = run_context_update(
            &resolved,
            ContextUpdateOptions {
                sections: vec!["Non-Negotiables".to_string()],
                file: Some(resolved.repo_root.join("update.md")),
                interactive: false,
                dry_run: true,
                output_path: resolved.repo_root.join("AGENTS.md"),
            },
        )?;

        assert!(report.dry_run);
        assert!(
            report
                .sections_updated
                .contains(&"Non-Negotiables".to_string())
        );

        Ok(())
    }
}
