//! Tests for runutil helpers and runner error handling.
//!
//! Responsibilities:
//! - Validate revert prompt parsing, formatting, and output ordering.
//! - Exercise runner error handling utilities with controlled inputs.
//!
//! Not handled here:
//! - Integration with real runner binaries or network calls.
//! - CLI/TUI rendering behavior beyond prompt IO surfaces.
//!
//! Invariants/assumptions:
//! - Tests run against isolated temp git repos.
//! - Prompt IO is deterministic for provided inputs.

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::git;
use crate::runner;
use crate::runutil::{
    RevertDecision, RevertOutcome, RevertPromptContext, RevertPromptHandler, RevertSource,
    RunAbortReason, RunnerBackend, RunnerErrorMessages, RunnerInvocation, abort_reason,
    apply_git_revert_mode, apply_git_revert_mode_with_context, parse_revert_response,
    prompt_revert_choice_with_io, run_prompt_with_handling_backend,
};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn init_git_repo(dir: &TempDir) {
    Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(dir.path())
        .output()
        .expect("git init failed");

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .expect("git config user.name failed");
}

fn commit_file(dir: &TempDir, filename: &str, content: &str, message: &str) {
    let file_path = dir.path().join(filename);
    fs::write(&file_path, content).expect("write file");

    Command::new("git")
        .args(["add", filename])
        .current_dir(dir.path())
        .output()
        .expect("git add failed");

    Command::new("git")
        .args(["commit", "--quiet", "-m", message])
        .current_dir(dir.path())
        .output()
        .expect("git commit failed");
}

fn decide_ask(stdin_is_terminal: bool, input: Option<&str>) -> RevertDecision {
    if !stdin_is_terminal {
        return RevertDecision::Keep;
    }
    parse_revert_response(input.unwrap_or(""), false)
}

#[test]
fn ask_mode_defaults_to_keep_when_non_interactive() {
    assert_eq!(decide_ask(false, Some("1")), RevertDecision::Keep);
}

#[test]
fn parse_revert_response_accepts_expected_inputs() {
    assert_eq!(parse_revert_response("", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("1", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("keep", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("2", false), RevertDecision::Revert);
    assert_eq!(parse_revert_response("r", false), RevertDecision::Revert);
    assert_eq!(
        parse_revert_response("revert", false),
        RevertDecision::Revert
    );
    assert_eq!(
        parse_revert_response("3", false),
        RevertDecision::Continue {
            message: String::new()
        }
    );
    assert_eq!(
        parse_revert_response("answer that", false),
        RevertDecision::Continue {
            message: "answer that".to_string()
        }
    );
}

#[test]
fn parse_revert_response_allows_proceed_when_enabled() {
    assert_eq!(parse_revert_response("4", true), RevertDecision::Proceed);
    assert_eq!(
        parse_revert_response("4", false),
        RevertDecision::Continue {
            message: "4".to_string()
        }
    );
}

fn prompt_with_preface(input: &str) -> (RevertDecision, String) {
    let context = RevertPromptContext::new("Scan validation failure", false).with_preface(
        "Scan validation failed after run.\n(raw stdout saved to /tmp/output.txt)\nDetails",
    );
    let mut reader = std::io::Cursor::new(input.as_bytes());
    let mut output = Vec::new();
    let decision = prompt_revert_choice_with_io(&context, &mut reader, &mut output)
        .expect("prompt with preface");
    let rendered = String::from_utf8(output).expect("output utf8");
    (decision, rendered)
}

#[test]
fn prompt_revert_choice_writes_preface_before_prompt_for_keep() {
    let (decision, output) = prompt_with_preface("1\n");
    assert_eq!(decision, RevertDecision::Keep);
    let preface_idx = output
        .find("Scan validation failed after run.")
        .expect("preface in output");
    let prompt_idx = output
        .find("Scan validation failure: action?")
        .expect("prompt in output");
    assert!(
        preface_idx < prompt_idx,
        "expected preface before prompt, got: {output:?}"
    );
}

#[test]
fn prompt_revert_choice_writes_preface_before_prompt_for_revert() {
    let (decision, output) = prompt_with_preface("2\n");
    assert_eq!(decision, RevertDecision::Revert);
    let preface_idx = output
        .find("Scan validation failed after run.")
        .expect("preface in output");
    let prompt_idx = output
        .find("Scan validation failure: action?")
        .expect("prompt in output");
    assert!(
        preface_idx < prompt_idx,
        "expected preface before prompt, got: {output:?}"
    );
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_keep() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| RevertDecision::Keep);
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Skipped {
            reason: "user chose to keep changes".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_revert() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| RevertDecision::Revert);
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Reverted {
            source: RevertSource::User
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "original");
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_continue() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| RevertDecision::Continue {
        message: "keep going".to_string(),
    });
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Continue {
            message: "keep going".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}

#[test]
fn apply_git_revert_mode_allows_proceed_when_enabled() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| RevertDecision::Proceed);
    let outcome = apply_git_revert_mode_with_context(
        dir.path(),
        GitRevertMode::Ask,
        RevertPromptContext::new("test prompt", true),
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Proceed {
            reason: "user chose to proceed".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}

struct TimeoutBackend {
    emitted: String,
}

impl RunnerBackend for TimeoutBackend {
    fn run_prompt<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _prompt: &str,
        _timeout: Option<Duration>,
        _permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        if let Some(handler) = output_handler {
            (handler)(&self.emitted);
        }
        Err(runner::RunnerError::Timeout)
    }

    fn resume_session<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _session_id: &str,
        _message: &str,
        _permission_mode: Option<ClaudePermissionMode>,
        _timeout: Option<Duration>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for a timeout-only test backend");
    }
}

struct InterruptBackend;

impl RunnerBackend for InterruptBackend {
    fn run_prompt<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _prompt: &str,
        _timeout: Option<Duration>,
        _permission_mode: Option<ClaudePermissionMode>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::Interrupted)
    }

    fn resume_session<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _session_id: &str,
        _message: &str,
        _permission_mode: Option<ClaudePermissionMode>,
        _timeout: Option<Duration>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for interrupt test");
    }
}

struct NonZeroBackend;

impl RunnerBackend for NonZeroBackend {
    fn run_prompt<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _prompt: &str,
        _timeout: Option<Duration>,
        _permission_mode: Option<ClaudePermissionMode>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        Err(runner::RunnerError::NonZeroExit {
            code: 1,
            stdout: "stdout".into(),
            stderr: "stderr".into(),
            session_id: None,
        })
    }

    fn resume_session<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _session_id: &str,
        _message: &str,
        _permission_mode: Option<ClaudePermissionMode>,
        _timeout: Option<Duration>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for non-zero test");
    }
}

#[test]
fn run_prompt_interrupt_returns_abort_reason() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let invocation = RunnerInvocation {
        repo_root: dir.path(),
        runner_kind: Runner::Codex,
        bins: runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Ask,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        revert_prompt: Some(Arc::new(|_context| RevertDecision::Keep)),
        phase_type: PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "interrupt_test",
        interrupted_msg: "interrupted",
        timeout_msg: "timed out",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |_| "other".to_string(),
    };

    let mut backend = InterruptBackend;
    let err = run_prompt_with_handling_backend(invocation, messages, &mut backend).unwrap_err();
    assert_eq!(abort_reason(&err), Some(RunAbortReason::Interrupted));
}

#[test]
fn run_prompt_user_revert_returns_abort_reason() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let invocation = RunnerInvocation {
        repo_root: dir.path(),
        runner_kind: Runner::Codex,
        bins: runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Ask,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        revert_prompt: Some(Arc::new(|_context| RevertDecision::Revert)),
        phase_type: PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "non_zero_test",
        interrupted_msg: "interrupted",
        timeout_msg: "timed out",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |_| "other".to_string(),
    };

    let mut backend = NonZeroBackend;
    let err = run_prompt_with_handling_backend(invocation, messages, &mut backend).unwrap_err();
    assert_eq!(abort_reason(&err), Some(RunAbortReason::UserRevert));

    let reverted = fs::read_to_string(&file_path).expect("read file after revert");
    assert_eq!(reverted, "original");
}

#[test]
fn timeout_applies_git_revert_mode_and_saves_safeguard_dump_when_stdout_is_available() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let invocation = RunnerInvocation {
        repo_root: dir.path(),
        runner_kind: Runner::Codex,
        bins: runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: Some(Duration::from_millis(10)),
        permission_mode: None,
        revert_on_error: true,
        git_revert_mode: GitRevertMode::Enabled,
        output_handler: None,
        output_stream: runner::OutputStream::Terminal,
        revert_prompt: None,
        phase_type: PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "timeout_test",
        interrupted_msg: "interrupted",
        timeout_msg: "timed out",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |_| "other".to_string(),
    };

    let mut backend = TimeoutBackend {
        emitted: "hello from runner before timeout\n".to_string(),
    };

    let err = run_prompt_with_handling_backend(invocation, messages, &mut backend).unwrap_err();
    let msg = format!("{err:#}");

    assert!(msg.contains("timed out"));
    assert!(msg.contains("Uncommitted changes were reverted."));
    assert!(msg.contains("redacted output saved to"));

    // Verify repo clean + file reverted.
    let reverted = fs::read_to_string(&file_path).expect("read file after revert");
    assert_eq!(reverted, "original");

    let status = git::status_porcelain(dir.path()).expect("git status --porcelain -z");
    assert!(
        status.trim().is_empty(),
        "expected clean repo after timeout revert"
    );

    // Verify safeguard file exists and contains our emitted output.
    let marker = "redacted output saved to ";
    let start = msg
        .find(marker)
        .map(|idx| idx + marker.len())
        .expect("find dump path prefix");
    let tail = &msg[start..];
    let end = tail.find(')').unwrap_or(tail.len());
    let path_str = tail[..end].trim();

    let dump = std::path::Path::new(path_str);
    assert!(
        dump.is_file(),
        "expected safeguard dump to exist: {path_str}"
    );
    let dump_contents = fs::read_to_string(dump).expect("read safeguard dump");
    assert!(dump_contents.contains("hello from runner before timeout"));
}

struct CaptureBackend {
    seen_output_stream: Option<runner::OutputStream>,
}

impl RunnerBackend for CaptureBackend {
    fn run_prompt<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _prompt: &str,
        _timeout: Option<Duration>,
        _permission_mode: Option<ClaudePermissionMode>,
        _output_handler: Option<runner::OutputHandler>,
        output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _session_id: Option<String>,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        self.seen_output_stream = Some(output_stream);
        Err(runner::RunnerError::Interrupted)
    }

    fn resume_session<'a>(
        &mut self,
        _runner_kind: Runner,
        _work_dir: &Path,
        _bins: runner::RunnerBinaries<'a>,
        _model: Model,
        _reasoning_effort: Option<ReasoningEffort>,
        _runner_cli: runner::ResolvedRunnerCliOptions,
        _session_id: &str,
        _message: &str,
        _permission_mode: Option<ClaudePermissionMode>,
        _timeout: Option<Duration>,
        _output_handler: Option<runner::OutputHandler>,
        _output_stream: runner::OutputStream,
        _phase_type: PhaseType,
        _plugins: Option<&crate::plugins::registry::PluginRegistry>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        unreachable!("resume_session should not be called for output-stream capture test");
    }
}

#[test]
fn run_prompt_passes_output_stream_to_backend() {
    let dir = TempDir::new().expect("temp dir");
    let invocation = RunnerInvocation {
        repo_root: dir.path(),
        runner_kind: Runner::Codex,
        bins: runner::RunnerBinaries {
            codex: "codex",
            opencode: "opencode",
            gemini: "gemini",
            claude: "claude",
            cursor: "agent",
            kimi: "kimi",
            pi: "pi",
        },
        model: Model::Gpt52Codex,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
        prompt: "test prompt",
        timeout: None,
        permission_mode: None,
        revert_on_error: false,
        git_revert_mode: GitRevertMode::Disabled,
        output_handler: None,
        output_stream: runner::OutputStream::HandlerOnly,
        revert_prompt: None,
        phase_type: PhaseType::Implementation,
        session_id: None,
    };

    let messages = RunnerErrorMessages {
        log_label: "capture",
        interrupted_msg: "interrupted",
        timeout_msg: "timed out",
        terminated_msg: "terminated",
        non_zero_msg: |_| "non-zero".to_string(),
        other_msg: |_| "other".to_string(),
    };

    let mut backend = CaptureBackend {
        seen_output_stream: None,
    };

    let _ = run_prompt_with_handling_backend(invocation, messages, &mut backend);
    assert_eq!(
        backend.seen_output_stream,
        Some(runner::OutputStream::HandlerOnly)
    );
}
