//! Prompt internal tests - shared test utilities and module declarations.
//!
//! Responsibilities: provide shared imports, helpers, and module declarations for prompt tests.
//! Not handled: actual test implementations (see submodules).
//! Invariants/assumptions: tests run in isolated temp directories; Config::default() is valid.

pub(crate) use super::registry::{prompt_template, PromptTemplateId};
pub(crate) use super::{review::*, scan::*, task_builder::*, util::*, worker::*, worker_phases::*};
pub(crate) use crate::cli::scan::ScanMode;
pub(crate) use crate::contracts::{Config, ProjectType};
pub(crate) use anyhow::Result;
pub(crate) use std::fs;
pub(crate) use tempfile::TempDir;

pub(crate) fn default_config() -> Config {
    Config::default()
}

mod phases;
mod registry;
mod review;
mod scan;
mod task;
mod variables;
mod worker;
