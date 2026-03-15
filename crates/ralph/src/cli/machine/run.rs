//! Run-oriented machine command handlers.
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
//! Invariants/assumptions:
//! - Machine run streams stay NDJSON-only.
//! - Event ordering matches runner and phase progression.
//! - One-off and loop run summaries preserve existing outcome strings.

use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::json;

use crate::agent;
use crate::cli::machine::args::{MachineRunArgs, MachineRunCommand};
use crate::cli::machine::common::build_config_resolve_document;
use crate::cli::machine::io::print_json_line;
use crate::commands::run::{RunEvent, RunEventHandler, RunOutcome};
use crate::contracts::{
    MACHINE_PARALLEL_STATUS_VERSION, MACHINE_RUN_EVENT_VERSION, MACHINE_RUN_SUMMARY_VERSION,
    MachineParallelStatusDocument, MachineRunEventEnvelope, MachineRunEventKind,
    MachineRunSummaryDocument,
};
use crate::runner::OutputHandler;
use crate::timeutil;

pub(super) fn handle_run(args: MachineRunArgs) -> Result<()> {
    let resolved = crate::config::resolve_from_cwd()?;
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
            let event_handler: RunEventHandler = Arc::new(Box::new(move |event: RunEvent| {
                let envelope = match event {
                    RunEvent::TaskSelected { task_id, title } => MachineRunEventEnvelope {
                        version: MACHINE_RUN_EVENT_VERSION,
                        kind: MachineRunEventKind::TaskSelected,
                        timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                        run_mode: Some("one".to_string()),
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
                        run_mode: Some("one".to_string()),
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
                        run_mode: Some("one".to_string()),
                        task_id: None,
                        phase: Some(phase.as_str().to_lowercase()),
                        exit_code: None,
                        message: None,
                        stream: None,
                        payload: None,
                    },
                };
                let _ = emit_run_event(envelope);
            })
                as Box<dyn Fn(RunEvent) + Send + Sync>);
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
                    "config": build_config_resolve_document(&resolved, false, false),
                })),
            })?;
            let result = if let Some(task_id) = args.id.as_deref() {
                crate::commands::run::run_one_with_id(
                    &resolved,
                    &overrides,
                    args.force,
                    task_id,
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
                    Some(output_handler),
                    Some(event_handler),
                )
            };
            emit_run_summary(result)
        }
        MachineRunCommand::Loop(args) => {
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
                payload: None,
            })?;
            let overrides = agent::resolve_run_agent_overrides(&args.agent)?;
            crate::commands::run::run_loop(
                &resolved,
                crate::commands::run::RunLoopOptions {
                    max_tasks: args.max_tasks,
                    agent_overrides: overrides,
                    force: args.force,
                    auto_resume: false,
                    starting_completed: 0,
                    non_interactive: true,
                    parallel_workers: None,
                    wait_when_blocked: false,
                    wait_poll_ms: 1000,
                    wait_timeout_seconds: 0,
                    notify_when_unblocked: false,
                    wait_when_empty: false,
                    empty_poll_ms: 30_000,
                },
            )?;
            print_json_line(&MachineRunSummaryDocument {
                version: MACHINE_RUN_SUMMARY_VERSION,
                task_id: None,
                exit_code: 0,
                outcome: "completed".to_string(),
            })
        }
        MachineRunCommand::ParallelStatus => {
            let state_path = crate::commands::run::state_file_path(&resolved.repo_root);
            let status = match crate::commands::run::load_state(&state_path)? {
                Some(state) => serde_json::to_value(state)?,
                None => json!({
                    "schema_version": 3,
                    "workers": [],
                    "message": "No parallel state found",
                }),
            };
            crate::cli::machine::io::print_json(&MachineParallelStatusDocument {
                version: MACHINE_PARALLEL_STATUS_VERSION,
                status,
            })
        }
    }
}

fn emit_run_summary(result: Result<RunOutcome>) -> Result<()> {
    match result {
        Ok(RunOutcome::Ran { task_id }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: Some(task_id),
            exit_code: 0,
            outcome: "ran".to_string(),
        }),
        Ok(RunOutcome::NoCandidates) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "no_candidates".to_string(),
        }),
        Ok(RunOutcome::Blocked { .. }) => print_json_line(&MachineRunSummaryDocument {
            version: MACHINE_RUN_SUMMARY_VERSION,
            task_id: None,
            exit_code: 0,
            outcome: "blocked".to_string(),
        }),
        Err(error) => {
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::Warning,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some("one".to_string()),
                task_id: None,
                phase: None,
                exit_code: Some(1),
                message: Some(format!("{error:#}")),
                stream: None,
                payload: None,
            })?;
            bail!("{error:#}")
        }
    }
}

fn emit_run_event(event: MachineRunEventEnvelope) -> Result<()> {
    print_json_line(&event)
}
