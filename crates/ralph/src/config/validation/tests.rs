//! Focused config-validation tests.
//!
//! Purpose:
//! - Focused config-validation tests.
//!
//! Responsibilities:
//! - Cover queue threshold, git-ref, and internal validation helpers housed in this module tree.
//!
//! Not handled here:
//! - Cross-module config resolution tests defined in `config/tests.rs`.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - These tests exercise the split validators directly.

use super::{
    git_ref_invalid_reason, queue::validate_queue_thresholds, validate_config,
    validate_queue_overrides,
};
use crate::contracts::{Config, QueueConfig};

#[test]
fn validate_queue_thresholds_accepts_boundary_values() {
    let queue = QueueConfig {
        size_warning_threshold_kb: Some(100),
        task_count_warning_threshold: Some(5000),
        max_dependency_depth: Some(1),
        auto_archive_terminal_after_days: Some(3650),
        ..Default::default()
    };
    assert!(validate_queue_thresholds(&queue).is_ok());
}

#[test]
fn validate_queue_thresholds_rejects_invalid_values() {
    let queue = QueueConfig {
        size_warning_threshold_kb: Some(50),
        ..Default::default()
    };
    let err = validate_queue_thresholds(&queue).expect_err("thresholds should fail");
    assert!(err.to_string().contains("size_warning_threshold_kb"));
}

#[test]
fn validate_queue_overrides_still_calls_threshold_validation() {
    let queue = QueueConfig {
        size_warning_threshold_kb: Some(50),
        ..Default::default()
    };
    let err = validate_queue_overrides(&queue).expect_err("overrides should fail");
    assert!(err.to_string().contains("size_warning_threshold_kb"));
}

#[test]
fn validate_config_rejects_invalid_thresholds() {
    let cfg = Config {
        version: 2,
        queue: QueueConfig {
            size_warning_threshold_kb: Some(50_000),
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("config should fail");
    assert!(err.to_string().contains("size_warning_threshold_kb"));
}

#[test]
fn git_ref_validator_rejects_invalid_patterns() {
    assert!(git_ref_invalid_reason("feature..branch").is_some());
    assert!(git_ref_invalid_reason("feature/@/branch").is_some());
    assert!(git_ref_invalid_reason("feature name").is_some());
    assert!(git_ref_invalid_reason("valid/branch").is_none());
}
