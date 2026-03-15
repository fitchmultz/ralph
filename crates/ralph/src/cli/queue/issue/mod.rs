//! `ralph queue issue` subcommand facade.
//!
//! Responsibilities:
//! - Define the stable module boundary for GitHub issue publishing commands.
//! - Re-export clap argument types used by the queue CLI surface.
//! - Keep command routing thin while sibling modules own workflow details.
//!
//! Not handled here:
//! - GitHub CLI process execution details.
//! - Queue export markdown rendering.
//! - Queue test coverage outside this subcommand's helpers.
//!
//! Invariants/assumptions:
//! - Queue mutation only happens while the queue lock is held.
//! - `gh` availability/auth checks stay inside execute paths.
//! - Public CLI/help behavior remains unchanged across internal refactors.

mod args;
mod common;
mod handle;
mod output;
mod publish;

pub use args::{
    QueueIssueArgs, QueueIssueCommand, QueueIssuePublishArgs, QueueIssuePublishManyArgs,
};
pub(crate) use handle::handle;
