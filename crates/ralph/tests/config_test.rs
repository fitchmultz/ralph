//! Unit tests for config resolution, merging, and path behavior.
//!
//! Purpose:
//! - Unit tests for config resolution, merging, and path behavior.
//!
//! Responsibilities:
//! - Provide shared fixtures for config integration-style unit tests.
//! - Delegate path, validation, merge, and JSONC behavior to focused modules.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use ralph::config;
use ralph::contracts::{
    AgentConfig, CiGateConfig, Config, GitRevertMode, Model, NotificationConfig, ProjectType,
    QueueConfig, ReasoningEffort, Runner, RunnerRetryConfig, WebhookConfig,
};
use serial_test::serial;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use test_support::env_lock;

mod test_support;

// Helper to create a minimal .ralph directory
fn setup_ralph_dir(dir: &TempDir) -> PathBuf {
    let ralph_dir = dir.path().join(".ralph");
    fs::create_dir_all(&ralph_dir).expect("create .ralph dir");
    ralph_dir
}

// Helper to create a queue.jsonc file
fn create_queue_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let queue_path = ralph_dir.join("queue.jsonc");
    fs::write(&queue_path, content).expect("write queue.jsonc");
    queue_path
}

// Helper to create a done.json file
#[allow(dead_code)]
fn create_done_json(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let done_path = ralph_dir.join("done.json");
    fs::write(&done_path, content).expect("write done.json");
    done_path
}

// Helper to create a done.jsonc file
#[allow(dead_code)]
fn create_done_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let done_path = ralph_dir.join("done.jsonc");
    fs::write(&done_path, content).expect("write done.jsonc");
    done_path
}

// Helper to create a config.jsonc file
fn create_config_jsonc(dir: &TempDir, content: &str) -> PathBuf {
    let ralph_dir = setup_ralph_dir(dir);
    let config_path = ralph_dir.join("config.jsonc");
    fs::write(&config_path, content).expect("write config.jsonc");
    config_path
}

#[path = "config_test/id_and_validation.rs"]
mod id_and_validation;
#[path = "config_test/jsonc_and_tilde.rs"]
mod jsonc_and_tilde;
#[path = "config_test/layer_merge.rs"]
mod layer_merge;
#[path = "config_test/repo_paths.rs"]
mod repo_paths;
