//! Integration tests for `ralph task from template` command.
//!
//! Purpose:
//! - Integration tests for `ralph task from template` command.
//!
//! Responsibilities:
//! - Test CLI argument validation and dry-run mode.
//! - Verify error handling for invalid templates.
//! - Test help output and command availability.
//!
//! Not handled here:
//! - Full task creation flow (requires AI runner, see unit tests).
//! - Template loading logic (see `template::loader` tests).
//! - Variable substitution details (see `template::variables` tests).
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
fn from_template_dry_run_succeeds() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "from",
            "template",
            "bug",
            "--title",
            "Dry run test",
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "from template dry-run failed\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("Would create task"),
        "expected 'Would create task' in stdout, got:\n{stdout}"
    );
    anyhow::ensure!(
        stdout.contains("Dry run - no task created"),
        "expected 'Dry run - no task created' in stdout"
    );

    // Verify task was NOT created
    let queue = test_support::read_queue(dir.path())?;
    anyhow::ensure!(
        !queue.tasks.iter().any(|t| t.title == "Dry run test"),
        "expected no task with title 'Dry run test'"
    );

    Ok(())
}

#[test]
fn from_template_invalid_template_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["task", "from", "template", "nonexistent", "--title", "Test"],
    );
    anyhow::ensure!(
        !status.success(),
        "expected failure for nonexistent template"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("not found"),
        "expected 'not found' in stderr, got:\n{stderr}"
    );

    Ok(())
}

#[test]
fn from_template_dry_run_with_target_shows_target() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "from",
            "template",
            "feature",
            "--title",
            "Add dark mode",
            "--set",
            "target=src/ui/theme.rs",
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "from template with target failed\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("Target: src/ui/theme.rs"),
        "expected 'Target: src/ui/theme.rs' in stdout, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn from_template_dry_run_with_tags_shows_tags() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "from",
            "template",
            "bug",
            "--title",
            "Fix with custom tags",
            "--tags",
            "urgent,critical",
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "from template with tags failed\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("Additional tags: urgent,critical"),
        "expected 'Additional tags: urgent,critical' in stdout, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn from_template_help_shows_examples() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["task", "from", "template", "--help"]);
    anyhow::ensure!(
        status.success(),
        "from template --help failed\nstderr:\n{stderr}"
    );
    // Help should include examples
    anyhow::ensure!(
        stdout.contains("Examples:"),
        "expected 'Examples:' in help output"
    );
    anyhow::ensure!(
        stdout.contains("ralph task from template bug"),
        "expected example command in help output"
    );
    // Help should document template variables
    anyhow::ensure!(
        stdout.contains("{{target}}"),
        "expected '{{target}}' variable documented in help"
    );
    anyhow::ensure!(
        stdout.contains("--set"),
        "expected '--set' option documented in help"
    );
    anyhow::ensure!(
        stdout.contains("--title"),
        "expected '--title' option documented in help"
    );

    Ok(())
}

#[test]
fn from_template_missing_title_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Running without --title should fail
    let (status, _stdout, _stderr) =
        test_support::run_in_dir(dir.path(), &["task", "from", "template", "bug"]);
    // This should fail because --title is required
    anyhow::ensure!(!status.success(), "expected failure when title is missing");

    Ok(())
}

#[test]
fn from_template_all_builtin_templates_dry_run() -> Result<()> {
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
        let (status, _stdout, stderr) = test_support::run_in_dir(
            dir.path(),
            &[
                "task",
                "from",
                "template",
                template,
                "--title",
                &format!("Test {}", template),
                "--dry-run",
            ],
        );
        anyhow::ensure!(
            status.success(),
            "from template '{}' dry-run failed\nstderr:\n{stderr}",
            template
        );
    }

    Ok(())
}

#[test]
fn from_template_invalid_set_format_shows_error() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Invalid --set format (missing =)
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "from",
            "template",
            "bug",
            "--title",
            "Test",
            "--set",
            "invalidformat",
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        !status.success(),
        "expected failure for invalid --set format"
    );
    anyhow::ensure!(
        stderr.contains("Invalid --set format"),
        "expected 'Invalid --set format' in stderr, got:\n{stderr}"
    );

    Ok(())
}

#[test]
fn from_template_with_multiple_set_vars() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

    // Multiple --set args should work
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "task",
            "from",
            "template",
            "bug",
            "--title",
            "Test multi var",
            "--set",
            "target=src/main.rs",
            "--set",
            "component=auth",
            "--dry-run",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "from template with multiple --set failed\nstderr:\n{stderr}"
    );
    // Target should be shown in output
    anyhow::ensure!(
        stdout.contains("Target: src/main.rs"),
        "expected 'Target: src/main.rs' in stdout, got:\n{stdout}"
    );

    Ok(())
}
