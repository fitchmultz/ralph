//! Prompt internal tests - shared test utilities and module declarations.
//!
//! Responsibilities: provide shared imports, helpers, and module declarations for prompt tests.
//! Not handled: actual test implementations (see submodules).
//! Invariants/assumptions: tests run in isolated temp directories; Config::default() is valid.

pub(crate) use super::registry::{PromptTemplateId, prompt_template};
pub(crate) use super::{
    merge_conflicts::*, review::*, scan::*, task_builder::*, task_decompose::*, util::*, worker::*,
    worker_phases::*,
};
pub(crate) use crate::cli::scan::ScanMode;
pub(crate) use crate::contracts::{Config, ProjectType};
pub(crate) use anyhow::Result;
pub(crate) use std::fs;
pub(crate) use tempfile::TempDir;

pub(crate) fn default_config() -> Config {
    Config::default()
}

mod merge_conflicts;
mod phases;
mod registry;
mod review;
mod scan;
mod task;
mod variables;
mod worker;
