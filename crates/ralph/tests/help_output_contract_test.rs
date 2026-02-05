//! CLI help output contract tests for the Ralph binary.
//!
//! Responsibilities:
//! - Assert key help text snippets remain present for core commands.
//! - Guard against regression in documented flags and examples.
//!
//! Not handled here:
//! - Full validation of help output formatting.
//! - Behavior tests for command execution.
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
    assert_contains(&combined, "gpt-5.3");
    assert_contains(&combined, "gpt-5.2-codex");
    assert_contains(&combined, "gpt-5.2");
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
    assert_contains(&combined, "ralph tui");
    assert_contains(&combined, "ralph run one -i");
    assert_contains(&combined, "ralph run loop -i");
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
