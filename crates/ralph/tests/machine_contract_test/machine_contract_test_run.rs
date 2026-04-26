//! Run-surface contract coverage for `ralph machine`.
//!
//! Purpose:
//! - Verify `ralph machine run` contracts for canonical task selection and loop terminal summaries.
//!
//! Responsibilities:
//! - Assert no-ID `machine run one --resume` emits `run_started` without a task ID.
//! - Verify `task_selected` and the final summary expose the actual CLI-selected task.
//! - Assert `machine run loop` preserves idle, blocked, and stalled terminal summaries.
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
use ralph::session::save_session;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

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
