//! Task CLI routing.
//!
//! Purpose:
//! - Task CLI routing.
//!
//! Responsibilities:
//! - Resolve config and dispatch `ralph task` subcommands to their handlers.
//! - Keep the task facade focused on re-exports and module wiring.
//!
//! Not handled here:
//! - Clap argument definitions.
//! - Queue persistence internals.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Config resolution always uses the current working directory.

use anyhow::Result;

use crate::config;

use super::{
    TaskArgs, TaskCommand, TaskFromCommand, batch, build, children, clone, decompose, edit,
    followups, from_template, mutate, parent, refactor, relations, schedule, show, split, start,
    status, template,
};

pub fn handle_task(args: TaskArgs, force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    match args.command {
        Some(TaskCommand::Ready(args)) => status::handle_ready(&args, force, &resolved),
        Some(TaskCommand::Status(args)) => status::handle_status(&args, force, &resolved),
        Some(TaskCommand::Done(args)) => status::handle_done(&args, force, &resolved),
        Some(TaskCommand::Reject(args)) => status::handle_reject(&args, force, &resolved),
        Some(TaskCommand::Field(args)) => edit::handle_field(&args, force, &resolved),
        Some(TaskCommand::Edit(args)) => edit::handle_edit(&args, force, &resolved),
        Some(TaskCommand::Mutate(args)) => mutate::handle(&args, force, &resolved),
        Some(TaskCommand::Followups(args)) => followups::handle(&args, force, &resolved),
        Some(TaskCommand::Update(args)) => edit::handle_update(&args, &resolved, force),
        Some(TaskCommand::Build(args)) => build::handle(&args, force, &resolved),
        Some(TaskCommand::Decompose(args)) => decompose::handle(&args, force, &resolved),
        Some(TaskCommand::Template(template_args)) => template::handle(&resolved, &template_args),
        Some(TaskCommand::BuildRefactor(args)) | Some(TaskCommand::Refactor(args)) => {
            refactor::handle(&args, force, &resolved)
        }
        Some(TaskCommand::Show(args)) => show::handle(&args, &resolved),
        Some(TaskCommand::Clone(args)) => clone::handle(&args, force, &resolved),
        Some(TaskCommand::Batch(args)) => batch::handle(&args, force, &resolved),
        Some(TaskCommand::Schedule(args)) => schedule::handle(&args, force, &resolved),
        Some(TaskCommand::Relate(args)) => relations::handle_relate(&args, force, &resolved),
        Some(TaskCommand::Blocks(args)) => relations::handle_blocks(&args, force, &resolved),
        Some(TaskCommand::MarkDuplicate(args)) => {
            relations::handle_mark_duplicate(&args, force, &resolved)
        }
        Some(TaskCommand::Split(args)) => split::handle(&args, force, &resolved),
        Some(TaskCommand::Start(args)) => start::handle(&args, force, &resolved),
        Some(TaskCommand::Children(args)) => children::handle(&args, &resolved),
        Some(TaskCommand::Parent(args)) => parent::handle(&args, &resolved),
        Some(TaskCommand::From(args)) => match args.command {
            TaskFromCommand::Template(template_args) => {
                from_template::handle(&resolved, &template_args, force)
            }
        },
        None => build::handle(&args.build, force, &resolved),
    }
}
