//! Purpose: Verify task priority behavior plus task/task-agent serde and
//! schema-adjacent helpers.
//!
//! Responsibilities:
//! - Cover priority cycling and parsing.
//! - Cover `custom_fields` scalar coercion and rejection behavior.
//! - Cover `TaskAgent` serialization/deserialization edge cases.
//!
//! Scope:
//! - Regression tests only; task-contract implementation lives in sibling
//!   modules.
//!
//! Usage:
//! - Run via `cargo test -p ralph-agent-loop contracts::task` or the broader
//!   CI gates.
//!
//! Invariants/Assumptions:
//! - Priority parsing keeps the canonical error message stable.
//! - Task-agent serde behavior preserves default omission and phase override support.

use std::collections::HashMap;

use crate::contracts::{Model, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner};

use super::{Task, TaskPriority};

#[test]
fn task_priority_cycle_wraps_through_all_values() {
    assert_eq!(TaskPriority::Low.cycle(), TaskPriority::Medium);
    assert_eq!(TaskPriority::Medium.cycle(), TaskPriority::High);
    assert_eq!(TaskPriority::High.cycle(), TaskPriority::Critical);
    assert_eq!(TaskPriority::Critical.cycle(), TaskPriority::Low);
}

#[test]
fn task_priority_from_str_is_case_insensitive_and_trims() {
    assert_eq!("HIGH".parse::<TaskPriority>().unwrap(), TaskPriority::High);
    assert_eq!(
        "Medium".parse::<TaskPriority>().unwrap(),
        TaskPriority::Medium
    );
    assert_eq!(" low ".parse::<TaskPriority>().unwrap(), TaskPriority::Low);
    assert_eq!(
        "CRITICAL".parse::<TaskPriority>().unwrap(),
        TaskPriority::Critical
    );
}

#[test]
fn task_priority_from_str_invalid_has_canonical_error_message() {
    let err = "nope".parse::<TaskPriority>().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid priority: 'nope'. Expected one of: critical, high, medium, low."
    );
}

#[test]
fn task_priority_from_str_empty_string_errors() {
    let err = "".parse::<TaskPriority>().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid priority: ''. Expected one of: critical, high, medium, low."
    );
}

#[test]
fn task_custom_fields_deserialize_coerces_scalars_to_strings() {
    let raw = r#"{
            "id": "RQ-0001",
            "title": "t",
            "custom_fields": {
                "guide_line_count": 1411,
                "enabled": true,
                "owner": "ralph"
            }
        }"#;

    let task: Task = serde_json::from_str(raw).expect("deserialize");
    assert_eq!(
        task.custom_fields
            .get("guide_line_count")
            .map(String::as_str),
        Some("1411")
    );
    assert_eq!(
        task.custom_fields.get("enabled").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        task.custom_fields.get("owner").map(String::as_str),
        Some("ralph")
    );
}

#[test]
fn task_custom_fields_deserialize_rejects_null() {
    let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":null}}"#;
    let err = serde_json::from_str::<Task>(raw).unwrap_err();
    let err_msg = err.to_string().to_lowercase();
    assert!(
        err_msg.contains("custom_fields"),
        "error should mention custom_fields: {}",
        err_msg
    );
    assert!(
        err_msg.contains("null"),
        "error should mention null: {}",
        err_msg
    );
}

#[test]
fn task_custom_fields_deserialize_rejects_custom_fields_null() {
    let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":null}"#;
    let err = serde_json::from_str::<Task>(raw).unwrap_err();
    let err_msg = err.to_string().to_lowercase();
    assert!(
        err_msg.contains("custom_fields"),
        "error should mention custom_fields: {}",
        err_msg
    );
    assert!(
        err_msg.contains("null"),
        "error should mention null: {}",
        err_msg
    );
}

#[test]
fn task_custom_fields_deserialize_rejects_custom_fields_non_object() {
    let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":123}"#;
    let err = serde_json::from_str::<Task>(raw).unwrap_err();
    let err_msg = err.to_string().to_lowercase();
    assert!(
        err_msg.contains("custom_fields"),
        "error should mention custom_fields: {}",
        err_msg
    );
    assert!(
        err_msg.contains("object") || err_msg.contains("map"),
        "error should mention object/map: {}",
        err_msg
    );
}

#[test]
fn task_custom_fields_deserialize_rejects_object_and_array_values() {
    let raw_obj = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":{"a":1}}}"#;
    let raw_arr = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":[1,2]}}"#;

    let err_obj = serde_json::from_str::<Task>(raw_obj).unwrap_err();
    let err_arr = serde_json::from_str::<Task>(raw_arr).unwrap_err();

    let err_obj_msg = err_obj.to_string().to_lowercase();
    let err_arr_msg = err_arr.to_string().to_lowercase();

    assert!(
        err_obj_msg.contains("custom_fields"),
        "object error should mention custom_fields: {}",
        err_obj_msg
    );
    assert!(
        err_arr_msg.contains("custom_fields"),
        "array error should mention custom_fields: {}",
        err_arr_msg
    );
}

#[test]
fn task_custom_fields_serializes_as_strings() {
    let mut custom_fields = HashMap::new();
    custom_fields.insert("count".to_string(), "42".to_string());
    custom_fields.insert("enabled".to_string(), "true".to_string());

    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test".to_string(),
        custom_fields,
        ..Default::default()
    };

    let json = serde_json::to_string(&task).expect("serialize");
    assert!(json.contains("\"count\":\"42\""));
    assert!(json.contains("\"enabled\":\"true\""));
}

#[test]
fn task_agent_deserializes_phases_and_phase_overrides() {
    let raw = r#"{
            "id":"RQ-0001",
            "title":"Task with agent overrides",
            "agent":{
                "runner":"codex",
                "model":"gpt-5.3-codex",
                "model_effort":"high",
                "phases":2,
                "iterations":1,
                "phase_overrides":{
                    "phase1":{"runner":"codex","model":"gpt-5.3-codex","reasoning_effort":"high"},
                    "phase2":{"runner":"kimi","model":"kimi-code/kimi-for-coding"}
                }
            }
        }"#;

    let task: Task = serde_json::from_str(raw).expect("deserialize");
    let agent = task.agent.expect("agent should be set");
    assert_eq!(agent.runner, Some(Runner::Codex));
    assert_eq!(agent.model, Some(Model::Gpt53Codex));
    assert_eq!(agent.phases, Some(2));
    assert_eq!(agent.iterations, Some(1));

    let phase_overrides = agent
        .phase_overrides
        .expect("phase overrides should be set");
    let phase1 = phase_overrides.phase1.expect("phase1 should be set");
    assert_eq!(phase1.runner, Some(Runner::Codex));
    assert_eq!(phase1.reasoning_effort, Some(ReasoningEffort::High));
    let phase2 = phase_overrides.phase2.expect("phase2 should be set");
    assert_eq!(phase2.runner, Some(Runner::Kimi));
}

#[test]
fn task_agent_omits_default_phase_and_effort_fields_when_serializing() {
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Serialize defaults".to_string(),
        agent: Some(crate::contracts::TaskAgent {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt53Codex),
            model_effort: crate::contracts::ModelEffort::Default,
            phases: None,
            iterations: None,
            followup_reasoning_effort: None,
            runner_cli: None,
            phase_overrides: Some(PhaseOverrides {
                phase1: Some(PhaseOverrideConfig {
                    runner: Some(Runner::Codex),
                    model: Some(Model::Gpt53Codex),
                    reasoning_effort: Some(ReasoningEffort::Medium),
                }),
                ..Default::default()
            }),
        }),
        ..Default::default()
    };

    let value = serde_json::to_value(task).expect("serialize");
    let agent = value
        .get("agent")
        .and_then(|value| value.as_object())
        .expect("agent object should exist");
    assert!(!agent.contains_key("model_effort"));
    assert!(!agent.contains_key("phases"));
    assert!(agent.contains_key("phase_overrides"));
}
