//! Top-level routing for `ralph machine`.
//!
//! Responsibilities:
//! - Dispatch machine subcommands to focused handlers.
//! - Keep schema/config/system/doctor branches centralized and thin.
//! - Emit only versioned JSON machine documents on stdout.
//!
//! Not handled here:
//! - Machine queue/task/run business logic.
//! - Machine contract type definitions.
//! - Human-facing CLI rendering.
//!
//! Invariants/assumptions:
//! - Machine routing never emits prose on stdout.
//! - Schema output stays aligned with the machine contract types.

use anyhow::Result;
use schemars::schema_for;
use serde_json::json;

use crate::cli::machine::args::{
    MachineArgs, MachineCommand, MachineConfigCommand, MachineDoctorCommand, MachineSystemCommand,
};
use crate::cli::machine::common::build_config_resolve_document;
use crate::cli::machine::io::print_json;
use crate::cli::machine::{queue, run, task};
use crate::commands::{cli_spec, doctor};
use crate::config;
use crate::contracts::{
    MACHINE_CLI_SPEC_VERSION, MACHINE_DOCTOR_REPORT_VERSION, MACHINE_SYSTEM_INFO_VERSION,
    MachineCliSpecDocument, MachineConfigResolveDocument, MachineDashboardReadDocument,
    MachineDecomposeDocument, MachineDoctorReportDocument, MachineGraphReadDocument,
    MachineParallelStatusDocument, MachineQueueReadDocument, MachineRunEventEnvelope,
    MachineRunSummaryDocument, MachineSystemInfoDocument, MachineTaskCreateDocument,
    MachineTaskCreateRequest, MachineTaskMutationDocument,
};

pub fn handle_machine(args: MachineArgs, force: bool) -> Result<()> {
    match args.command {
        MachineCommand::System(args) => match args.command {
            MachineSystemCommand::Info => print_json(&MachineSystemInfoDocument {
                version: MACHINE_SYSTEM_INFO_VERSION,
                cli_version: env!("CARGO_PKG_VERSION").to_string(),
            }),
        },
        MachineCommand::Queue(args) => queue::handle_queue(args),
        MachineCommand::Config(args) => match args.command {
            MachineConfigCommand::Resolve => {
                let resolved = config::resolve_from_cwd()?;
                let repo_trust = config::load_repo_trust(&resolved.repo_root)?;
                let dirty_repo = crate::git::status_porcelain(&resolved.repo_root)
                    .map(|status| !status.trim().is_empty())
                    .unwrap_or(false);
                print_json(&build_config_resolve_document(
                    &resolved,
                    repo_trust.is_trusted(),
                    dirty_repo,
                ))
            }
        },
        MachineCommand::Task(args) => task::handle_task(args, force),
        MachineCommand::Run(args) => run::handle_run(*args),
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
