//! `ralph task ...` command group: Clap types and handler facade.
//!
//! Purpose:
//! - `ralph task ...` command group: Clap types and handler facade.
//!
//! Responsibilities:
//! - Re-export task argument types from the focused `args` tree.
//! - Wire task subcommand modules into a small facade surface.
//! - Expose the shared task command entrypoint from `handle.rs`.
//!
//! Not handled here:
//! - Queue persistence and locking semantics (see `crate::queue` and `crate::lock`).
//! - Task execution or runner behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Configuration is resolved from the current working directory in `handle.rs`.
//! - Parser and help regression coverage lives in `tests.rs`, not inline here.

mod args;
mod batch;
mod build;
mod children;
mod clone;
mod decompose;
mod edit;
mod followups;
mod from_template;
mod handle;
mod mutate;
mod parent;
mod refactor;
mod relations;
mod schedule;
mod show;
mod split;
mod start;
mod status;
mod template;

pub use args::{
    BatchEditArgs, BatchFieldArgs, BatchMode, BatchOperation, BatchStatusArgs, TaskArgs,
    TaskBatchArgs, TaskBlocksArgs, TaskBuildArgs, TaskBuildRefactorArgs, TaskChildrenArgs,
    TaskCloneArgs, TaskCommand, TaskDecomposeArgs, TaskDecomposeChildPolicyArg,
    TaskDecomposeFormatArg, TaskDoneArgs, TaskEditArgs, TaskEditFieldArg, TaskFieldArgs,
    TaskFollowupsApplyArgs, TaskFollowupsArgs, TaskFollowupsCommand, TaskFollowupsFormatArg,
    TaskFromArgs, TaskFromCommand, TaskFromTemplateArgs, TaskMarkDuplicateArgs, TaskMutateArgs,
    TaskParentArgs, TaskReadyArgs, TaskRejectArgs, TaskRelateArgs, TaskRelationFormat,
    TaskScheduleArgs, TaskShowArgs, TaskSplitArgs, TaskStartArgs, TaskStatusArg, TaskStatusArgs,
    TaskTemplateArgs, TaskTemplateBuildArgs, TaskTemplateCommand, TaskTemplateShowArgs,
    TaskUpdateArgs,
};
pub use handle::handle_task;

#[cfg(test)]
mod tests;
