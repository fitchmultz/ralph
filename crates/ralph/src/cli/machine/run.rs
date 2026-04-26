//! Run-oriented machine command handlers.
//!
//! Purpose:
//! - Run-oriented machine command handlers.
//!
//! Responsibilities:
//! - Implement `ralph machine run ...` operations.
//! - Emit NDJSON run events and final machine run summaries.
//! - Bridge machine run requests into the shared run command layer.
//!
//! Not handled here:
//! - Queue/task/config command routing.
//! - Clap argument definitions.
//! - Human-facing CLI rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Machine run streams stay NDJSON-only.
//! - Event ordering matches runner and phase progression.
//! - One-off and loop run summaries preserve existing outcome strings.

use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::json;

use crate::agent;
use crate::cli::machine::args::{MachineRunArgs, MachineRunCommand};
use crate::cli::machine::common::{
    build_config_resolve_document, build_resume_preview, machine_resume_decision_from_runtime,
    machine_run_loop_command, machine_run_parallel_status_command, machine_run_stop_command,
    machine_safety_context, queue_paths,
};
use crate::cli::machine::io::{print_json, print_json_line};
use crate::commands::run::{
    RunEvent, RunEventHandler, RunLoopOutcome, RunOneResumeOptions, RunOutcome,
};
use crate::contracts::{
    BlockingState, MACHINE_RUN_EVENT_VERSION, MACHINE_RUN_STOP_VERSION,
    MACHINE_RUN_SUMMARY_VERSION, MachineContinuationAction, MachineContinuationSummary,
    MachineRunEventEnvelope, MachineRunEventKind, MachineRunStopAction, MachineRunStopDocument,
    MachineRunStopMarker, MachineRunSummaryDocument,
};
use crate::runner::OutputHandler;
use crate::timeutil;

pub(super) fn handle_run(args: MachineRunArgs) -> Result<()> {
    let resolved = crate::config::resolve_from_cwd()?;
    let (repo_trusted, dirty_repo) = machine_safety_context(&resolved)?;
    match args.command {
        MachineRunCommand::One(args) => {
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;
            let stdout_emitter = Arc::new(Box::new(move |chunk: &str| {
                let _ = emit_run_event(MachineRunEventEnvelope {
                    version: MACHINE_RUN_EVENT_VERSION,
                    kind: MachineRunEventKind::RunnerOutput,
                    timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                    run_mode: Some("one".to_string()),
                    task_id: None,
                    phase: None,
                    exit_code: None,
                    message: None,
                    stream: Some("combined".to_string()),
                    payload: Some(json!({ "text": chunk })),
                });
            }) as Box<dyn Fn(&str) + Send + Sync>);
            let output_handler: OutputHandler = stdout_emitter;
            let event_handler = build_run_event_handler("one");
            let resume_preview =
                build_resume_preview(&resolved, args.id.as_deref(), args.resume, true, false)?;
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::RunStarted,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some("one".to_string()),
                task_id: args.id.clone(),
                phase: None,
                exit_code: None,
                message: None,
                stream: None,
                payload: Some(json!({
                    "config": build_config_resolve_document(
                        &resolved,
                        repo_trusted,
                        dirty_repo,
                        resume_preview
                    ),
                })),
            })?;
            let resume_options = RunOneResumeOptions::detect(args.resume, true);
            let result = if let Some(task_id) = args.id.as_deref() {
                crate::commands::run::run_one_with_id(
                    &resolved,
                    &overrides,
                    args.force,
                    task_id,
                    resume_options,
                    Some(output_handler),
                    Some(event_handler),
                    None,
                )
                .map(|_| RunOutcome::Ran {
                    task_id: task_id.to_string(),
                })
            } else {
                crate::commands::run::run_one_with_handlers(
                    &resolved,
                    &overrides,
                    args.force,
                    resume_options,
                    Some(output_handler),
                    Some(event_handler),
                )
            };
            emit_run_summary(&resolved, "one", result)
        }
        MachineRunCommand::Loop(args) => {
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;
            let event_handler = build_run_event_handler("loop");
            let resume_preview =
                build_resume_preview(&resolved, None, args.resume, true, args.resume)?;
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::RunStarted,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some("loop".to_string()),
                task_id: None,
                phase: None,
                exit_code: None,
                message: None,
                stream: None,
                payload: Some(json!({
                    "config": build_config_resolve_document(
                        &resolved,
                        repo_trusted,
                        dirty_repo,
                        resume_preview
                    ),
                })),
            })?;
            let result = crate::commands::run::run_loop(
                &resolved,
                crate::commands::run::RunLoopOptions {
                    max_tasks: args.max_tasks,
                    agent_overrides: overrides,
                    force: args.force,
                    auto_resume: args.resume,
                    starting_completed: 0,
                    non_interactive: true,
                    parallel_workers: args.parallel,
                    wait_when_blocked: false,
                    wait_poll_ms: 1000,
                    wait_timeout_seconds: 0,
                    notify_when_unblocked: false,
                    wait_when_empty: false,
                    empty_poll_ms: 30_000,
                    run_event_handler: Some(event_handler),
                },
            );
            emit_loop_run_summary(&resolved, result)
        }
        MachineRunCommand::Stop(args) => {
            print_json(&build_run_stop_document(&resolved, args.dry_run)?)
        }
        MachineRunCommand::ParallelStatus => {
            let state_path = crate::commands::run::state_file_path(&resolved.repo_root);
            let state = crate::commands::run::load_state(&state_path)?;
            let document = crate::commands::run::build_parallel_status_document(
                &resolved.repo_root,
                state.as_ref(),
            )?;
            crate::cli::machine::io::print_json(&document)
        }
    }
}

fn build_run_stop_document(
    resolved: &crate::config::Resolved,
    dry_run: bool,
) -> Result<MachineRunStopDocument> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let before = crate::signal::stop_signal_snapshot(&cache_dir);
    let (action, marker) = if dry_run {
        let action = if before.exists {
            MachineRunStopAction::AlreadyPresent
        } else {
            MachineRunStopAction::WouldCreate
        };
        (
            action,
            MachineRunStopMarker {
                path: before.path.display().to_string(),
                existed_before: before.exists,
                exists_after: before.exists,
            },
        )
    } else {
        let requested = crate::signal::request_stop_signal(&cache_dir)?;
        let action = if requested.existed_before {
            MachineRunStopAction::AlreadyPresent
        } else {
            MachineRunStopAction::Created
        };
        (
            action,
            MachineRunStopMarker {
                path: requested.path.display().to_string(),
                existed_before: requested.existed_before,
                exists_after: requested.exists_after,
            },
        )
    };

    let context = current_stop_continuation_context(resolved);
    let continuation = build_run_stop_continuation(dry_run, &action, &context);

    Ok(MachineRunStopDocument {
        version: MACHINE_RUN_STOP_VERSION,
        dry_run,
        action,
        paths: queue_paths(resolved),
        marker,
        blocking: context.blocking.clone(),
        continuation,
    })
}

#[derive(Debug, Clone)]
struct StopContinuationContext {
    parallel_active: bool,
    blocking: Option<BlockingState>,
}

fn current_stop_continuation_context(
    resolved: &crate::config::Resolved,
) -> StopContinuationContext {
    let state_path = crate::commands::run::state_file_path(&resolved.repo_root);
    let state = crate::commands::run::load_state(&state_path).ok().flatten();
    let parallel_active = state.is_some();
    let blocking = state.as_ref().and_then(|parallel_state| {
        crate::commands::run::build_parallel_status_document(
            &resolved.repo_root,
            Some(parallel_state),
        )
        .ok()
        .and_then(|document| document.blocking)
    });

    StopContinuationContext {
        parallel_active,
        blocking,
    }
}

fn build_run_stop_continuation(
    dry_run: bool,
    action: &MachineRunStopAction,
    context: &StopContinuationContext,
) -> MachineContinuationSummary {
    let headline = match (dry_run, action) {
        (true, MachineRunStopAction::WouldCreate) => "Stop request would be recorded.",
        (true, MachineRunStopAction::Created) => "Stop request would be recorded.",
        (true, MachineRunStopAction::AlreadyPresent) => "Stop request is already recorded.",
        (false, MachineRunStopAction::Created) => "Stop request recorded.",
        (false, MachineRunStopAction::AlreadyPresent) => "Stop request is already recorded.",
        (false, MachineRunStopAction::WouldCreate) => "Stop request recorded.",
    };

    let detail = match (dry_run, action, context.parallel_active) {
        (true, MachineRunStopAction::WouldCreate, true) => {
            "Ralph would create the stop marker and let parallel workers finish in-flight work before the loop exits."
        }
        (true, MachineRunStopAction::WouldCreate, false) => {
            "Ralph would create the stop marker and exit after the current task completes."
        }
        (_, MachineRunStopAction::AlreadyPresent, true) => {
            "A stop marker already exists, so Ralph should stop scheduling new parallel work and wait for in-flight tasks to finish."
        }
        (_, MachineRunStopAction::AlreadyPresent, false) => {
            "A stop marker already exists, so Ralph should exit after the current task completes."
        }
        (_, _, true) => {
            "The stop marker is recorded. Ralph should stop scheduling new parallel work and wait for in-flight tasks to finish."
        }
        (_, _, false) => {
            "The stop marker is recorded. Ralph should exit after the current task completes."
        }
    }
    .to_string();

    let mut next_steps = Vec::new();
    if context.parallel_active {
        next_steps.push(MachineContinuationAction {
            title: "Inspect parallel status".to_string(),
            command: machine_run_parallel_status_command().to_string(),
            detail: "Check whether workers are still running, blocked, or finishing integration."
                .to_string(),
        });
    }
    next_steps.push(MachineContinuationAction {
        title: "Resume run-control inspection".to_string(),
        command: machine_run_loop_command(context.parallel_active, false).to_string(),
        detail: "Use the machine loop surface to keep monitoring the run until the stop request is honored."
            .to_string(),
    });
    if dry_run {
        next_steps.push(MachineContinuationAction {
            title: "Record the stop request".to_string(),
            command: machine_run_stop_command(false).to_string(),
            detail: "Apply the stop marker when the operator confirms the loop should stop after the current work."
                .to_string(),
        });
    }

    MachineContinuationSummary {
        headline: headline.to_string(),
        detail,
        blocking: context.blocking.clone(),
        next_steps,
    }
}

fn build_run_event_handler(run_mode: &'static str) -> RunEventHandler {
    Arc::new(Box::new(move |event: RunEvent| {
        let envelope = machine_event_envelope(run_mode, event);
        let _ = emit_run_event(envelope);
    }) as Box<dyn Fn(RunEvent) + Send + Sync>)
}

fn machine_event_envelope(run_mode: &'static str, event: RunEvent) -> MachineRunEventEnvelope {
    match event {
        RunEvent::ResumeDecision { decision } => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::ResumeDecision,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: decision.task_id.clone(),
            phase: None,
            exit_code: None,
            message: Some(decision.message.clone()),
            stream: None,
            payload: Some(
                serde_json::to_value(machine_resume_decision_from_runtime(&decision))
                    .expect("resume decision serializes"),
            ),
        },
        RunEvent::TaskSelected { task_id, title } => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::TaskSelected,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: Some(task_id),
            phase: None,
            exit_code: None,
            message: Some(title),
            stream: None,
            payload: None,
        },
        RunEvent::PhaseEntered { phase } => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::PhaseEntered,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: None,
            phase: Some(phase.as_str().to_lowercase()),
            exit_code: None,
            message: None,
            stream: None,
            payload: None,
        },
        RunEvent::PhaseCompleted { phase } => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::PhaseCompleted,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: None,
            phase: Some(phase.as_str().to_lowercase()),
            exit_code: None,
            message: None,
            stream: None,
            payload: None,
        },
        RunEvent::BlockedStateChanged { state } => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::BlockedStateChanged,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: state.task_id.clone(),
            phase: None,
            exit_code: None,
            message: Some(state.message.clone()),
            stream: None,
            payload: Some(serde_json::to_value(state).expect("blocking state serializes")),
        },
        RunEvent::BlockedStateCleared => MachineRunEventEnvelope {
            version: MACHINE_RUN_EVENT_VERSION,
            kind: MachineRunEventKind::BlockedStateCleared,
            timestamp: timeutil::now_utc_rfc3339_or_fallback(),
            run_mode: Some(run_mode.to_string()),
            task_id: None,
            phase: None,
            exit_code: None,
            message: Some("blocking state cleared".to_string()),
            stream: None,
            payload: None,
        },
    }
}

fn emit_run_summary(
    resolved: &crate::config::Resolved,
    run_mode: &'static str,
    result: Result<RunOutcome>,
) -> Result<()> {
    match result {
        Ok(RunOutcome::Ran { task_id }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: Some(task_id),
            exit_code: 0,
            outcome: "ran".to_string(),
            blocking: None,
        }),
        Ok(RunOutcome::NoCandidates) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "no_candidates".to_string(),
            blocking: Some(
                crate::contracts::BlockingState::idle(false)
                    .with_observed_at(timeutil::now_utc_rfc3339_or_fallback()),
            ),
        }),
        Ok(RunOutcome::Blocked { state, .. }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "blocked".to_string(),
            blocking: Some(*state),
        }),
        Err(error) => {
            let blocking =
                crate::commands::run::queue_lock_blocking_state(&resolved.repo_root, &error)
                    .or_else(|| {
                        error
                            .downcast_ref::<crate::commands::run::CiFailure>()
                            .map(|failure| failure.blocking_state())
                    });
            if let Some(state) = blocking.as_ref() {
                emit_run_event(MachineRunEventEnvelope {
                    version: MACHINE_RUN_EVENT_VERSION,
                    kind: MachineRunEventKind::BlockedStateChanged,
                    timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                    run_mode: Some(run_mode.to_string()),
                    task_id: state.task_id.clone(),
                    phase: None,
                    exit_code: Some(1),
                    message: Some(state.message.clone()),
                    stream: None,
                    payload: Some(serde_json::to_value(state)?),
                })?;
            }
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::Warning,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some(run_mode.to_string()),
                task_id: None,
                phase: None,
                exit_code: Some(1),
                message: Some(format!("{error:#}")),
                stream: None,
                payload: None,
            })?;
            print_json_line(&MachineRunSummaryDocument {
                version: MACHINE_RUN_SUMMARY_VERSION,
                task_id: None,
                exit_code: 1,
                outcome: if blocking.is_some() {
                    "stalled".to_string()
                } else {
                    "failed".to_string()
                },
                blocking,
            })?;
            bail!("{error:#}")
        }
    }
}

fn emit_loop_run_summary(
    resolved: &crate::config::Resolved,
    result: Result<RunLoopOutcome>,
) -> Result<()> {
    match result {
        Ok(RunLoopOutcome::Completed) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "completed".to_string(),
            blocking: None,
        }),
        Ok(RunLoopOutcome::NoCandidates { blocking }) => {
            print_json_line(&MachineRunSummaryDocument {
                version: MACHINE_RUN_SUMMARY_VERSION,
                task_id: None,
                exit_code: 0,
                outcome: "no_candidates".to_string(),
                blocking: Some(*blocking),
            })
        }
        Ok(RunLoopOutcome::Blocked { summary, blocking }) => {
            log::debug!(
                "machine loop summary blocked (ready={} deps={} sched={})",
                summary.runnable_candidates,
                summary.blocked_by_dependencies,
                summary.blocked_by_schedule
            );
            print_json_line(&MachineRunSummaryDocument {
                version: MACHINE_RUN_SUMMARY_VERSION,
                task_id: None,
                exit_code: 0,
                outcome: "blocked".to_string(),
                blocking: Some(*blocking),
            })
        }
        Ok(RunLoopOutcome::Stalled { blocking }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "stalled".to_string(),
            blocking: Some(*blocking),
        }),
        Ok(RunLoopOutcome::Stopped { blocking }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "stopped".to_string(),
            blocking: blocking.map(|state| *state),
        }),
        Err(error) => {
            let blocking =
                crate::commands::run::queue_lock_blocking_state(&resolved.repo_root, &error)
                    .or_else(|| {
                        error
                            .downcast_ref::<crate::commands::run::CiFailure>()
                            .map(|failure| failure.blocking_state())
                    });
            if let Some(state) = blocking.as_ref() {
                emit_run_event(MachineRunEventEnvelope {
                    version: MACHINE_RUN_EVENT_VERSION,
                    kind: MachineRunEventKind::BlockedStateChanged,
                    timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                    run_mode: Some("loop".to_string()),
                    task_id: state.task_id.clone(),
                    phase: None,
                    exit_code: Some(1),
                    message: Some(state.message.clone()),
                    stream: None,
                    payload: Some(serde_json::to_value(state)?),
                })?;
            }
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::Warning,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some("loop".to_string()),
                task_id: None,
                phase: None,
                exit_code: Some(1),
                message: Some(format!("{error:#}")),
                stream: None,
                payload: None,
            })?;
            print_json_line(&MachineRunSummaryDocument {
                version: MACHINE_RUN_SUMMARY_VERSION,
                task_id: None,
                exit_code: 1,
                outcome: if blocking.is_some() {
                    "stalled".to_string()
                } else {
                    "failed".to_string()
                },
                blocking,
            })?;
            bail!("{error:#}")
        }
    }
}

fn emit_run_event(event: MachineRunEventEnvelope) -> Result<()> {
    print_json_line(&event)
}
