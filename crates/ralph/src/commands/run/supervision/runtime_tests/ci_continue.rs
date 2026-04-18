//! CI-continue resume coverage for supervision.
//!
//! Responsibilities:
//! - Verify CI-gate continue flows resume the same runner session with the operator message.
//! - Keep interrupt-flag coordination localized to CI-continue regressions.
//!
//! Not handled here:
//! - Base resume fallback behavior for invalid sessions.
//! - Post-run git/queue scenarios without CI continue.
//!
//! Invariants/assumptions:
//! - Tests hold the interrupt mutex for the full scenario when mutating interrupt state.

use super::support::{continue_review_session, resolved_for_repo, write_queue};
use crate::commands::run::supervision::{CiContinueContext, PushPolicy, post_run_supervise};
use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
use crate::contracts::{CiGateConfig, GitRevertMode, Runner, TaskStatus};
use crate::runutil;
use crate::testsupport::git as git_test;
use crate::testsupport::runner::create_fake_runner;
use crate::testsupport::{INTERRUPT_TEST_MUTEX, reset_ctrlc_interrupt_flag};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[test]
fn post_run_supervise_ci_gate_continue_resumes_session() -> anyhow::Result<()> {
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = TempDir::new()?;
    git_test::init_repo(temp.path())?;
    write_queue(temp.path(), TaskStatus::Todo)?;
    git_test::commit_all(temp.path(), "init")?;
    std::fs::write(temp.path().join("work.txt"), "change")?;

    let resume_args = temp.path().join("resume-args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
set -e
echo "$@" > "{resume_args}"
echo '{{"type":"text","part":{{"text":"resume"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
        resume_args = resume_args.display()
    );
    let runner_path = create_fake_runner(temp.path(), "opencode", &runner_script)?;

    let ci_pass = temp.path().join("ci-pass.txt");
    let mut resolved = resolved_for_repo(temp.path());
    std::fs::write(
        temp.path().join(".ralph/trust.jsonc"),
        r#"{"allow_project_commands": true}"#,
    )?;
    resolved.config.agent.ci_gate = Some(CiGateConfig {
        enabled: Some(true),
        argv: Some(vec![
            "python3".to_string(),
            "-c".to_string(),
            format!(
                "from pathlib import Path; raise SystemExit(0 if Path(r\"{}\").is_file() else 1)",
                ci_pass.display()
            ),
        ]),
    });
    resolved.config.agent.opencode_bin = Some(runner_path.to_str().unwrap().to_string());

    let prompt_handler: runutil::RevertPromptHandler = Arc::new(|_context| {
        Ok(runutil::RevertDecision::Continue {
            message: "fix the ci gate".to_string(),
        })
    });

    let mut continue_session = continue_review_session("sess-123");
    continue_session.runner = Runner::Opencode;
    continue_session.ci_failure_retry_count = CI_GATE_AUTO_RETRY_LIMIT;

    let mut on_resume = |_output: &crate::runner::RunnerOutput,
                         _elapsed: std::time::Duration|
     -> anyhow::Result<()> {
        std::fs::write(&ci_pass, "ok")?;
        Ok(())
    };

    post_run_supervise(
        &resolved,
        None,
        "RQ-0001",
        GitRevertMode::Ask,
        crate::contracts::GitPublishMode::Off,
        PushPolicy::RequireUpstream,
        Some(prompt_handler),
        Some(CiContinueContext {
            continue_session: &mut continue_session,
            on_resume: &mut on_resume,
        }),
        None,
        None,
        false,
        false,
        None,
    )?;

    let args = std::fs::read_to_string(&resume_args)?;
    anyhow::ensure!(
        args.contains("fix the ci gate"),
        "expected resume args to include continue message, got: {args}"
    );
    Ok(())
}
