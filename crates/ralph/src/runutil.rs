//! Shared helpers for runner invocations with consistent error handling.

use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::redaction::redact_text;
use crate::{fsutil, gitutil, outpututil, runner};
use anyhow::{bail, Result};
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct RunnerInvocation<'a> {
    pub repo_root: &'a Path,
    pub runner_kind: Runner,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub prompt: &'a str,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    /// If true, revert uncommitted changes on runner errors.
    /// Set to false for task to preserve user's existing work.
    pub revert_on_error: bool,
    /// Policy for reverting uncommitted changes when errors occur.
    pub git_revert_mode: GitRevertMode,
    /// Optional callback for streaming runner output.
    pub output_handler: Option<runner::OutputHandler>,
    /// Optional handler for revert prompts (interactive UIs).
    pub revert_prompt: Option<RevertPromptHandler>,
}

pub struct RunnerErrorMessages<'a, FNonZero, FOther>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    pub log_label: &'a str,
    pub interrupted_msg: &'a str,
    pub timeout_msg: &'a str,
    pub terminated_msg: &'a str,
    pub non_zero_msg: FNonZero,
    pub other_msg: FOther,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertOutcome {
    Reverted,
    Skipped { reason: String },
    Continue { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertDecision {
    Revert,
    Keep,
    Continue { message: String },
}

pub type RevertPromptHandler = Arc<dyn Fn(&str) -> RevertDecision + Send + Sync>;

const TIMEOUT_STDOUT_CAPTURE_MAX_BYTES: usize = 128 * 1024;

trait RunnerBackend {
    #[allow(clippy::too_many_arguments)]
    fn run_prompt<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        prompt: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<runner::OutputHandler>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;

    #[allow(clippy::too_many_arguments)]
    fn resume_session<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        session_id: &str,
        message: &str,
        permission_mode: Option<ClaudePermissionMode>,
        timeout: Option<Duration>,
        output_handler: Option<runner::OutputHandler>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError>;
}

struct RealRunnerBackend;

impl RunnerBackend for RealRunnerBackend {
    fn run_prompt<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        prompt: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<runner::OutputHandler>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        runner::run_prompt(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            prompt,
            timeout,
            permission_mode,
            output_handler,
        )
    }

    fn resume_session<'a>(
        &mut self,
        runner_kind: Runner,
        work_dir: &Path,
        bins: runner::RunnerBinaries<'a>,
        model: Model,
        reasoning_effort: Option<ReasoningEffort>,
        session_id: &str,
        message: &str,
        permission_mode: Option<ClaudePermissionMode>,
        timeout: Option<Duration>,
        output_handler: Option<runner::OutputHandler>,
    ) -> Result<runner::RunnerOutput, runner::RunnerError> {
        runner::resume_session(
            runner_kind,
            work_dir,
            bins,
            model,
            reasoning_effort,
            session_id,
            message,
            permission_mode,
            timeout,
            output_handler,
        )
    }
}

fn wrap_output_handler_with_capture(
    existing: Option<runner::OutputHandler>,
    max_bytes: usize,
) -> (Arc<Mutex<String>>, Option<runner::OutputHandler>) {
    let capture = Arc::new(Mutex::new(String::new()));
    let capture_for_handler = capture.clone();
    let existing_for_handler = existing.clone();

    let handler: runner::OutputHandler = Arc::new(Box::new(move |chunk: &str| {
        if let Ok(mut buf) = capture_for_handler.lock() {
            buf.push_str(chunk);
            if buf.len() > max_bytes {
                let excess = buf.len() - max_bytes;
                buf.drain(..excess);
            }
        }
        if let Some(existing) = existing_for_handler.as_ref() {
            (existing)(chunk);
        }
    }));

    (capture, Some(handler))
}

fn run_prompt_with_handling_backend<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
    backend: &mut impl RunnerBackend,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let RunnerInvocation {
        repo_root,
        runner_kind,
        bins,
        model,
        reasoning_effort,
        prompt,
        timeout,
        permission_mode,
        revert_on_error,
        git_revert_mode,
        output_handler,
        revert_prompt,
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        non_zero_msg,
        other_msg,
    } = messages;

    // Timeout errors do not currently contain stdout. To support safeguard dumps on timeout,
    // capture streamed output (bounded) when a timeout is configured.
    let should_capture_timeout_stdout = revert_on_error && timeout.is_some();
    let (timeout_stdout_capture, effective_output_handler) = if should_capture_timeout_stdout {
        let (capture, handler) =
            wrap_output_handler_with_capture(output_handler, TIMEOUT_STDOUT_CAPTURE_MAX_BYTES);
        (Some(capture), handler)
    } else {
        (None, output_handler)
    };

    let mut result = backend.run_prompt(
        runner_kind,
        repo_root,
        bins,
        model.clone(),
        reasoning_effort,
        prompt,
        timeout,
        permission_mode,
        effective_output_handler.clone(),
    );

    loop {
        match result {
            Ok(output) => return Ok(output),
            Err(runner::RunnerError::Interrupted) => {
                let message = if revert_on_error {
                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    format_revert_failure_message(interrupted_msg, outcome)
                } else {
                    interrupted_msg.to_string()
                };
                bail!("{message}");
            }
            Err(runner::RunnerError::Timeout) => {
                let mut safeguard_msg = String::new();
                let message = if revert_on_error {
                    if let Some(capture) = timeout_stdout_capture.as_ref() {
                        let captured = capture.lock().map(|buf| buf.clone()).unwrap_or_default();
                        if !captured.trim().is_empty() {
                            match fsutil::safeguard_text_dump(
                                "runner_error",
                                &redact_text(&captured),
                            ) {
                                Ok(path) => {
                                    safeguard_msg =
                                        format!("\n(raw stdout saved to {})", path.display());
                                }
                                Err(err) => {
                                    log::warn!("failed to save safeguard dump: {}", err);
                                }
                            }
                        }
                    }

                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    format_revert_failure_message(timeout_msg, outcome)
                } else {
                    timeout_msg.to_string()
                };

                bail!("{}{}", message, safeguard_msg);
            }
            Err(runner::RunnerError::NonZeroExit {
                code,
                stdout,
                stderr,
                session_id,
            }) => {
                log_stderr_tail(log_label, &stderr.to_string());
                let mut safeguard_msg = String::new();
                if revert_on_error {
                    if !stdout.0.is_empty() {
                        match fsutil::safeguard_text_dump("runner_error", &stdout.to_string()) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(raw stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save safeguard dump: {}", err);
                            }
                        }
                    }
                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    match outcome {
                        RevertOutcome::Continue { message } => {
                            let Some(session_id) = session_id.as_deref() else {
                                bail!("Catastrophic: no session id captured; cannot Continue.");
                            };
                            if let Some(capture) = timeout_stdout_capture.as_ref() {
                                if let Ok(mut buf) = capture.lock() {
                                    buf.clear();
                                }
                            }
                            result = backend.resume_session(
                                runner_kind,
                                repo_root,
                                bins,
                                model.clone(),
                                reasoning_effort,
                                session_id,
                                &message,
                                permission_mode,
                                timeout,
                                effective_output_handler.clone(),
                            );
                            continue;
                        }
                        _ => {
                            let message =
                                format_revert_failure_message(&non_zero_msg(code), outcome);
                            bail!("{}{}", message, safeguard_msg);
                        }
                    }
                }
                bail!("{}{}", non_zero_msg(code), safeguard_msg);
            }
            Err(runner::RunnerError::TerminatedBySignal {
                stdout,
                stderr,
                session_id,
            }) => {
                log_stderr_tail(log_label, &stderr.to_string());
                let mut safeguard_msg = String::new();
                if revert_on_error {
                    if !stdout.0.is_empty() {
                        match fsutil::safeguard_text_dump("runner_error", &stdout.to_string()) {
                            Ok(path) => {
                                safeguard_msg =
                                    format!("\n(raw stdout saved to {})", path.display());
                            }
                            Err(err) => {
                                log::warn!("failed to save safeguard dump: {}", err);
                            }
                        }
                    }
                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    match outcome {
                        RevertOutcome::Continue { message } => {
                            let Some(session_id) = session_id.as_deref() else {
                                bail!("Catastrophic: no session id captured; cannot Continue.");
                            };
                            if let Some(capture) = timeout_stdout_capture.as_ref() {
                                if let Ok(mut buf) = capture.lock() {
                                    buf.clear();
                                }
                            }
                            result = backend.resume_session(
                                runner_kind,
                                repo_root,
                                bins,
                                model.clone(),
                                reasoning_effort,
                                session_id,
                                &message,
                                permission_mode,
                                timeout,
                                effective_output_handler.clone(),
                            );
                            continue;
                        }
                        _ => {
                            let message = format_revert_failure_message(terminated_msg, outcome);
                            bail!("{}{}", message, safeguard_msg);
                        }
                    }
                }
                bail!("{}{}", terminated_msg, safeguard_msg);
            }
            Err(err) => {
                let message = if revert_on_error {
                    let outcome = apply_git_revert_mode(
                        repo_root,
                        git_revert_mode,
                        log_label,
                        revert_prompt.as_ref(),
                    )?;
                    format_revert_failure_message(&other_msg(err), outcome)
                } else {
                    other_msg(err)
                };
                bail!("{message}");
            }
        }
    }
}

pub fn run_prompt_with_handling<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let mut backend = RealRunnerBackend;
    run_prompt_with_handling_backend(invocation, messages, &mut backend)
}

pub fn apply_git_revert_mode(
    repo_root: &Path,
    mode: GitRevertMode,
    prompt_label: &str,
    revert_prompt: Option<&RevertPromptHandler>,
) -> Result<RevertOutcome> {
    match mode {
        GitRevertMode::Enabled => {
            gitutil::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted)
        }
        GitRevertMode::Disabled => Ok(RevertOutcome::Skipped {
            reason: "git_revert_mode=disabled".to_string(),
        }),
        GitRevertMode::Ask => {
            if let Some(prompt) = revert_prompt {
                return apply_revert_decision(repo_root, prompt(prompt_label));
            }
            let stdin = std::io::stdin();
            if !stdin.is_terminal() {
                return Ok(RevertOutcome::Skipped {
                    reason: "stdin is not a TTY; keeping changes".to_string(),
                });
            }
            let choice = prompt_revert_choice(prompt_label)?;
            apply_revert_decision(repo_root, choice)
        }
    }
}

fn apply_revert_decision(repo_root: &Path, decision: RevertDecision) -> Result<RevertOutcome> {
    match decision {
        RevertDecision::Revert => {
            gitutil::revert_uncommitted(repo_root)?;
            Ok(RevertOutcome::Reverted)
        }
        RevertDecision::Keep => Ok(RevertOutcome::Skipped {
            reason: "user chose to keep changes".to_string(),
        }),
        RevertDecision::Continue { message } => Ok(RevertOutcome::Continue {
            message: message.trim_end_matches(['\n', '\r']).to_string(),
        }),
    }
}

pub fn format_revert_failure_message(base: &str, outcome: RevertOutcome) -> String {
    match outcome {
        RevertOutcome::Reverted => format!("{base} Uncommitted changes were reverted."),
        RevertOutcome::Skipped { reason } => format!("{base} Revert skipped ({reason})."),
        RevertOutcome::Continue { .. } => {
            format!("{base} Continue requested. No changes were reverted.")
        }
    }
}

/// Build a shell command for the current platform (sh -c on Unix, cmd /C on Windows).
pub fn shell_command(command: &str) -> Command {
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

fn prompt_revert_choice(label: &str) -> Result<RevertDecision> {
    let mut stderr = std::io::stderr();
    eprint!("{label}: action? [1=keep (default), 2=revert, 3=other]: ");
    stderr.flush().ok();

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    let mut input = String::new();
    reader.read_line(&mut input)?;

    let mut decision = parse_revert_response(&input);

    if matches!(decision, RevertDecision::Continue { ref message } if message.is_empty()) {
        eprint!("{label}: enter message to send (empty => keep): ");
        stderr.flush().ok();

        let mut msg = String::new();
        reader.read_line(&mut msg)?;
        let msg = msg.trim_end_matches(['\n', '\r']);
        if msg.trim().is_empty() {
            decision = RevertDecision::Keep;
        } else {
            decision = RevertDecision::Continue {
                message: msg.to_string(),
            };
        }
    }

    Ok(decision)
}

fn parse_revert_response(input: &str) -> RevertDecision {
    let raw = input.trim_end_matches(['\n', '\r']);
    let normalized = raw.trim().to_lowercase();

    match normalized.as_str() {
        "" => RevertDecision::Keep,
        "1" | "k" | "keep" => RevertDecision::Keep,
        "2" | "r" | "revert" => RevertDecision::Revert,
        "3" => RevertDecision::Continue {
            message: String::new(),
        },
        _ => RevertDecision::Continue {
            message: raw.to_string(),
        },
    }
}

fn log_stderr_tail(label: &str, stderr: &str) {
    let tail = outpututil::tail_lines(
        stderr,
        outpututil::OUTPUT_TAIL_LINES,
        outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
    );
    if tail.is_empty() {
        return;
    }

    log::error!("{label} stderr (tail):");
    for line in tail {
        log::info!("{label}: {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_git_repo(dir: &TempDir) {
        Command::new("git")
            .arg("init")
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
            .args(["commit", "-m", message])
            .current_dir(dir.path())
            .output()
            .expect("git commit failed");
    }

    fn decide_ask(stdin_is_terminal: bool, input: Option<&str>) -> RevertDecision {
        if !stdin_is_terminal {
            return RevertDecision::Keep;
        }
        parse_revert_response(input.unwrap_or(""))
    }

    #[test]
    fn ask_mode_defaults_to_keep_when_non_interactive() {
        assert_eq!(decide_ask(false, Some("1")), RevertDecision::Keep);
    }

    #[test]
    fn parse_revert_response_accepts_expected_inputs() {
        assert_eq!(parse_revert_response(""), RevertDecision::Keep);
        assert_eq!(parse_revert_response("1"), RevertDecision::Keep);
        assert_eq!(parse_revert_response("keep"), RevertDecision::Keep);
        assert_eq!(parse_revert_response("2"), RevertDecision::Revert);
        assert_eq!(parse_revert_response("r"), RevertDecision::Revert);
        assert_eq!(parse_revert_response("revert"), RevertDecision::Revert);
        assert_eq!(
            parse_revert_response("3"),
            RevertDecision::Continue {
                message: String::new()
            }
        );
        assert_eq!(
            parse_revert_response("answer that"),
            RevertDecision::Continue {
                message: "answer that".to_string()
            }
        );
    }

    #[test]
    fn apply_git_revert_mode_uses_prompt_handler_keep() {
        let dir = TempDir::new().expect("temp dir");
        init_git_repo(&dir);
        commit_file(&dir, "file.txt", "original", "initial");

        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "modified").expect("modify file");

        let handler: RevertPromptHandler = Arc::new(|_| RevertDecision::Keep);
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

        let handler: RevertPromptHandler = Arc::new(|_| RevertDecision::Revert);
        let outcome = apply_git_revert_mode(
            dir.path(),
            GitRevertMode::Ask,
            "test prompt",
            Some(&handler),
        )
        .expect("apply revert mode");

        assert_eq!(outcome, RevertOutcome::Reverted);
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

        let handler: RevertPromptHandler = Arc::new(|_| RevertDecision::Continue {
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
            _prompt: &str,
            _timeout: Option<Duration>,
            _permission_mode: Option<ClaudePermissionMode>,
            output_handler: Option<runner::OutputHandler>,
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
            _session_id: &str,
            _message: &str,
            _permission_mode: Option<ClaudePermissionMode>,
            _timeout: Option<Duration>,
            _output_handler: Option<runner::OutputHandler>,
        ) -> Result<runner::RunnerOutput, runner::RunnerError> {
            unreachable!("resume_session should not be called for a timeout-only test backend");
        }
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
            },
            model: Model::Gpt52Codex,
            reasoning_effort: None,
            prompt: "test prompt",
            timeout: Some(Duration::from_millis(10)),
            permission_mode: None,
            revert_on_error: true,
            git_revert_mode: GitRevertMode::Enabled,
            output_handler: None,
            revert_prompt: None,
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
        assert!(msg.contains("raw stdout saved to"));

        // Verify repo clean + file reverted.
        let reverted = fs::read_to_string(&file_path).expect("read file after revert");
        assert_eq!(reverted, "original");

        let status = gitutil::status_porcelain(dir.path()).expect("git status --porcelain -z");
        assert!(
            status.trim().is_empty(),
            "expected clean repo after timeout revert"
        );

        // Verify safeguard file exists and contains our emitted output.
        let marker = "raw stdout saved to ";
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
}
