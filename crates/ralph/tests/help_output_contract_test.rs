use std::path::PathBuf;
use std::process::{Command, ExitStatus};

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn run(args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
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

    assert_contains(&combined, "Allowed models:");
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
    assert_contains(&combined, "--rp-on");
    assert_contains(&combined, "--rp-off");

    // Examples should demonstrate explicit selection.
    assert_contains(&combined, "ralph run one");
    assert_contains(&combined, "--runner");
}

#[test]
fn task_build_help_mentions_rp_flags() {
    let (status, stdout, stderr) = run(&["task", "build", "--help"]);
    assert!(
        status.success(),
        "expected `ralph task build --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "--rp-on");
    assert_contains(&combined, "--rp-off");
}

#[test]
fn scan_help_mentions_rp_flags() {
    let (status, stdout, stderr) = run(&["scan", "--help"]);
    assert!(
        status.success(),
        "expected `ralph scan --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    assert_contains(&combined, "--rp-on");
    assert_contains(&combined, "--rp-off");
}
