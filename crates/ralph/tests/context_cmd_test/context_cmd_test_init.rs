//! `ralph context init` integration tests.
//!
//! Purpose:
//! - `ralph context init` integration tests.
//!
//! Responsibilities:
//! - Cover AGENTS.md creation and generated section expectations.
//! - Verify project-type detection and explicit hint behavior.
//! - Verify file overwrite and custom output-path semantics.
//!
//! Not handled here:
//! - `context validate` or `context update` behaviors.
//! - Interactive flows requiring a TTY.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Each test starts from a fresh temp repo.
//! - Generated files are asserted through on-disk content only.

use anyhow::Result;
use std::fs;

use super::context_cmd_test_support::{run_in_dir, setup_repo};

#[test]
fn context_init_creates_agents_md() -> Result<()> {
    let dir = setup_repo()?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
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

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let agents_md = dir.path().join("AGENTS.md");
    anyhow::ensure!(agents_md.exists(), "AGENTS.md context file was not created");

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
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"",
    )?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(content.contains("Cargo"), "Rust-specific content missing");
    Ok(())
}

#[test]
fn context_init_detects_python_project() -> Result<()> {
    let dir = setup_repo()?;
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"test-project\"",
    )?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("pip") || content.contains("python") || content.contains("pytest"),
        "Python-specific content missing"
    );
    Ok(())
}

#[test]
fn context_init_detects_typescript_project() -> Result<()> {
    let dir = setup_repo()?;
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test-project"}"#,
    )?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("npm") || content.contains("node") || content.contains("package"),
        "TypeScript-specific content missing"
    );
    Ok(())
}

#[test]
fn context_init_detects_go_project() -> Result<()> {
    let dir = setup_repo()?;
    fs::write(dir.path().join("go.mod"), "module test-project\n\ngo 1.21")?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(status.success(), "context init failed\nstderr:\n{stderr}");

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("go ") || content.contains("Go "),
        "Go-specific content missing"
    );
    Ok(())
}

#[test]
fn context_init_respects_force_flag() -> Result<()> {
    let dir = setup_repo()?;
    let initial_content = "# Custom AGENTS.md\n\nThis is custom content.";
    fs::write(dir.path().join("AGENTS.md"), initial_content)?;

    let (status, _stdout, _stderr) = run_in_dir(dir.path(), &["context", "init"]);
    anyhow::ensure!(
        status.success(),
        "context init should succeed when file exists"
    );

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content == initial_content,
        "content should be preserved without force"
    );

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["context", "init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "context init --force failed\nstderr:\n{stderr}"
    );

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

    let (status, _stdout, stderr) = run_in_dir(
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

    let (status, _stdout, stderr) =
        run_in_dir(dir.path(), &["context", "init", "--project-type", "rust"]);
    anyhow::ensure!(
        status.success(),
        "context init --project-type failed\nstderr:\n{stderr}"
    );

    let content = fs::read_to_string(dir.path().join("AGENTS.md"))?;
    anyhow::ensure!(
        content.contains("cargo") || content.contains("rust") || content.contains("clippy"),
        "Rust-specific content missing"
    );

    Ok(())
}
