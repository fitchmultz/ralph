//! Integration tests for `ralph task template` subcommands (list, show, build).
//!
//! Purpose:
//! - Integration tests for `ralph task template` subcommands (list, show, build).
//!
//! Responsibilities:
//! - Test template list command shows built-in and custom templates.
//! - Test template show command displays template details correctly.
//! - Test template build command argument parsing and validation.
//! - Test error handling for invalid template names.
//!
//! Not handled here:
//! - Full task creation via build command (requires AI runner, see unit tests).
//! - Template variable substitution (see task_from_template.rs).
//! - Template loading internals (see template::loader tests).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests run in isolated temp directories.
//! - `seed_ralph_dir()` provides the baseline `.ralph/` fixture for setup.

use anyhow::Result;

mod test_support;

#[test]
fn task_template_list_shows_builtins() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "list"]);

    anyhow::ensure!(status.success(), "list failed: {}", stderr);
    anyhow::ensure!(
        stdout.contains("Available task templates:"),
        "header missing"
    );

    // Verify all 10 built-in templates appear
    let builtins = [
        "bug",
        "feature",
        "refactor",
        "test",
        "docs",
        "add-tests",
        "refactor-performance",
        "fix-error-handling",
        "add-docs",
        "security-audit",
    ];

    for template in &builtins {
        anyhow::ensure!(
            stdout.contains(&format!("{:<12}", template)),
            "template '{}' missing in list",
            template
        );
    }

    // Verify source labels appear
    anyhow::ensure!(stdout.contains("(built-in)"), "built-in label missing");

    // Verify footer help text
    anyhow::ensure!(
        stdout.contains("ralph task template show"),
        "show help text missing"
    );
    anyhow::ensure!(
        stdout.contains("ralph task template build"),
        "build help text missing"
    );

    Ok(())
}

#[test]
fn task_template_show_invalid_template_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create a custom template with invalid JSON
    let templates_dir = dir.path().join(".ralph/templates");
    std::fs::create_dir_all(&templates_dir)?;
    let invalid_template =
        r#"{"id": "", "title": "", "status": "todo", "priority": "high", "plan": [}"#; // Missing closing bracket
    std::fs::write(templates_dir.join("invalid.json"), invalid_template)?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "show", "invalid"]);

    anyhow::ensure!(
        !status.success(),
        "expected failure for invalid template JSON"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("json") || stderr.to_lowercase().contains("parse"),
        "expected JSON/parsing error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn task_template_show_displays_template() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "show", "bug"]);

    anyhow::ensure!(status.success(), "show failed: {}", stderr);
    anyhow::ensure!(
        stdout.contains("Template: bug"),
        "template name missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("built-in"),
        "source label missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("Priority:"),
        "priority field missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("Status:"),
        "status field missing: {}",
        stdout
    );
    anyhow::ensure!(stdout.contains("Plan:"), "plan section missing: {}", stdout);

    // Bug template should have specific content
    anyhow::ensure!(stdout.contains("Tags:"), "tags field missing: {}", stdout);

    Ok(())
}

#[test]
fn task_template_show_custom_template() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Create custom template
    let templates_dir = dir.path().join(".ralph/templates");
    std::fs::create_dir_all(&templates_dir)?;
    let custom_template = r#"{
        "id": "",
        "title": "",
        "status": "todo",
        "priority": "critical",
        "tags": ["custom", "urgent"],
        "scope": ["src/custom.rs"],
        "plan": ["Step 1", "Step 2", "Step 3"],
        "evidence": ["Custom evidence"]
    }"#;
    std::fs::write(templates_dir.join("mytemplate.json"), custom_template)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "show", "mytemplate"]);

    anyhow::ensure!(status.success(), "show failed: {}", stderr);
    anyhow::ensure!(
        stdout.contains("Template: mytemplate"),
        "template name missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("custom"),
        "source label missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("critical"),
        "priority value missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("custom, urgent"),
        "tags missing: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("src/custom.rs"),
        "scope missing: {}",
        stdout
    );

    Ok(())
}

#[test]
fn task_template_show_nonexistent_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "show", "nonexistent"]);

    anyhow::ensure!(
        !status.success(),
        "expected failure for nonexistent template"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("not found"),
        "expected 'not found' error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn task_template_build_missing_request_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Build without providing a request should fail
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "build", "bug"]);

    anyhow::ensure!(
        !status.success(),
        "expected failure when request is missing"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("missing request"),
        "expected 'missing request' error, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn task_template_build_with_empty_request_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Build with only whitespace request should fail
    let (status, _stdout, _stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "build", "bug", "   "]);

    anyhow::ensure!(
        !status.success(),
        "expected failure when request is empty/whitespace"
    );

    Ok(())
}

#[test]
fn task_template_help_shows_examples() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Test list --help
    let (status, _stdout, _stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "list", "--help"]);
    anyhow::ensure!(status.success(), "list --help failed");
    // List has no additional args, just verify it works

    // Test show --help
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "show", "--help"]);
    anyhow::ensure!(status.success(), "show --help failed: {}", stderr);
    anyhow::ensure!(
        stdout.contains("<NAME>"),
        "show help should mention NAME arg: {}",
        stdout
    );

    // Test build --help
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "build", "--help"]);
    anyhow::ensure!(status.success(), "build --help failed: {}", stderr);
    anyhow::ensure!(
        stdout.contains("--tags"),
        "build help should document --tags: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("--scope"),
        "build help should document --scope: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("--runner"),
        "build help should document --runner: {}",
        stdout
    );
    anyhow::ensure!(
        stdout.contains("--model"),
        "build help should document --model: {}",
        stdout
    );

    Ok(())
}

#[test]
fn task_template_all_builtin_templates_listed() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "template", "list"]);

    anyhow::ensure!(status.success(), "list failed: {}", stderr);

    // Verify all 10 built-in templates are listed
    let expected_templates = [
        ("add-docs", "documentation"),
        ("add-tests", "tests"),
        ("bug", "Bug fix"),
        ("docs", "documentation"),
        ("feature", "feature"),
        ("fix-error-handling", "error"),
        ("refactor", "refactoring"),
        ("refactor-performance", "performance"),
        ("security-audit", "Security"),
        ("test", "test"),
    ];

    for (name, _description_part) in &expected_templates {
        anyhow::ensure!(
            stdout.contains(&format!("{:<12}", name)),
            "template '{}' not found in list",
            name
        );
    }

    Ok(())
}

#[test]
fn task_template_show_all_builtins_succeeds() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let templates = [
        "bug",
        "feature",
        "refactor",
        "test",
        "docs",
        "add-tests",
        "refactor-performance",
        "fix-error-handling",
        "add-docs",
        "security-audit",
    ];

    for template in &templates {
        let (status, stdout, stderr) =
            test_support::run_in_dir(dir.path(), &["task", "template", "show", template]);

        anyhow::ensure!(
            status.success(),
            "show {} failed: {}\nstdout: {}",
            template,
            stderr,
            stdout
        );
        anyhow::ensure!(
            stdout.contains(&format!("Template: {}", template)),
            "template name not shown for {}",
            template
        );
    }

    Ok(())
}
