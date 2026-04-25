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
use crate::contracts::{Config, QueueConfig, WebhookConfig};

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

#[test]
fn validate_config_rejects_invalid_webhook_timeout_secs() {
    let cfg = Config {
        version: 2,
        agent: crate::contracts::AgentConfig {
            webhook: WebhookConfig {
                timeout_secs: Some(0),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("timeout should fail");
    let message = err.to_string();
    assert!(message.contains("Invalid agent.webhook.timeout_secs: 0."));
    assert!(message.contains("between 1 and 300"));
}

#[test]
fn validate_config_rejects_invalid_webhook_retry_count() {
    let cfg = Config {
        version: 2,
        agent: crate::contracts::AgentConfig {
            webhook: WebhookConfig {
                retry_count: Some(11),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("retry count should fail");
    let message = err.to_string();
    assert!(message.contains("Invalid agent.webhook.retry_count: 11."));
    assert!(message.contains("between 0 and 10"));
}

#[test]
fn validate_config_rejects_invalid_webhook_retry_backoff() {
    let cfg = Config {
        version: 2,
        agent: crate::contracts::AgentConfig {
            webhook: WebhookConfig {
                retry_backoff_ms: Some(99),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("retry backoff should fail");
    let message = err.to_string();
    assert!(message.contains("Invalid agent.webhook.retry_backoff_ms: 99."));
    assert!(message.contains("between 100 and 30000"));
}

#[test]
fn validate_config_rejects_invalid_webhook_queue_capacity() {
    let cfg = Config {
        version: 2,
        agent: crate::contracts::AgentConfig {
            webhook: WebhookConfig {
                queue_capacity: Some(9),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("queue capacity should fail");
    let message = err.to_string();
    assert!(message.contains("Invalid agent.webhook.queue_capacity: 9."));
    assert!(message.contains("between 10 and 10000"));
}

#[test]
fn validate_config_rejects_invalid_webhook_parallel_multiplier() {
    let cfg = Config {
        version: 2,
        agent: crate::contracts::AgentConfig {
            webhook: WebhookConfig {
                parallel_queue_multiplier: Some(10.5),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let err = validate_config(&cfg).expect_err("parallel multiplier should fail");
    let message = err.to_string();
    assert!(message.contains("Invalid agent.webhook.parallel_queue_multiplier: 10.5."));
    assert!(message.contains("between 1 and 10"));
}
