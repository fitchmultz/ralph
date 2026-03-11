//! `ralph machine` command group for versioned app-facing JSON contracts.
//!
//! Responsibilities:
//! - Define the first-class machine API consumed by the macOS app.
//! - Route machine requests into shared queue/task/config/run logic.
//! - Emit only versioned JSON documents or NDJSON event envelopes.
//!
//! Not handled here:
//! - Human-facing CLI rendering.
//! - Core queue/task/run business logic.
//! - App-side transport/retry behavior.
//!
//! Invariants/assumptions:
//! - Machine responses stay versioned and deterministic.
//! - Machine commands never emit human-oriented prose on stdout.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde::Serialize;
use serde_json::{Value as JsonValue, json};

use crate::agent;
use crate::commands::run::{RunEvent, RunEventHandler, RunOutcome};
use crate::commands::task as task_cmd;
use crate::commands::{cli_spec, doctor};
use crate::config;
use crate::contracts::*;
use crate::queue;
use crate::queue::graph::{
    build_graph, find_critical_paths, get_blocked_tasks, get_runnable_tasks,
};
use crate::queue::operations::{RunnableSelectionOptions, queue_runnability_report};
use crate::runner::OutputHandler;
use crate::template::load_template_with_context;
use crate::timeutil;

#[derive(Args)]
pub struct MachineArgs {
    #[command(subcommand)]
    pub command: MachineCommand,
}

#[derive(Subcommand)]
pub enum MachineCommand {
    System(MachineSystemArgs),
    Queue(MachineQueueArgs),
    Config(MachineConfigArgs),
    Task(MachineTaskArgs),
    Run(Box<MachineRunArgs>),
    Doctor(MachineDoctorArgs),
    CliSpec,
    Schema,
}

#[derive(Args)]
pub struct MachineSystemArgs {
    #[command(subcommand)]
    pub command: MachineSystemCommand,
}

#[derive(Subcommand)]
pub enum MachineSystemCommand {
    Info,
}

#[derive(Args)]
pub struct MachineQueueArgs {
    #[command(subcommand)]
    pub command: MachineQueueCommand,
}

#[derive(Subcommand)]
pub enum MachineQueueCommand {
    Read,
    Graph,
    Dashboard(MachineDashboardArgs),
    Validate,
}

#[derive(Args)]
pub struct MachineDashboardArgs {
    #[arg(long, default_value_t = 30)]
    pub days: u32,
}

#[derive(Args)]
pub struct MachineConfigArgs {
    #[command(subcommand)]
    pub command: MachineConfigCommand,
}

#[derive(Subcommand)]
pub enum MachineConfigCommand {
    Resolve,
}

#[derive(Args)]
pub struct MachineTaskArgs {
    #[command(subcommand)]
    pub command: MachineTaskCommand,
}

#[derive(Subcommand)]
pub enum MachineTaskCommand {
    Create(MachineTaskCreateArgs),
    Mutate(MachineTaskMutateArgs),
    Decompose(Box<MachineTaskDecomposeArgs>),
}

#[derive(Args)]
pub struct MachineTaskCreateArgs {
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,
}

#[derive(Args)]
pub struct MachineTaskMutateArgs {
    #[arg(long, value_name = "PATH")]
    pub input: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct MachineTaskDecomposeArgs {
    pub source: Vec<String>,
    #[arg(long)]
    pub attach_to: Option<String>,
    #[arg(long, default_value_t = 3)]
    pub max_depth: u8,
    #[arg(long, default_value_t = 5)]
    pub max_children: u8,
    #[arg(long, default_value_t = 50)]
    pub max_nodes: u8,
    #[arg(long, default_value = "draft")]
    pub status: String,
    #[arg(long, default_value = "fail")]
    pub child_policy: String,
    #[arg(long)]
    pub with_dependencies: bool,
    #[arg(long)]
    pub write: bool,
    #[command(flatten)]
    pub agent: agent::AgentArgs,
}

#[derive(Args)]
pub struct MachineRunArgs {
    #[command(subcommand)]
    pub command: MachineRunCommand,
}

#[derive(Subcommand)]
pub enum MachineRunCommand {
    One(MachineRunOneArgs),
    Loop(MachineRunLoopArgs),
    ParallelStatus,
}

#[derive(Args)]
pub struct MachineRunOneArgs {
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub agent: agent::RunAgentArgs,
}

#[derive(Args)]
pub struct MachineRunLoopArgs {
    #[arg(long, default_value_t = 0)]
    pub max_tasks: u32,
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub agent: agent::RunAgentArgs,
}

#[derive(Args)]
pub struct MachineDoctorArgs {
    #[command(subcommand)]
    pub command: MachineDoctorCommand,
}

#[derive(Subcommand)]
pub enum MachineDoctorCommand {
    Report,
}

pub fn handle_machine(args: MachineArgs, force: bool) -> Result<()> {
    match args.command {
        MachineCommand::System(args) => match args.command {
            MachineSystemCommand::Info => print_json(&MachineSystemInfoDocument {
                version: MACHINE_SYSTEM_INFO_VERSION,
                cli_version: env!("CARGO_PKG_VERSION").to_string(),
            }),
        },
        MachineCommand::Queue(args) => handle_queue(args),
        MachineCommand::Config(args) => match args.command {
            MachineConfigCommand::Resolve => {
                let resolved = config::resolve_from_cwd()?;
                print_json(&MachineConfigResolveDocument {
                    version: MACHINE_CONFIG_RESOLVE_VERSION,
                    paths: queue_paths(&resolved),
                    config: resolved.config.clone(),
                })
            }
        },
        MachineCommand::Task(args) => handle_task(args, force),
        MachineCommand::Run(args) => handle_run(*args),
        MachineCommand::Doctor(args) => match args.command {
            MachineDoctorCommand::Report => {
                let resolved = config::resolve_from_cwd_for_doctor()?;
                let report = doctor::run_doctor(&resolved, false)?;
                print_json(&MachineDoctorReportDocument {
                    version: MACHINE_DOCTOR_REPORT_VERSION,
                    report: serde_json::to_value(report)?,
                })
            }
        },
        MachineCommand::CliSpec => print_json(&MachineCliSpecDocument {
            version: MACHINE_CLI_SPEC_VERSION,
            spec: cli_spec::build_cli_spec(),
        }),
        MachineCommand::Schema => print_json(&json!({
            "system_info": schema_for!(MachineSystemInfoDocument),
            "queue_read": schema_for!(MachineQueueReadDocument),
            "config_resolve": schema_for!(MachineConfigResolveDocument),
            "task_create_request": schema_for!(MachineTaskCreateRequest),
            "task_create": schema_for!(MachineTaskCreateDocument),
            "task_mutation": schema_for!(MachineTaskMutationDocument),
            "graph_read": schema_for!(MachineGraphReadDocument),
            "dashboard_read": schema_for!(MachineDashboardReadDocument),
            "decompose": schema_for!(MachineDecomposeDocument),
            "doctor_report": schema_for!(MachineDoctorReportDocument),
            "parallel_status": schema_for!(MachineParallelStatusDocument),
            "cli_spec": schema_for!(MachineCliSpecDocument),
            "run_event": schema_for!(MachineRunEventEnvelope),
            "run_summary": schema_for!(MachineRunSummaryDocument),
        })),
    }
}

fn handle_queue(args: MachineQueueArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match args.command {
        MachineQueueCommand::Read => {
            let active = queue::load_queue(&resolved.queue_path)?;
            let done = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done)
            };
            let options = RunnableSelectionOptions::new(false, true);
            let runnability = queue_runnability_report(&active, done_ref, options)?;
            let next_runnable_task_id = queue::operations::next_runnable_task(&active, done_ref)
                .map(|task| task.id.clone());
            print_json(&MachineQueueReadDocument {
                version: MACHINE_QUEUE_READ_VERSION,
                paths: queue_paths(&resolved),
                active,
                done,
                next_runnable_task_id,
                runnability: serde_json::to_value(runnability)?,
            })
        }
        MachineQueueCommand::Graph => {
            let (active, done) = crate::cli::load_and_validate_queues_read_only(&resolved, true)?;
            let done_ref = done
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let graph = build_graph(&active, done_ref);
            let critical = find_critical_paths(&graph);
            print_json(&MachineGraphReadDocument {
                version: MACHINE_GRAPH_READ_VERSION,
                graph: build_graph_json(&graph, &critical)?,
            })
        }
        MachineQueueCommand::Dashboard(args) => {
            let (active, done) = crate::cli::load_and_validate_queues_read_only(&resolved, true)?;
            let done_ref = done
                .as_ref()
                .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
            let cache_dir = resolved.repo_root.join(".ralph/cache");
            let productivity = crate::productivity::load_productivity_stats(&cache_dir).ok();
            let dashboard = crate::reports::build_dashboard_report(
                &active,
                done_ref,
                productivity.as_ref(),
                args.days,
            );
            print_json(&MachineDashboardReadDocument {
                version: MACHINE_DASHBOARD_READ_VERSION,
                dashboard: serde_json::to_value(dashboard)?,
            })
        }
        MachineQueueCommand::Validate => {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done_file)
            };
            let warnings = queue::validate_queue_set(
                &queue_file,
                done_ref,
                &resolved.id_prefix,
                resolved.id_width,
                resolved.config.queue.max_dependency_depth.unwrap_or(10),
            )?;
            let warning_values = warnings
                .into_iter()
                .map(|warning| {
                    json!({
                        "task_id": warning.task_id,
                        "message": warning.message,
                    })
                })
                .collect::<Vec<_>>();
            print_json(&json!({
                "version": 1,
                "valid": true,
                "warnings": warning_values,
            }))
        }
    }
}

fn handle_task(args: MachineTaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
    match args.command {
        MachineTaskCommand::Create(args) => {
            let raw = read_json_input(args.input.as_deref())?;
            let request: MachineTaskCreateRequest =
                serde_json::from_str(&raw).context("parse machine task create request")?;
            let task = create_task(&resolved, &request, force)?;
            print_json(&MachineTaskCreateDocument {
                version: MACHINE_TASK_CREATE_VERSION,
                task,
            })
        }
        MachineTaskCommand::Mutate(args) => {
            let raw = read_json_input(args.input.as_deref())?;
            let request = serde_json::from_str::<queue::operations::TaskMutationRequest>(&raw)
                .context("parse machine task mutation request")?;

            let _queue_lock =
                queue::acquire_queue_lock(&resolved.repo_root, "machine task mutate", force)?;
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let done_file = queue::load_queue_or_default(&resolved.done_path)?;
            let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
                None
            } else {
                Some(&done_file)
            };
            let now = timeutil::now_utc_rfc3339()?;
            let mut working = queue_file.clone();
            let report = queue::operations::apply_task_mutation_request(
                &mut working,
                done_ref,
                &request,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                resolved.config.queue.max_dependency_depth.unwrap_or(10),
            )?;
            if !args.dry_run {
                crate::undo::create_undo_snapshot(
                    &resolved,
                    &format!("machine task mutate [{} task(s)]", report.tasks.len()),
                )?;
                queue::save_queue(&resolved.queue_path, &working)?;
            }
            print_json(&MachineTaskMutationDocument {
                version: MACHINE_TASK_MUTATION_VERSION,
                report: serde_json::to_value(report)?,
            })
        }
        MachineTaskCommand::Decompose(args) => {
            let source_input = task_cmd::read_request_from_args_or_stdin(&args.source)?;
            let overrides = agent::resolve_agent_overrides(&args.agent)?;
            let status = parse_task_status(&args.status)?;
            let child_policy = parse_child_policy(&args.child_policy)?;
            let preview = task_cmd::plan_task_decomposition(
                &resolved,
                &task_cmd::TaskDecomposeOptions {
                    source_input,
                    attach_to_task_id: args.attach_to,
                    max_depth: args.max_depth,
                    max_children: usize::from(args.max_children),
                    max_nodes: usize::from(args.max_nodes),
                    status,
                    child_policy,
                    with_dependencies: args.with_dependencies,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    repoprompt_tool_injection: agent::resolve_rp_required(
                        args.agent.repo_prompt,
                        &resolved,
                    ),
                },
            )?;
            let write = if args.write {
                Some(task_cmd::write_task_decomposition(
                    &resolved, &preview, force,
                )?)
            } else {
                None
            };
            print_json(&MachineDecomposeDocument {
                version: MACHINE_DECOMPOSE_VERSION,
                result: json!({
                    "version": 1,
                    "mode": if write.is_some() { "write" } else { "preview" },
                    "preview": preview,
                    "write": write,
                }),
            })
        }
    }
}

fn handle_run(args: MachineRunArgs) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;
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
                    "config": MachineConfigResolveDocument {
                        version: MACHINE_CONFIG_RESOLVE_VERSION,
                        paths: queue_paths(&resolved),
                        config: resolved.config.clone(),
                    }
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
            let result = crate::commands::run::run_loop(
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
            );
            result?;
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
            print_json(&MachineParallelStatusDocument {
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
        Err(err) => {
            emit_run_event(MachineRunEventEnvelope {
                version: MACHINE_RUN_EVENT_VERSION,
                kind: MachineRunEventKind::Warning,
                timestamp: timeutil::now_utc_rfc3339_or_fallback(),
                run_mode: Some("one".to_string()),
                task_id: None,
                phase: None,
                exit_code: Some(1),
                message: Some(format!("{err:#}")),
                stream: None,
                payload: None,
            })?;
            bail!("{err:#}")
        }
    }
}

fn emit_run_event(event: MachineRunEventEnvelope) -> Result<()> {
    println!("{}", serde_json::to_string(&event)?);
    Ok(())
}

fn create_task(
    resolved: &config::Resolved,
    request: &MachineTaskCreateRequest,
    force: bool,
) -> Result<Task> {
    if request.version != MACHINE_TASK_CREATE_VERSION {
        bail!(
            "Unsupported machine task create request version {}",
            request.version
        );
    }

    if request.title.trim().is_empty() {
        bail!("Task title cannot be empty");
    }

    let queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "machine task create", force)?;
    let active = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let predicted_id = queue::next_id_across(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        resolved.config.queue.max_dependency_depth.unwrap_or(10),
    )?;

    if let Some(template) = &request.template {
        let _loaded = load_template_with_context(
            template,
            &resolved.repo_root,
            request.target.as_deref(),
            false,
        )?;
        let opts = task_cmd::TaskBuildOptions {
            request: request.title.clone(),
            hint_tags: request.tags.join(","),
            hint_scope: request.scope.join(","),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: RunnerCliOptionsPatch::default(),
            force,
            repoprompt_tool_injection: false,
            template_hint: Some(template.clone()),
            template_target: request.target.clone(),
            strict_templates: false,
            estimated_minutes: None,
        };
        drop(queue_lock);
        task_cmd::build_task(resolved, opts)?;
        let queue_after = queue::load_queue(&resolved.queue_path)?;
        return queue_after
            .tasks
            .into_iter()
            .find(|task| task.id == predicted_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Created template task {} not found after write",
                    predicted_id
                )
            });
    }

    let now = timeutil::now_utc_rfc3339()?;
    let priority = request.priority.parse::<crate::contracts::TaskPriority>()?;
    let task = Task {
        id: predicted_id,
        status: TaskStatus::Todo,
        title: request.title.trim().to_string(),
        description: request
            .description
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        priority,
        tags: request.tags.clone(),
        scope: request.scope.clone(),
        evidence: Vec::new(),
        plan: Vec::new(),
        notes: Vec::new(),
        request: None,
        agent: None,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        estimated_minutes: None,
        actual_minutes: None,
        depends_on: Vec::new(),
        blocks: Vec::new(),
        relates_to: Vec::new(),
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    };

    let mut working = active;
    working.tasks.push(task.clone());
    crate::undo::create_undo_snapshot(resolved, &format!("machine task create [{}]", task.id))?;
    queue::save_queue(&resolved.queue_path, &working)?;
    Ok(task)
}

fn build_graph_json(
    graph: &crate::queue::graph::DependencyGraph,
    critical_paths: &[crate::queue::graph::CriticalPathResult],
) -> Result<JsonValue> {
    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);
    let mut tasks = graph
        .task_ids()
        .filter_map(|id| graph.get(id))
        .map(|node| {
            let mut dependencies = node.dependencies.clone();
            dependencies.sort_unstable();
            let mut dependents = node.dependents.clone();
            dependents.sort_unstable();
            json!({
                "id": node.task.id,
                "title": node.task.title,
                "status": node.task.status.as_str(),
                "dependencies": dependencies,
                "dependents": dependents,
                "critical": graph.is_on_critical_path(&node.task.id, critical_paths),
            })
        })
        .collect::<Vec<_>>();
    tasks.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));

    let mut critical_paths_json = critical_paths
        .iter()
        .map(|path| {
            json!({
                "path": path.path,
                "length": path.length,
                "blocked": path.is_blocked,
            })
        })
        .collect::<Vec<_>>();
    critical_paths_json
        .sort_by(|left, right| left["path"].to_string().cmp(&right["path"].to_string()));

    Ok(json!({
        "summary": {
            "total_tasks": graph.len(),
            "runnable_tasks": runnable.len(),
            "blocked_tasks": blocked.len(),
        },
        "critical_paths": critical_paths_json,
        "tasks": tasks,
    }))
}

fn queue_paths(resolved: &config::Resolved) -> MachineQueuePaths {
    MachineQueuePaths {
        repo_root: resolved.repo_root.display().to_string(),
        queue_path: resolved.queue_path.display().to_string(),
        done_path: resolved.done_path.display().to_string(),
        project_config_path: resolved
            .project_config_path
            .as_ref()
            .map(|path| path.display().to_string()),
        global_config_path: resolved
            .global_config_path
            .as_ref()
            .map(|path| path.display().to_string()),
    }
}

fn parse_task_status(value: &str) -> Result<TaskStatus> {
    match value.trim().to_ascii_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        other => bail!("Unsupported task status '{}'", other),
    }
}

fn parse_child_policy(value: &str) -> Result<task_cmd::DecompositionChildPolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fail" => Ok(task_cmd::DecompositionChildPolicy::Fail),
        "append" => Ok(task_cmd::DecompositionChildPolicy::Append),
        "replace" => Ok(task_cmd::DecompositionChildPolicy::Replace),
        other => bail!("Unsupported decomposition child policy '{}'", other),
    }
}

fn read_json_input(path: Option<&str>) -> Result<String> {
    if let Some(path) = path {
        return std::fs::read_to_string(path)
            .with_context(|| format!("read JSON input from {}", path));
    }

    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("read JSON input from stdin")?;
    if raw.trim().is_empty() {
        bail!("JSON input is empty")
    }
    Ok(raw)
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_json_line<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}
