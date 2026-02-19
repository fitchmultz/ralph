//! Integration tests for `ralph context` commands.
//!
//! Responsibilities:
//! - Test context init, update, and validate subcommands via CLI
//! - Verify AGENTS.md generation with different project types
//! - Test force, dry-run, and path options
//!
//! Not handled here:
//! - Interactive wizard testing (requires TTY)
//! - Template content validation (covered by unit tests in commands/context.rs)
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp directories via test_support::temp_dir_outside_repo()
//! - Git is available on PATH

use anyhow::Result;
use std::fs;

mod test_support;

fn setup_repo() -> Result<tempfile::TempDir> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;
    Ok(dir)
}

// =============================================================================
// Context Init Tests
// =============================================================================

#[test]
fn context_init_creates_agents_md() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    anyhow::ensure!(agents_md.exists(), "AGENTS.md was not created");

    let content = fs::read_to_string(&agents_md)?;
    anyhow::ensure!(content.contains("# Repository Guidelines"), "missing title");
    anyhow::ensure!(
        content.contains("Non-Negotiables"),
        "missing non-negotiables section"
    );
    anyhow::ensure!(
        content.contains("Repository Map"),
        "missing repository map section"
    );
    anyhow::ensure!(
        content.contains("Build, Test, and CI"),
        "missing build/test/ci section"
    );

    Ok(())
}

#[test]
fn context_init_creates_context_files() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    // Verify AGENTS.md context file was created
    let agents_md = dir.path().join("AGENTS.md");
    anyhow::ensure!(agents_md.exists(), "AGENTS.md context file was not created");

    // Verify it contains expected context sections
    let content = fs::read_to_string(&agents_md)?;
    anyhow::ensure!(
        content.contains("# Repository Guidelines"),
        "missing title in context file"
    );
    anyhow::ensure!(
        content.contains("## Non-Negotiables"),
        "missing non-negotiables section"
    );
    anyhow::ensure!(
        content.contains("## Repository Map"),
        "missing repository map section"
    );
    anyhow::ensure!(
        content.contains("## Build, Test, and CI"),
        "missing build/test/ci section"
    );

    Ok(())
}

#[test]
fn context_init_detects_rust_project() -> Result<()> {
    let dir = setup_repo()?;

    // Create Cargo.toml to make it a Rust project
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"",
    )?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    let content = fs::read_to_string(&agents_md)?;

    // Rust template should mention Cargo
    anyhow::ensure!(content.contains("Cargo"), "Rust-specific content missing");

    Ok(())
}

#[test]
fn context_init_detects_python_project() -> Result<()> {
    let dir = setup_repo()?;

    // Create pyproject.toml to make it a Python project
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"test-project\"",
    )?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    let content = fs::read_to_string(&agents_md)?;

    // Python template should mention pip or python
    anyhow::ensure!(
        content.contains("pip") || content.contains("python") || content.contains("pytest"),
        "Python-specific content missing"
    );

    Ok(())
}

#[test]
fn context_init_detects_typescript_project() -> Result<()> {
    let dir = setup_repo()?;

    // Create package.json to make it a TypeScript/JavaScript project
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test-project"}"#,
    )?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    let content = fs::read_to_string(&agents_md)?;

    // TypeScript template should mention npm or node
    anyhow::ensure!(
        content.contains("npm") || content.contains("node") || content.contains("package"),
        "TypeScript-specific content missing"
    );

    Ok(())
}

#[test]
fn context_init_detects_go_project() -> Result<()> {
    let dir = setup_repo()?;

    // Create go.mod to make it a Go project
    fs::write(dir.path().join("go.mod"), "module test-project\n\ngo 1.21")?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);

    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    let content = fs::read_to_string(&agents_md)?;

    // Go template should mention Go-specific content
    anyhow::ensure!(
        content.contains("go ") || content.contains("Go "),
        "Go-specific content missing"
    );

    Ok(())
}

#[test]
fn context_init_respects_force_flag() -> Result<()> {
    let dir = setup_repo()?;

    // Create initial AGENTS.md with custom content
    let initial_content = "# Custom AGENTS.md\n\nThis is custom content.";
    fs::write(dir.path().join("AGENTS.md"), initial_content)?;

    // Run init without force - should preserve existing
    let (status, _stdout, _stderr) = test_support::run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(
        status.success(),
        "context init should succeed when file exists"
    );

    // Verify content preserved
    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content == initial_content,
        "content should be preserved without force"
    );

    // Run init with force - should overwrite
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["context", "init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "context init --force failed\nstderr:\n{stderr}"
    );

    // Verify content was overwritten
    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("# Repository Guidelines"),
        "content should be overwritten with force"
    );

    Ok(())
}

#[test]
fn context_init_respects_output_path() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["context", "init", "--output", "docs/AGENTS.md"],
    );

    anyhow::ensure!(
        status.success(),
        "context init --output failed\nstderr:\n{stderr}"
    );

    let custom_path = dir.path().join("docs/AGENTS.md");
    anyhow::ensure!(
        custom_path.exists(),
        "AGENTS.md was not created at custom path"
    );

    let content = fs::read_to_string(&custom_path)?;
    anyhow::ensure!(content.contains("# Repository Guidelines"), "missing title");

    Ok(())
}

#[test]
fn context_init_respects_project_type_hint() -> Result<()> {
    let dir = setup_repo()?;

    // Even without Cargo.toml, should generate Rust template with hint
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["context", "init", "--project-type", "rust"]);

    anyhow::ensure!(
        status.success(),
        "context init --project-type failed\nstderr:\n{stderr}"
    );

    let agents_md = dir.path().join("AGENTS.md");
    let content = fs::read_to_string(&agents_md)?;

    // Should contain Rust-specific content even without Cargo.toml
    anyhow::ensure!(
        content.contains("cargo") || content.contains("rust") || content.contains("clippy"),
        "Rust-specific content missing"
    );

    Ok(())
}

// =============================================================================
// Context Validate Tests
// =============================================================================

#[test]
fn context_validate_fails_when_file_missing() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);

    anyhow::ensure!(!status.success(), "should fail when AGENTS.md missing");
    anyhow::ensure!(
        stderr.contains("Validation failed")
            || stderr.contains("missing")
            || stderr.contains("not found"),
        "should report validation failure in stderr, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_passes_for_valid_file() -> Result<()> {
    let dir = setup_repo()?;

    // Create a valid AGENTS.md with all required sections
    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);

    anyhow::ensure!(
        status.success(),
        "validation should pass\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("valid") || stderr.contains("valid"),
        "should report validity"
    );

    Ok(())
}

#[test]
fn context_validate_checks_context() -> Result<()> {
    let dir = setup_repo()?;

    // Create AGENTS.md with valid context structure
    let content = r#"# Repository Guidelines

Test project context.

## Non-Negotiables

- Rule 1: Do not commit secrets
- Rule 2: Run tests before pushing

## Repository Map

- `src/`: Source code
- `tests/`: Test files

## Build, Test, and CI

Run `make ci` to verify everything.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);

    anyhow::ensure!(
        status.success(),
        "context validate should pass for valid context file\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("valid") || stderr.contains("valid"),
        "should report context is valid"
    );

    Ok(())
}

#[test]
fn context_validate_fails_for_missing_required_sections() -> Result<()> {
    let dir = setup_repo()?;

    // Create AGENTS.md missing required sections (missing Repository Map)
    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);

    anyhow::ensure!(
        !status.success(),
        "should fail with missing required sections"
    );
    anyhow::ensure!(
        stderr.contains("Validation failed") || stderr.contains("Missing sections"),
        "should report missing sections, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_strict_fails_for_missing_recommended() -> Result<()> {
    let dir = setup_repo()?;

    // Create AGENTS.md with required sections but missing some recommended ones
    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    // Non-strict mode should pass
    let (status, _stdout, _stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);
    anyhow::ensure!(status.success(), "non-strict validation should pass");

    // Strict mode should fail due to missing recommended sections
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["context", "validate", "--strict"]);

    anyhow::ensure!(
        !status.success(),
        "strict validation should fail with missing recommended sections"
    );
    anyhow::ensure!(
        stderr.contains("Validation failed") || stderr.contains("Missing sections"),
        "should report missing sections in strict mode, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_respects_custom_path() -> Result<()> {
    let dir = setup_repo()?;

    // Create directory structure
    fs::create_dir_all(dir.path().join("docs"))?;

    // Create a valid AGENTS.md at custom path
    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Some rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("docs/AGENTS.md"), content)?;

    // Validate custom path
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["context", "validate", "--path", "docs/AGENTS.md"],
    );

    anyhow::ensure!(
        status.success(),
        "validation should pass for custom path\nstderr:\n{stderr}"
    );

    Ok(())
}

// =============================================================================
// Context Update Tests
// =============================================================================

#[test]
fn context_update_fails_when_file_missing() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["context", "update", "--section", "test"]);

    anyhow::ensure!(!status.success(), "should fail when AGENTS.md missing");
    anyhow::ensure!(
        stderr.contains("does not exist") || stderr.contains("not found"),
        "should report missing file, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_update_with_file_succeeds() -> Result<()> {
    let dir = setup_repo()?;

    // Create initial AGENTS.md
    let initial_content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("AGENTS.md"), initial_content)?;

    // Create update file
    let update_content = r#"## Non-Negotiables

Updated rules with new information.
"#;
    fs::write(dir.path().join("update.md"), update_content)?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["context", "update", "--file", "update.md"]);

    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    // Verify section was updated
    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated rules"),
        "section should be updated, got:\n{content}"
    );

    Ok(())
}

#[test]
fn context_update_dry_run_does_not_modify() -> Result<()> {
    let dir = setup_repo()?;

    // Create initial AGENTS.md
    let initial_content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("AGENTS.md"), initial_content)?;

    // Create update file
    let update_content = r#"## Non-Negotiables

Updated rules with new information.
"#;
    fs::write(dir.path().join("update.md"), update_content)?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["context", "update", "--file", "update.md", "--dry-run"],
    );

    anyhow::ensure!(
        status.success(),
        "dry-run should succeed\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("Dry run") || stderr.contains("Dry run"),
        "should indicate dry run mode"
    );

    // Verify original content unchanged
    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Original rules"),
        "original content should be preserved in dry run, got:\n{content}"
    );
    anyhow::ensure!(
        !content.contains("Updated rules"),
        "content should not be updated in dry run"
    );

    Ok(())
}

#[test]
fn context_update_with_section_filter() -> Result<()> {
    let dir = setup_repo()?;

    // Create initial AGENTS.md with multiple sections
    let initial_content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original non-negotiables.

## Repository Map

Original repository map.

## Build, Test, and CI

Original build info.
"#;
    fs::write(dir.path().join("AGENTS.md"), initial_content)?;

    // Create update file with multiple sections
    let update_content = r#"## Non-Negotiables

Updated non-negotiables.

## Repository Map

Updated repository map.
"#;
    fs::write(dir.path().join("update.md"), update_content)?;

    // Update only Non-Negotiables section
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "context",
            "update",
            "--file",
            "update.md",
            "--section",
            "Non-Negotiables",
        ],
    );

    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    // Verify only Non-Negotiables was updated
    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated non-negotiables"),
        "Non-Negotiables should be updated"
    );
    anyhow::ensure!(
        content.contains("Original repository map"),
        "Repository Map should not be updated"
    );

    Ok(())
}

#[test]
fn context_update_respects_output_path() -> Result<()> {
    let dir = setup_repo()?;

    // Create directory structure
    fs::create_dir_all(dir.path().join("docs"))?;

    // Create initial AGENTS.md at custom path
    let initial_content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Original rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("docs/AGENTS.md"), initial_content)?;

    // Create update file
    let update_content = r#"## Non-Negotiables

Updated rules.
"#;
    fs::write(dir.path().join("update.md"), update_content)?;

    // Update at custom path
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "context",
            "update",
            "--file",
            "update.md",
            "--output",
            "docs/AGENTS.md",
        ],
    );

    anyhow::ensure!(status.success(), "update should succeed\nstderr:\n{stderr}");

    // Verify section was updated at custom path
    let content = fs::read_to_string(dir.path().join("docs/AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("Updated rules"),
        "section should be updated at custom path"
    );

    Ok(())
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn context_update_fails_without_source() -> Result<()> {
    let dir = setup_repo()?;

    // Create initial AGENTS.md
    let content = r#"# Repository Guidelines

Test project.

## Non-Negotiables

Rules.

## Repository Map

- `src/`: Source code

## Build, Test, and CI

Make targets.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    // Try to update without --file or --interactive
    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "update"]);

    anyhow::ensure!(!status.success(), "should fail without update source");
    anyhow::ensure!(
        stderr.contains("No update source")
            || stderr.contains("--file")
            || stderr.contains("--interactive"),
        "should report missing update source, got: {stderr}"
    );

    Ok(())
}

#[test]
fn context_validate_reports_missing_sections_in_stderr() -> Result<()> {
    let dir = setup_repo()?;

    // Create AGENTS.md missing all required sections
    let content = r#"# Repository Guidelines

Test project with no sections.
"#;
    fs::write(dir.path().join("AGENTS.md"), content)?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["context", "validate"]);

    anyhow::ensure!(!status.success(), "should fail with missing sections");
    anyhow::ensure!(
        stderr.contains("Non-Negotiables")
            || stderr.contains("Repository Map")
            || stderr.contains("Build, Test, and CI")
            || stderr.contains("Missing sections"),
        "should report which sections are missing, got: {stderr}"
    );

    Ok(())
}
