//! CLI help output contract tests for the Ralph binary.
//!
//! Purpose:
//! - CLI help output contract tests for the Ralph binary.
//!
//! Responsibilities:
//! - Assert key help text snippets remain present for core commands.
//! - Guard against regression in documented flags and examples.
//!
//! Not handled here:
//! - Full validation of help output formatting.
//! - Behavior tests for command execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The Ralph binary is built and discoverable by the test harness.

use std::process::{Command, ExitStatus};

mod test_support;

fn run(args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(test_support::ralph_bin())
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected output to contain {needle:?}\n--- output ---\n{haystack}\n--- end ---"
    );
}

fn assert_not_contains(haystack: &str, needle: &str) {
    assert!(
        !haystack.contains(needle),
        "expected output not to contain {needle:?}\n--- output ---\n{haystack}\n--- end ---"
    );
}

fn assert_occurs_once(haystack: &str, needle: &str) {
    let count = haystack.matches(needle).count();
    assert_eq!(
        count, 1,
        "expected {needle:?} to appear exactly once, found {count}\n--- output ---\n{haystack}\n--- end ---"
    );
}

#[test]
fn root_help_mentions_runner_and_models_and_precedence() {
    let (status, stdout, stderr) = run(&["--help"]);
    assert!(
        status.success(),
        "expected `ralph --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "Allowed runners:");
    assert_contains(&combined, "codex");
    assert_contains(&combined, "opencode");
    assert_contains(&combined, "gemini");
    assert_contains(&combined, "claude");
    assert_contains(&combined, "cursor");

    assert_contains(&combined, "Allowed models:");
    assert_contains(&combined, "gpt-5.3-codex");
    assert_contains(&combined, "gpt-5.3-codex-spark");
    assert_contains(&combined, "gpt-5.3");
    assert_not_contains(&combined, "gpt-5.2-codex");
    assert_not_contains(&combined, "gpt-5.2");
    assert_contains(&combined, "zai-coding-plan/glm-4.7");
    assert_contains(&combined, "gemini-3-pro-preview");
    assert_contains(&combined, "gemini-3-flash-preview");
    assert_contains(&combined, "sonnet");
    assert_contains(&combined, "opus");
    assert_contains(&combined, "arbitrary model ids");

    assert_contains(&combined, "CLI flags override");
    assert_contains(&combined, "project config");
    assert_contains(&combined, "global config");
}

#[test]
fn run_help_mentions_precedence_and_overrides_exist() {
    let (status, stdout, stderr) = run(&["run", "--help"]);
    assert!(
        status.success(),
        "expected `ralph run --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "Runner selection");
    assert_contains(&combined, "CLI overrides");
    assert_contains(&combined, "task");
    assert_contains(&combined, "config");
    assert_contains(&combined, "Blocking-state diagnosis");
    assert_contains(&combined, "ralph doctor");
    assert_contains(&combined, "ralph machine doctor report");
    assert_contains(&combined, "Examples:");
    assert_contains(&combined, "ralph run one");
    assert_contains(&combined, "ralph run loop");
    assert_contains(&combined, "ralph run resume");
}

#[test]
fn run_one_help_mentions_flags_and_examples() {
    let (status, stdout, stderr) = run(&["run", "one", "--help"]);
    assert!(
        status.success(),
        "expected `ralph run one --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    // Flags must be present on the subcommand help output.
    assert_contains(&combined, "--runner");
    assert_contains(&combined, "--model");
    assert_contains(&combined, "--effort");
    assert_contains(&combined, "--phases");
    assert_contains(&combined, "--repo-prompt");
    assert_contains(&combined, "--id");

    // Examples should demonstrate explicit selection.
    assert_contains(&combined, "ralph run one");
    assert_contains(&combined, "--runner");
    assert_contains(&combined, "ralph run one --id");
    assert_contains(&combined, "Blocking-state diagnosis");
    assert_contains(&combined, "ralph doctor");
}

#[test]
fn queue_recovery_help_mentions_continuation_workflows() {
    let (validate_status, validate_stdout, validate_stderr) = run(&["queue", "validate", "--help"]);
    assert!(
        validate_status.success(),
        "expected `ralph queue validate --help` to succeed\nstdout:\n{validate_stdout}\nstderr:\n{validate_stderr}"
    );
    let validate_combined = format!("{validate_stdout}\n{validate_stderr}");
    assert_contains(&validate_combined, "Continuation workflow");
    assert_contains(&validate_combined, "ralph queue repair --dry-run");
    assert_contains(&validate_combined, "ralph undo --dry-run");

    let (repair_status, repair_stdout, repair_stderr) = run(&["queue", "repair", "--help"]);
    assert!(
        repair_status.success(),
        "expected `ralph queue repair --help` to succeed\nstdout:\n{repair_stdout}\nstderr:\n{repair_stderr}"
    );
    let repair_combined = format!("{repair_stdout}\n{repair_stderr}");
    assert_contains(&repair_combined, "Continuation workflow");
    assert_contains(&repair_combined, "ralph undo --dry-run");

    let (undo_status, undo_stdout, undo_stderr) = run(&["undo", "--help"]);
    assert!(
        undo_status.success(),
        "expected `ralph undo --help` to succeed\nstdout:\n{undo_stdout}\nstderr:\n{undo_stderr}"
    );
    let undo_combined = format!("{undo_stdout}\n{undo_stderr}");
    assert_contains(&undo_combined, "Continuation workflow");
    assert_contains(&undo_combined, "ralph undo --list");
    assert_contains(&undo_combined, "ralph queue validate");
}

#[test]
fn run_loop_help_mentions_blocking_state_diagnosis() {
    let (status, stdout, stderr) = run(&["run", "loop", "--help"]);
    assert!(
        status.success(),
        "expected `ralph run loop --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "Blocking-state diagnosis");
    assert_contains(&combined, "ralph doctor");
    assert_contains(&combined, "wait-when-blocked");
}

#[test]
fn task_mutate_help_mentions_continuation_and_format() {
    let (status, stdout, stderr) = run(&["task", "mutate", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task mutate --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");
    assert_contains(&combined, "Continuation workflow");
    assert_contains(&combined, "--format");
    assert_contains(&combined, "ralph undo --dry-run");
}

#[test]
fn task_build_help_mentions_repo_prompt_flag() {
    let (status, stdout, stderr) = run(&["task", "build", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task build --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "--repo-prompt");
}

#[test]
fn task_help_mentions_default_and_explicit_build() {
    let (status, stdout, stderr) = run(&["task", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "ralph task");
    assert_contains(&combined, "build");
    assert_contains(&combined, "template");
    assert_contains(&combined, "done --note \"Build checks pass\" RQ-0001");
    assert_contains(&combined, "split --number 3 RQ-0001");
}

#[test]
fn task_help_shows_group_headings() {
    let (status, stdout, stderr) = run(&["task", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_occurs_once(
        &combined,
        "Create and build: task, build, refactor, build-refactor",
    );
    assert_occurs_once(
        &combined,
        "Lifecycle: show, ready, status, done, reject, start, schedule",
    );
    assert_occurs_once(&combined, "Edit: field, edit, update");
    assert_occurs_once(
        &combined,
        "Relationships: clone, split, relate, blocks, mark-duplicate, children, parent",
    );
    assert_occurs_once(&combined, "Batch and templates: batch, template");
}

#[test]
fn task_help_shows_common_journeys() {
    let (status, stdout, stderr) = run(&["task", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "Common journeys:");
    assert_contains(&combined, "Create a task:");
    assert_contains(&combined, "Start work on a task:");
    assert_contains(&combined, "Complete a task:");
    assert_contains(&combined, "Split a task:");
}

#[test]
fn task_show_help_mentions_examples() {
    let (status, stdout, stderr) = run(&["task", "show", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task show --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "ralph task show RQ-0001");
    assert_contains(&combined, "--format");
    assert_contains(&combined, "compact");
}

#[test]
fn scan_help_mentions_repo_prompt_flag() {
    let (status, stdout, stderr) = run(&["scan", "--help"]);
    assert!(
        status.success(),
        "expected `ralph scan --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "--repo-prompt");
}

#[test]
fn config_show_help_mentions_format_and_examples() {
    let (status, stdout, stderr) = run(&["config", "show", "--help"]);
    assert!(
        status.success(),
        "expected `ralph config show --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "--format");
    assert_contains(&combined, "json");
    assert_contains(&combined, "yaml");
    assert_contains(&combined, "ralph config show --format json");
}

#[test]
fn daemon_help_mentions_subcommands() {
    let (status, stdout, stderr) = run(&["daemon", "--help"]);
    assert!(
        status.success(),
        "expected `ralph daemon --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "start");
    assert_contains(&combined, "stop");
    assert_contains(&combined, "status");
    assert_contains(&combined, "logs");
}

#[test]
fn config_examples_from_docs_execute_successfully() {
    use std::process::Command;

    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Initialize a git repo in the temp directory
    let git_init = Command::new("git")
        .args(["init"])
        .current_dir(&temp_dir)
        .output()
        .expect("failed to run git init");
    assert!(
        git_init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&git_init.stderr)
    );

    // Configure git user for the temp repo
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&temp_dir)
        .output()
        .expect("failed to set git email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&temp_dir)
        .output()
        .expect("failed to set git name");

    // Run ralph init
    let ralph_init = Command::new(test_support::ralph_bin())
        .args(["init", "--non-interactive"])
        .current_dir(&temp_dir)
        .output()
        .expect("failed to run ralph init");
    assert!(
        ralph_init.status.success(),
        "ralph init failed: {}",
        String::from_utf8_lossy(&ralph_init.stderr)
    );

    let commands: Vec<Vec<&str>> = vec![
        vec!["config", "show"],
        vec!["config", "show", "--format", "json"],
        vec!["config", "paths"],
        vec!["config", "schema"],
        vec!["config", "profiles", "list"],
    ];

    for args in &commands {
        let output = Command::new(test_support::ralph_bin())
            .args(args)
            .current_dir(&temp_dir)
            .output()
            .unwrap_or_else(|_| panic!("failed to execute ralph {}", args.join(" ")));

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            output.status.success(),
            "expected `ralph {}` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}",
            args.join(" ")
        );
    }
}
