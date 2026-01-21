//! Validates generated JSON schemas against runtime config constraints.

use serde_json::Value;
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

#[test]
fn config_schema_agent_phases_matches_runtime_validation() {
    let schema = load_config_schema();
    let phases = &schema["definitions"]["AgentConfig"]["properties"]["phases"];

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
