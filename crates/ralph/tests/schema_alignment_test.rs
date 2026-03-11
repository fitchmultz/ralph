//! Validates generated JSON schemas against runtime config constraints.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn load_config_schema() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root should exist")
        .to_path_buf();
    let schema_path = root.join("schemas").join("config.schema.json");
    let raw =
        fs::read_to_string(&schema_path).expect("schemas/config.schema.json should be readable");
    serde_json::from_str(&raw).expect("config.schema.json must be valid JSON")
}

fn load_queue_schema() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root should exist")
        .to_path_buf();
    let schema_path = root.join("schemas").join("queue.schema.json");
    let raw =
        fs::read_to_string(&schema_path).expect("schemas/queue.schema.json should be readable");
    serde_json::from_str(&raw).expect("queue.schema.json must be valid JSON")
}

fn load_machine_schema() -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root should exist")
        .to_path_buf();
    let schema_path = root.join("schemas").join("machine.schema.json");
    let raw =
        fs::read_to_string(&schema_path).expect("schemas/machine.schema.json should be readable");
    serde_json::from_str(&raw).expect("machine.schema.json must be valid JSON")
}

#[test]
fn schema_alignment_config_agent_phases_matches_runtime_validation() {
    let schema = load_config_schema();
    let phases = &schema["$defs"]["AgentConfig"]["properties"]["phases"];

    let min = phases["minimum"].as_f64().expect("phases.minimum missing");
    let max = phases["maximum"].as_f64().expect("phases.maximum missing");

    assert_eq!(
        min, 1.0,
        "schema minimum must align with runtime validation"
    );
    assert_eq!(
        max, 3.0,
        "schema maximum must align with runtime validation"
    );
}

#[test]
fn schema_alignment_queue_task_required_fields_match_runtime_validation() {
    let schema = load_queue_schema();
    let required = schema["$defs"]["Task"]["required"]
        .as_array()
        .expect("Task.required should be an array");

    let required_set: BTreeSet<&str> = required
        .iter()
        .map(|value| value.as_str().expect("required field must be string"))
        .collect();
    let expected: BTreeSet<&str> = ["id", "title", "created_at", "updated_at"]
        .into_iter()
        .collect();

    assert_eq!(
        required_set, expected,
        "queue schema required fields must align with runtime validation"
    );
}

#[test]
fn schema_alignment_queue_task_timestamps_require_strings() {
    let schema = load_queue_schema();
    let created_at = &schema["$defs"]["Task"]["properties"]["created_at"]["type"];
    let updated_at = &schema["$defs"]["Task"]["properties"]["updated_at"]["type"];

    assert_eq!(created_at, "string", "created_at must be a string");
    assert_eq!(updated_at, "string", "updated_at must be a string");
}

#[test]
fn schema_alignment_machine_bundle_contains_expected_documents() {
    let schema = load_machine_schema();
    let object = schema
        .as_object()
        .expect("machine schema bundle should be a JSON object");

    for key in [
        "system_info",
        "queue_read",
        "config_resolve",
        "task_create_request",
        "task_create",
        "task_mutation",
        "graph_read",
        "dashboard_read",
        "decompose",
        "doctor_report",
        "parallel_status",
        "cli_spec",
        "run_event",
        "run_summary",
    ] {
        assert!(
            object.contains_key(key),
            "machine schema bundle missing expected document {key}"
        );
    }
}
