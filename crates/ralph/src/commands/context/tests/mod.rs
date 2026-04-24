//! Tests for AGENTS.md context command behavior, grouped by concern.
//!
//! Purpose:
//! - Tests for AGENTS.md context command behavior, grouped by concern.
//!
//! Responsibilities:
//! - Provide shared fixtures for detection, init, update, and validation tests.
//! - Keep the production context facade free of large inline scenario blocks.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::detect::detect_project_type;
use super::markdown::{extract_section_titles, parse_markdown_sections};
use super::types::{
    ContextInitOptions, ContextUpdateOptions, ContextValidateOptions, DetectedProjectType,
    FileInitStatus,
};
use super::{run_context_init, run_context_update, run_context_validate};
use crate::config;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

fn create_test_resolved(dir: &TempDir) -> config::Resolved {
    let repo_root = dir.path().to_path_buf();
    config::Resolved {
        config: crate::contracts::Config::default(),
        queue_path: repo_root.join(".ralph/queue.json"),
        done_path: repo_root.join(".ralph/done.json"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(repo_root.join(".ralph/config.json")),
        repo_root,
    }
}

mod detect_tests;
mod init_tests;
mod markdown_tests;
mod update_tests;
mod validate_tests;
