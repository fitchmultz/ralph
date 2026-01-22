//! Shared helpers for runner invocations with consistent error handling.

use crate::contracts::{ClaudePermissionMode, GitRevertMode, Model, ReasoningEffort, Runner};
use crate::{fsutil, gitutil, outpututil, runner};
use anyhow::{bail, Result};
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertDecision {
    Revert,
    Keep,
}

pub type RevertPromptHandler = Arc<dyn Fn(&str) -> RevertDecision + Send + Sync>;

pub fn run_prompt_with_handling<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
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

    match runner::run_prompt(
        runner_kind,
        repo_root,
        bins,
        model,
        reasoning_effort,
        prompt,
        timeout,
        permission_mode,
        output_handler,
    ) {
        Ok(output) => Ok(output),
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
            bail!("{}", timeout_msg);
        }
        Err(runner::RunnerError::NonZeroExit {
            code,
            stdout,
            stderr,
        }) => {
            log_stderr_tail(log_label, &stderr.to_string());
            let mut safeguard_msg = String::new();
            if revert_on_error {
                if !stdout.0.is_empty() {
                    match fsutil::safeguard_text_dump("runner_error", &stdout.to_string()) {
                        Ok(path) => {
                            safeguard_msg = format!("\n(raw stdout saved to {})", path.display());
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
                let message = format_revert_failure_message(&non_zero_msg(code), outcome);
                bail!("{}{}", message, safeguard_msg);
            }
            bail!("{}{}", non_zero_msg(code), safeguard_msg);
        }
        Err(runner::RunnerError::TerminatedBySignal { stdout, stderr }) => {
            log_stderr_tail(log_label, &stderr.to_string());
            let mut safeguard_msg = String::new();
            if revert_on_error {
                if !stdout.0.is_empty() {
                    match fsutil::safeguard_text_dump("runner_error", &stdout.to_string()) {
                        Ok(path) => {
                            safeguard_msg = format!("\n(raw stdout saved to {})", path.display());
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
                let message = format_revert_failure_message(terminated_msg, outcome);
                bail!("{}{}", message, safeguard_msg);
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
    }
}

pub fn format_revert_failure_message(base: &str, outcome: RevertOutcome) -> String {
    match outcome {
        RevertOutcome::Reverted => format!("{base} Uncommitted changes were reverted."),
        RevertOutcome::Skipped { reason } => format!("{base} Revert skipped ({reason})."),
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
    eprint!("{label}: revert uncommitted changes? [1=revert (default), 2=keep]: ");
    stderr.flush().ok();

    let mut input = String::new();
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    reader.read_line(&mut input)?;
    Ok(parse_revert_response(&input).unwrap_or_else(|| {
        log::warn!(
            "{label}: unrecognized response '{}'; defaulting to revert",
            input.trim()
        );
        RevertDecision::Revert
    }))
}

fn parse_revert_response(input: &str) -> Option<RevertDecision> {
    let trimmed = input.trim().to_lowercase();
    match trimmed.as_str() {
        "" => Some(RevertDecision::Revert),
        "1" | "y" | "yes" => Some(RevertDecision::Revert),
        "2" | "n" | "no" => Some(RevertDecision::Keep),
        _ => None,
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
        parse_revert_response(input.unwrap_or("")).unwrap_or(RevertDecision::Revert)
    }

    #[test]
    fn ask_mode_defaults_to_keep_when_non_interactive() {
        assert_eq!(decide_ask(false, Some("1")), RevertDecision::Keep);
    }

    #[test]
    fn parse_revert_response_accepts_expected_inputs() {
        assert_eq!(parse_revert_response(""), Some(RevertDecision::Revert));
        assert_eq!(parse_revert_response("1"), Some(RevertDecision::Revert));
        assert_eq!(parse_revert_response("y"), Some(RevertDecision::Revert));
        assert_eq!(parse_revert_response("yes"), Some(RevertDecision::Revert));
        assert_eq!(parse_revert_response("2"), Some(RevertDecision::Keep));
        assert_eq!(parse_revert_response("n"), Some(RevertDecision::Keep));
        assert_eq!(parse_revert_response("no"), Some(RevertDecision::Keep));
        assert_eq!(parse_revert_response("wat"), None);
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
}
