//! Run-surface contract coverage for `ralph machine`.
//!
//! Purpose:
//! - Verify `ralph machine run` contracts for canonical task selection and loop terminal summaries.
//!
//! Responsibilities:
//! - Assert no-ID `machine run one --resume` emits `run_started` without a task ID.
//! - Verify `task_selected` and the final summary expose the actual CLI-selected task.
//! - Assert `machine run loop` preserves idle, blocked, and stalled terminal summaries.
//! - Lock the startup-versus-in-stream failure boundary for `machine run loop`.
//! - Keep run-surface assertions isolated from queue, recovery, and parallel suites.
//!
//! Non-scope:
//! - Runner output formatting details beyond the machine contract markers needed by RalphMac.
//! - Parallel worker lifecycle contracts unrelated to terminal machine summaries.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - The fake runner completes deterministically and does not require network access.
//! - Queue fixtures intentionally make the CLI-selected task unambiguous.
//! - `machine run loop` emits `run_started` before its terminal summary.
//! - Terminal summaries remain single-line JSON documents that can be parsed from the last line.

use super::machine_contract_test_support::{
    configure_ci_gate, configure_runner, create_fake_runner, git_add_all_commit, make_test_task,
    run_in_dir, setup_ralph_repo, trust_project_commands, write_done, write_queue,
};
use anyhow::{Context, Result};
use ralph::contracts::{Runner, SessionState, TaskStatus};
use ralph::queue;
use ralph::session::save_session;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn parse_machine_error_document(stderr: &str) -> Result<Value> {
    let json_start = stderr
        .find('{')
        .context("expected machine_error JSON object in stderr")?;
    serde_json::from_str(&stderr[json_start..]).context("parse machine_error document")
}

fn configure_parallel_origin(dir: &Path) -> Result<()> {
    let remote_dir = dir.join("origin.git");
    let init = Command::new("git")
        .current_dir(dir)
        .args([
            "init",
            "--quiet",
            "--bare",
            remote_dir.to_str().expect("utf-8 path"),
        ])
        .status()?;
    assert!(init.success(), "expected bare origin init to succeed");

    let add_remote = Command::new("git")
        .current_dir(dir)
        .args([
            "remote",
            "add",
            "origin",
            remote_dir.to_str().expect("utf-8 path"),
        ])
        .status()?;
    assert!(
        add_remote.success(),
        "expected origin remote add to succeed"
    );
    Ok(())
}

#[test]
fn machine_run_one_without_id_reports_selected_task_via_events_and_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let queue = serde_json::json!({
        "version": 1,
        "tasks": [
            {
                "id": "RQ-0001",
                "status": "todo",
                "title": "Canonical next task",
                "priority": "high",
                "created_at": "2026-03-10T00:00:00Z",
                "updated_at": "2026-03-10T00:00:00Z"
            }
        ]
    });
    std::fs::write(
        dir.path().join(".ralph/queue.jsonc"),
        serde_json::to_string_pretty(&queue)?,
    )
    .context("write queue fixture")?;

    let runner_path = create_fake_runner(
        dir.path(),
        "codex",
        r#"#!/bin/sh
printf '{"type":"assistant","message":{"content":[{"type":"output_text","text":"runner output"}]}}\n'
"#,
    )?;
    configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;
    configure_ci_gate(dir.path(), None, Some(false))?;
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo CI passed\n")?;
    git_add_all_commit(dir.path(), "setup")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "one", "--resume"]);
    assert!(
        status.success(),
        "machine run one --resume failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let lines: Vec<Value> = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("parse machine run output")?;

    let run_started = lines.first().context("expected run_started event")?;
    assert_eq!(run_started["kind"], "run_started");
    assert!(run_started["task_id"].is_null());

    let task_selected = lines
        .iter()
        .find(|line| line["kind"] == "task_selected")
        .context("expected task_selected event")?;
    assert_eq!(task_selected["task_id"], "RQ-0001");

    let summary = lines.last().context("expected machine run summary")?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["task_id"], "RQ-0001");

    Ok(())
}

#[test]
fn machine_run_loop_empty_repo_reports_no_candidates_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "loop"]);
    assert!(
        status.success(),
        "machine run loop failed on empty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_str(
        stdout
            .lines()
            .last()
            .expect("expected machine loop summary line"),
    )?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["task_id"], Value::Null);
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["outcome"], "no_candidates");
    assert_eq!(summary["blocking"]["status"], "waiting");
    assert_eq!(summary["blocking"]["reason"]["kind"], "idle");
    Ok(())
}

#[test]
fn machine_run_loop_parallel_empty_repo_reports_no_candidates_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;
    trust_project_commands(dir.path())?;
    configure_parallel_origin(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["machine", "run", "loop", "--force", "--parallel", "2"],
    );
    assert!(
        status.success(),
        "machine run loop --parallel failed on empty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_str(
        stdout
            .lines()
            .last()
            .expect("expected machine loop summary line"),
    )?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["task_id"], Value::Null);
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["outcome"], "no_candidates");
    assert_eq!(summary["blocking"]["status"], "waiting");
    assert_eq!(summary["blocking"]["reason"]["kind"], "idle");
    Ok(())
}

#[test]
fn machine_run_loop_dependency_blocked_repo_reports_blocked_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let mut blocked = make_test_task("RQ-2001", "Scheduled task", TaskStatus::Todo);
    blocked.scheduled_start = Some("2099-01-01T00:00:00Z".to_string());
    write_queue(dir.path(), &[blocked])?;
    write_done(dir.path(), &[])?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "loop", "--force"]);
    assert!(
        status.success(),
        "machine run loop failed on blocked repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_str(
        stdout
            .lines()
            .last()
            .expect("expected machine loop summary line"),
    )?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["outcome"], "blocked");
    assert_eq!(summary["blocking"]["status"], "waiting");
    assert_eq!(summary["blocking"]["reason"]["kind"], "schedule_blocked");
    assert_eq!(summary["blocking"]["reason"]["blocked_tasks"], 1);
    Ok(())
}

#[test]
fn machine_run_loop_parallel_blocked_repo_reports_blocked_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;
    trust_project_commands(dir.path())?;
    configure_parallel_origin(dir.path())?;

    let mut blocked = make_test_task("RQ-2002", "Scheduled task", TaskStatus::Todo);
    blocked.scheduled_start = Some("2099-01-01T00:00:00Z".to_string());
    write_queue(dir.path(), &[blocked])?;
    write_done(dir.path(), &[])?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["machine", "run", "loop", "--force", "--parallel", "2"],
    );
    assert!(
        status.success(),
        "machine run loop --parallel failed on blocked repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_str(
        stdout
            .lines()
            .last()
            .expect("expected machine loop summary line"),
    )?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["outcome"], "blocked");
    assert_eq!(summary["blocking"]["status"], "waiting");
    assert_eq!(summary["blocking"]["reason"]["kind"], "schedule_blocked");
    assert_eq!(summary["blocking"]["reason"]["blocked_tasks"], 1);
    Ok(())
}

#[test]
fn machine_run_stop_creates_stop_marker_document() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "stop"]);
    assert!(
        status.success(),
        "machine run stop failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(document["version"], 1);
    assert_eq!(document["dry_run"], false);
    assert_eq!(document["action"], "created");
    assert_eq!(document["marker"]["existed_before"], false);
    assert_eq!(document["marker"]["exists_after"], true);
    let actual_path = document["marker"]["path"]
        .as_str()
        .context("expected marker path string")?;
    assert_eq!(
        std::fs::canonicalize(actual_path)?,
        std::fs::canonicalize(dir.path().join(".ralph/cache/stop_requested"))?
    );
    Ok(())
}

#[test]
fn machine_run_stop_reports_already_present_marker() -> Result<()> {
    let dir = setup_ralph_repo()?;
    let cache_dir = dir.path().join(".ralph/cache");
    ralph::signal::create_stop_signal(&cache_dir)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "stop"]);
    assert!(
        status.success(),
        "machine run stop failed with existing marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(document["action"], "already_present");
    assert_eq!(document["marker"]["existed_before"], true);
    assert_eq!(document["marker"]["exists_after"], true);
    Ok(())
}

#[test]
fn machine_run_stop_dry_run_previews_marker_without_writing() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "stop", "--dry-run"]);
    assert!(
        status.success(),
        "machine run stop --dry-run failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(document["dry_run"], true);
    assert_eq!(document["action"], "would_create");
    assert_eq!(document["marker"]["existed_before"], false);
    assert_eq!(document["marker"]["exists_after"], false);
    assert!(!dir.path().join(".ralph/cache/stop_requested").exists());
    Ok(())
}

#[test]
fn machine_run_stop_uses_runtime_parallel_state_for_guidance() -> Result<()> {
    let dir = setup_ralph_repo()?;
    std::fs::write(
        dir.path().join(".ralph/config.jsonc"),
        r#"{"version":2,"parallel":{"workers":2}}"#,
    )
    .context("write parallel config fixture")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "stop"]);
    assert!(
        status.success(),
        "machine run stop with configured parallel workers failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let document: Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(document["action"], "created");
    assert_eq!(document["blocking"], Value::Null);
    assert_eq!(
        document["continuation"]["detail"],
        "The stop marker is recorded. Ralph should exit after the current task completes."
    );
    let next_steps = document["continuation"]["next_steps"]
        .as_array()
        .context("expected continuation next_steps array")?;
    assert!(
        next_steps
            .iter()
            .all(|step| step["command"] != "ralph machine run parallel-status"),
        "stop guidance should not suggest parallel status without live parallel state"
    );
    assert_eq!(
        next_steps
            .iter()
            .find(|step| step["title"] == "Resume run-control inspection")
            .context("expected loop resume step")?["command"],
        "ralph machine run loop --resume --max-tasks 0"
    );
    Ok(())
}

#[test]
fn machine_run_stop_startup_failure_emits_only_machine_error() -> Result<()> {
    let dir = setup_ralph_repo()?;
    let cache_dir = dir.path().join(".ralph/cache");
    std::fs::write(&cache_dir, "not a directory").context("block cache dir creation")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "stop"]);
    assert!(!status.success(), "machine run stop unexpectedly succeeded");
    assert!(
        stdout.trim().is_empty(),
        "expected no stdout success document"
    );

    let machine_error = parse_machine_error_document(&stderr)?;
    assert_eq!(machine_error["version"], 1);
    assert_eq!(machine_error["code"], "unknown");
    Ok(())
}

#[test]
fn machine_run_loop_override_startup_failure_emits_only_machine_error() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["machine", "run", "loop", "--runner", "nope"]);
    assert!(
        !status.success(),
        "machine run loop should fail for invalid runner override\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "startup failure should not begin machine stdout stream:\n{stdout}"
    );

    let machine_error = parse_machine_error_document(&stderr)?;
    assert_eq!(machine_error["version"], 1);
    assert_eq!(machine_error["code"], "parse_error");
    Ok(())
}

#[test]
fn machine_run_loop_resume_refusal_reports_stalled_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let mut task = make_test_task("RQ-3001", "Resume candidate", TaskStatus::Doing);
    task.started_at = Some("2026-04-20T00:00:00Z".to_string());
    write_queue(dir.path(), &[task])?;
    write_done(dir.path(), &[])?;

    let cache_dir = dir.path().join(".ralph/cache");
    let session = SessionState {
        version: 1,
        session_id: "session-RQ-3001".to_string(),
        task_id: "RQ-3001".to_string(),
        run_started_at: "2026-04-20T00:00:00Z".to_string(),
        last_updated_at: "2026-04-20T00:05:00Z".to_string(),
        iterations_planned: 3,
        iterations_completed: 1,
        current_phase: 2,
        runner: Runner::Codex,
        model: "gpt-5.5".to_string(),
        tasks_completed_in_loop: 0,
        max_tasks: 0,
        git_head_commit: None,
        phase1_settings: None,
        phase2_settings: None,
        phase3_settings: None,
    };
    save_session(&cache_dir, &session)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["machine", "run", "loop", "--force"]);
    assert!(
        status.success(),
        "machine run loop should summarize resume refusal without hard failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let summary: Value = serde_json::from_str(
        stdout
            .lines()
            .last()
            .expect("expected machine loop summary line"),
    )?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["exit_code"], 0);
    assert_eq!(summary["outcome"], "stalled");
    assert_eq!(summary["blocking"]["status"], "stalled");
    assert_eq!(summary["blocking"]["reason"]["kind"], "runner_recovery");
    assert_eq!(
        summary["blocking"]["reason"]["reason"],
        "session_timed_out_requires_confirmation"
    );
    Ok(())
}

#[test]
fn machine_run_loop_runtime_failure_still_emits_terminal_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let task = make_test_task("RQ-4001", "Fails CI after run starts", TaskStatus::Todo);
    write_queue(dir.path(), &[task])?;
    write_done(dir.path(), &[])?;

    let runner_path = create_fake_runner(
        dir.path(),
        "codex",
        r#"#!/bin/sh
cat >/dev/null
exit 1
"#,
    )?;
    configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;
    configure_ci_gate(dir.path(), None, Some(false))?;
    git_add_all_commit(dir.path(), "setup")?;

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["machine", "run", "loop", "--max-tasks", "1"]);
    assert!(
        !status.success(),
        "machine run loop should fail after run_started\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let lines: Vec<Value> = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("parse machine loop runtime failure output")?;
    let run_started = lines
        .iter()
        .find(|line| line["kind"] == "run_started")
        .context("expected run_started event")?;
    assert_eq!(run_started["kind"], "run_started");

    let summary = lines
        .last()
        .context("expected terminal machine run summary")?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["task_id"], Value::Null);
    assert_eq!(summary["exit_code"], 1);
    assert_eq!(summary["outcome"], "failed");
    assert!(summary["blocking"].is_null());

    let machine_error = parse_machine_error_document(&stderr)?;
    assert_eq!(machine_error["version"], 1);
    Ok(())
}

#[test]
fn machine_run_loop_queue_lock_failure_emits_stalled_terminal_summary() -> Result<()> {
    let dir = setup_ralph_repo()?;

    let task = make_test_task(
        "RQ-4002",
        "Hits queue lock after run starts",
        TaskStatus::Todo,
    );
    write_queue(dir.path(), &[task])?;
    write_done(dir.path(), &[])?;

    let _lock = queue::acquire_queue_lock(dir.path(), "test lock holder", false)?;

    let (status, stdout, stderr) =
        run_in_dir(dir.path(), &["machine", "run", "loop", "--max-tasks", "1"]);
    assert!(
        !status.success(),
        "machine run loop should report queue lock failure after run_started\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let lines: Vec<Value> = stdout
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .context("parse machine loop ci failure output")?;
    let run_started = lines
        .iter()
        .find(|line| line["kind"] == "run_started")
        .context("expected run_started event")?;
    assert_eq!(run_started["kind"], "run_started");

    let summary = lines
        .last()
        .context("expected terminal machine run summary")?;
    assert_eq!(summary["version"], 2);
    assert_eq!(summary["task_id"], Value::Null);
    assert_eq!(summary["exit_code"], 1);
    assert_eq!(summary["outcome"], "stalled");
    assert_eq!(summary["blocking"]["status"], "stalled");
    assert_eq!(summary["blocking"]["reason"]["kind"], "lock_blocked");

    let machine_error = parse_machine_error_document(&stderr)?;
    assert_eq!(machine_error["version"], 1);
    Ok(())
}
