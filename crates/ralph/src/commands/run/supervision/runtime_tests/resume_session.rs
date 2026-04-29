//! Resume fallback coverage for supervision continue sessions.
//!
//! Purpose:
//! - Resume fallback coverage for supervision continue sessions.
//!
//! Responsibilities:
//! - Verify invalid or missing session identifiers fall back to fresh runner invocations.
//! - Cover runner-specific resume semantics for Opencode, Pi, Gemini, and Claude.
//! - Assert the explicit resume decision model matches the fallback path that executed.
//!
//! Not handled here:
//! - Post-run supervision or git/queue mutations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Fake runner scripts emit machine-readable output matching each runner contract.

use super::support::{PI_ENV_MUTEX, continue_session_with, resolved_for_repo};
use crate::commands::run::supervision::resume_continue_session;
use crate::contracts::Runner;
use crate::testsupport::runner::create_fake_runner;
use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};
use std::sync::{Arc, Mutex, MutexGuard};
use tempfile::TempDir;

fn interrupt_guard() -> MutexGuard<'static, ()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let guard = interrupt_mutex.lock().expect("interrupt mutex poisoned");
    reset_ctrlc_interrupt_flag();
    guard
}

#[test]
fn resume_continue_session_emits_resume_decision_event_for_invalid_session_fallback()
-> anyhow::Result<()> {
    let _interrupt_guard = interrupt_guard();
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
for arg in "$@"; do
  if [ "$arg" = "-s" ] || [ "$arg" = "--session" ]; then
    echo 'ZodError: invalid_format sessionID' >&2
    echo 'Invalid string: must start with "ses"' >&2
    exit 0
  fi
done
echo "$@" > "{args_path}"
echo '{{"type":"text","part":{{"text":"fresh"}}}}'
echo '{{"sessionID":"sess-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "opencode", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.opencode_bin = Some(runner_path.to_string_lossy().to_string());

    let captured = Arc::new(Mutex::new(Vec::new()));
    let handler: crate::commands::run::RunEventHandler = Arc::new(Box::new({
        let captured = Arc::clone(&captured);
        move |event| captured.lock().expect("event mutex poisoned").push(event)
    }));
    let mut session = continue_session_with(
        Runner::Opencode,
        Some("bad-session"),
        crate::commands::run::PhaseType::Implementation,
    );
    session.run_event_handler = Some(handler);
    session.output_stream = crate::runner::OutputStream::HandlerOnly;

    let resumed = resume_continue_session(&resolved, &mut session, "hello", None)?;

    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::RunnerSessionInvalid
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-fresh"));
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "-s"),
        "fresh invocation should not include resume session args, got: {args}"
    );
    assert!(
        captured
            .lock()
            .expect("event mutex poisoned")
            .iter()
            .any(|event| matches!(
                event,
                crate::commands::run::RunEvent::ResumeDecision { decision }
                    if decision.reason == crate::session::ResumeReason::RunnerSessionInvalid
            )),
        "expected resume_continue_session to emit a structured resume decision event"
    );
    Ok(())
}

#[test]
fn resume_continue_session_falls_back_to_fresh_invocation_without_session_id() -> anyhow::Result<()>
{
    let _interrupt_guard = interrupt_guard();
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{args_path}"
echo '{{"type":"text","part":{{"text":"fresh"}}}}'
echo '{{"sessionID":"sess-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "opencode", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.opencode_bin = Some(runner_path.to_string_lossy().to_string());

    let mut session = continue_session_with(
        Runner::Opencode,
        None,
        crate::commands::run::PhaseType::Implementation,
    );
    let resumed = resume_continue_session(&resolved, &mut session, "hello", None)?;

    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "-s"),
        "fresh invocation should not include resume session args, got: {args}"
    );
    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::MissingRunnerSessionId
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_pi_falls_back_to_fresh_when_resume_lookup_fails() -> anyhow::Result<()> {
    let _interrupt_guard = interrupt_guard();
    let _env_guard = PI_ENV_MUTEX.lock().expect("pi env mutex poisoned");
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("pi-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{args_path}"
echo '{{"type":"result","result":"fresh"}}'
echo '{{"sessionID":"sess-pi-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "pi", &runner_script)?;

    let previous_pi_root = std::env::var_os("PI_CODING_AGENT_DIR");
    let pi_root = temp_dir.path().join("pi-root");
    std::fs::create_dir_all(&pi_root)?;
    unsafe { std::env::set_var("PI_CODING_AGENT_DIR", &pi_root) };

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.pi_bin = Some(runner_path.to_string_lossy().to_string());
    let mut session = continue_session_with(
        Runner::Pi,
        Some("missing-session-id"),
        crate::commands::run::PhaseType::Implementation,
    );

    let result = resume_continue_session(&resolved, &mut session, "hello", None);

    match previous_pi_root {
        Some(value) => unsafe { std::env::set_var("PI_CODING_AGENT_DIR", value) },
        None => unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") },
    }

    let resumed = result?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--session"),
        "fresh invocation should not include --session args, got: {args}"
    );
    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::RunnerSessionInvalid
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-pi-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_gemini_falls_back_to_fresh_on_invalid_resume() -> anyhow::Result<()> {
    let _interrupt_guard = interrupt_guard();
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("gemini-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
if printf '%s' "$*" | grep -q -- '--resume'; then
  echo 'Error resuming session: Invalid session identifier "does-not-exist".' >&2
  echo '  Use --list-sessions to see available sessions.' >&2
  exit 42
fi
echo "$*" > "{args_path}"
echo '{{"type":"message","role":"assistant","content":"fresh"}}'
echo '{{"session_id":"sess-gemini-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "gemini", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.gemini_bin = Some(runner_path.to_string_lossy().to_string());
    let mut session = continue_session_with(
        Runner::Gemini,
        Some("does-not-exist"),
        crate::commands::run::PhaseType::Implementation,
    );

    let resumed = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--resume"),
        "fresh invocation should not include --resume args, got: {args}"
    );
    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::RunnerSessionInvalid
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-gemini-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_claude_falls_back_to_fresh_on_invalid_uuid() -> anyhow::Result<()> {
    let _interrupt_guard = interrupt_guard();
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("claude-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
if printf '%s' "$*" | grep -q -- '--resume'; then
  echo '{{"type":"result","is_error":true,"errors":["--resume requires a valid session ID"]}}'
  exit 1
fi
echo "$*" > "{args_path}"
echo '{{"type":"assistant","session_id":"sess-claude-fresh","message":{{"content":[{{"type":"text","text":"fresh"}}]}}}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "claude", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.claude_bin = Some(runner_path.to_string_lossy().to_string());
    let mut session = continue_session_with(
        Runner::Claude,
        Some("not-a-uuid"),
        crate::commands::run::PhaseType::Implementation,
    );

    let resumed = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "--resume"),
        "fresh invocation should not include --resume args, got: {args}"
    );
    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::RunnerSessionInvalid
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-claude-fresh"));
    Ok(())
}

#[test]
fn resume_continue_session_opencode_falls_back_when_resume_errors_with_exit_zero()
-> anyhow::Result<()> {
    let _interrupt_guard = interrupt_guard();
    let temp_dir = TempDir::new()?;
    let args_path = temp_dir.path().join("opencode-runner-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
for arg in "$@"; do
  if [ "$arg" = "-s" ] || [ "$arg" = "--session" ]; then
    echo 'ZodError: invalid_format sessionID' >&2
    echo 'Invalid string: must start with "ses"' >&2
    exit 0
  fi
done
echo "$*" > "{args_path}"
echo '{{"type":"text","part":{{"text":"fresh"}}}}'
echo '{{"sessionID":"sess-opencode-fresh"}}'
"#,
        args_path = args_path.display()
    );
    let runner_path = create_fake_runner(temp_dir.path(), "opencode", &runner_script)?;

    let mut resolved = resolved_for_repo(temp_dir.path());
    resolved.config.agent.opencode_bin = Some(runner_path.to_string_lossy().to_string());
    let mut session = continue_session_with(
        Runner::Opencode,
        Some("bad-session"),
        crate::commands::run::PhaseType::Implementation,
    );

    let resumed = resume_continue_session(&resolved, &mut session, "hello", None)?;
    let args = std::fs::read_to_string(&args_path)?;
    assert!(
        !args.split_whitespace().any(|arg| arg == "-s"),
        "fresh invocation should not include -s args, got: {args}"
    );
    assert_eq!(
        resumed.decision.reason,
        crate::session::ResumeReason::RunnerSessionInvalid
    );
    assert_eq!(session.session_id.as_deref(), Some("sess-opencode-fresh"));
    Ok(())
}
