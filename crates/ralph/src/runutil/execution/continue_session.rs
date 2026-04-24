//! Continue-session policy helpers for runner execution.
//!
//! Purpose:
//! - Continue-session policy helpers for runner execution.
//!
//! Responsibilities:
//! - Select a resume session ID.
//! - Centralize known-invalid continue-session fallback classification.
//! - Execute continue-session or rerun flows through the backend.
//! - Narrate when Ralph is resuming, rerunning fresh, or lacking a reusable session id.
//!
//! Not handled here:
//! - Retry policy.
//! - Error shaping after runner failures.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Fresh continue fallbacks stay conservative and runner-specific.
//! - Unknown resume failures must still hard-fail instead of silently rerunning.

use crate::contracts::Runner;
use crate::runner;

use super::backend::{RunnerAttemptContext, RunnerBackend};

fn continue_session_error_text(err: &runner::RunnerError) -> String {
    match err {
        runner::RunnerError::NonZeroExit { stdout, stderr, .. }
        | runner::RunnerError::TerminatedBySignal { stdout, stderr, .. } => {
            format!("{} {}", stdout, stderr).to_lowercase()
        }
        _ => format!("{:#}", err).to_lowercase(),
    }
}

pub(crate) fn should_fallback_to_fresh_continue(
    runner_kind: &Runner,
    err: &runner::RunnerError,
) -> bool {
    let text = continue_session_error_text(err);

    match runner_kind {
        Runner::Pi => {
            text.contains("pi session file not found")
                || text.contains("no session found matching")
                || text.contains("read pi session dir")
        }
        Runner::Gemini => {
            text.contains("error resuming session")
                && (text.contains("invalid session identifier") || text.contains("--list-sessions"))
        }
        Runner::Claude => {
            text.contains("--resume requires a valid session id")
                || text.contains("not a valid uuid")
        }
        Runner::Opencode => {
            (text.contains("zoderror")
                && text.contains("sessionid")
                && text.contains("must start with \"ses\""))
                || (text.contains("semantic failure with zero exit status")
                    && text.contains("opencode"))
        }
        Runner::Cursor => {
            text.contains("invalid session")
                || text.contains("invalid chat")
                || text.contains("session not found")
                || text.contains("unknown session")
                || text.contains("no session found")
                || text.contains("no matching session")
                || (text.contains("resume") && text.contains("not found"))
        }
        _ => false,
    }
}

fn choose_continue_session_id<'a>(
    error_session_id: Option<&'a str>,
    invocation_session_id: Option<&'a str>,
) -> Option<&'a str> {
    error_session_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .or_else(|| {
            invocation_session_id
                .map(str::trim)
                .filter(|id| !id.is_empty())
        })
}

pub(super) fn continue_or_rerun(
    backend: &mut impl RunnerBackend,
    attempt: &RunnerAttemptContext<'_>,
    continue_message: &str,
    fresh_prompt: &str,
    invocation_session_id: Option<&str>,
    error_session_id: Option<&str>,
) -> Result<runner::RunnerOutput, runner::RunnerError> {
    let continue_session_id = choose_continue_session_id(error_session_id, invocation_session_id);
    if let Some(session_id) = continue_session_id {
        match backend.resume_session(attempt.resume_session_request(session_id, continue_message)) {
            Ok(output) => {
                eprintln!(
                    "Resume: continuing the existing runner session for phase {:?}.",
                    attempt.phase_type
                );
                return Ok(output);
            }
            Err(err) if should_fallback_to_fresh_continue(attempt.runner_kind, &err) => {
                eprintln!(
                    "Resume: existing runner session could not be reused; starting a fresh invocation."
                );
                eprintln!("  {}", err);
            }
            Err(err) => return Err(err),
        }
    } else {
        eprintln!("Resume: no runner session id was available; starting a fresh invocation.");
    }

    backend.run_prompt(
        attempt.run_prompt_request(fresh_prompt, invocation_session_id.map(str::to_string)),
    )
}

#[cfg(test)]
mod tests {
    use crate::contracts::Runner;
    use crate::runner::RunnerError;

    use super::should_fallback_to_fresh_continue;

    #[test]
    fn cursor_resume_unknown_session_falls_back_to_fresh() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "".into(),
            stderr: "session not found for resume".into(),
            session_id: None,
        };
        assert!(should_fallback_to_fresh_continue(&Runner::Cursor, &err));
    }

    #[test]
    fn cursor_resume_unrecognized_error_does_not_fallback() {
        let err = RunnerError::NonZeroExit {
            code: 1,
            stdout: "".into(),
            stderr: "unexpected failure".into(),
            session_id: None,
        };
        assert!(!should_fallback_to_fresh_continue(&Runner::Cursor, &err));
    }
}
