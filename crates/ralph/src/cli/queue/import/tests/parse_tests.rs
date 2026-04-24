//! Parser-focused queue import tests.
//!
//! Purpose:
//! - Parser-focused queue import tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::super::parse::{
    parse_csv_tasks, parse_custom_fields, parse_json_tasks, parse_list_field, parse_status,
};
use crate::contracts::{TaskPriority, TaskStatus};

#[test]
fn parse_json_array_succeeds() {
    let json = r#"[{"id": "RQ-0001", "title": "Test task", "status": "todo"}]"#;
    let tasks = parse_json_tasks(json).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "RQ-0001");
    assert_eq!(tasks[0].title, "Test task");
}

#[test]
fn parse_json_wrapper_succeeds() {
    let json = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
    let tasks = parse_json_tasks(json).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "RQ-0001");
}

#[test]
fn parse_json_wrapper_wrong_version_fails() {
    let json = r#"{"version": 2, "tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
    let result = parse_json_tasks(json);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("version"));
}

#[test]
fn parse_json_empty_input_returns_empty() {
    assert!(parse_json_tasks("").unwrap().is_empty());
    assert!(parse_json_tasks("   ").unwrap().is_empty());
}

#[test]
fn parse_csv_basic_succeeds() {
    let csv = "id,title,status\nRQ-0001,Test task,todo\nRQ-0002,Another task,done";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].status, TaskStatus::Todo);
    assert_eq!(tasks[1].status, TaskStatus::Done);
}

#[test]
fn parse_csv_missing_title_fails() {
    let csv = "id,status\nRQ-0001,todo";
    let result = parse_csv_tasks(csv, b',');
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("title"));
}

#[test]
fn parse_csv_empty_title_fails() {
    let csv = "id,title\nRQ-0001,";
    assert!(parse_csv_tasks(csv, b',').is_err());
}

#[test]
fn parse_csv_list_fields_parsed() {
    let csv = "title,tags,scope,evidence,plan,notes\nTest,a,b,c,d,e";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks[0].tags, vec!["a"]);
    assert_eq!(tasks[0].scope, vec!["b"]);
    assert_eq!(tasks[0].evidence, vec!["c"]);
    assert_eq!(tasks[0].plan, vec!["d"]);
    assert_eq!(tasks[0].notes, vec!["e"]);
}

#[test]
fn parse_csv_list_fields_drop_empty() {
    let csv = "title,evidence\nTest,a;;b;";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks[0].evidence, vec!["a", "b"]);
}

#[test]
fn parse_csv_semicolon_fields_parsed() {
    let csv = "title,evidence,plan,notes\nTest,a;b,c;d,e;f;";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks[0].evidence, vec!["a", "b"]);
    assert_eq!(tasks[0].plan, vec!["c", "d"]);
    assert_eq!(tasks[0].notes, vec!["e", "f"]);
}

#[test]
fn parse_csv_custom_fields_parsed() {
    let csv = "title,custom_fields\nTest,\"a=1,b=two\"";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks[0].custom_fields.get("a"), Some(&"1".to_string()));
    assert_eq!(tasks[0].custom_fields.get("b"), Some(&"two".to_string()));
}

#[test]
fn parse_csv_custom_fields_invalid_fails() {
    let csv = "title,custom_fields\nTest,invalid_no_equals";
    assert!(parse_csv_tasks(csv, b',').is_err());
}

#[test]
fn parse_csv_empty_custom_fields_ok() {
    let csv = "title,custom_fields\nTest,";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert!(tasks[0].custom_fields.is_empty());
}

#[test]
fn parse_csv_unknown_columns_ignored() {
    let csv = "id,title,unknown_col\nRQ-0001,Test,foo";
    let tasks = parse_csv_tasks(csv, b',').unwrap();
    assert_eq!(tasks[0].id, "RQ-0001");
    assert_eq!(tasks[0].title, "Test");
}

#[test]
fn parse_tsv_succeeds() {
    let tsv = "id\ttitle\tstatus\nRQ-0001\tTest\ttodo";
    let tasks = parse_csv_tasks(tsv, b'\t').unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "RQ-0001");
}

#[test]
fn parse_list_field_handles_delimiters() {
    assert_eq!(parse_list_field("a, b, , c", ','), vec!["a", "b", "c"]);
    assert_eq!(parse_list_field("x; y; ; z", ';'), vec!["x", "y", "z"]);
}

#[test]
fn parse_custom_fields_parses_empty_string() {
    assert!(parse_custom_fields("").unwrap().is_empty());
}

#[test]
fn parse_status_case_insensitive() {
    assert_eq!(parse_status("TODO").unwrap(), TaskStatus::Todo);
    assert_eq!(parse_status("Done").unwrap(), TaskStatus::Done);
    assert_eq!(parse_status("Rejected").unwrap(), TaskStatus::Rejected);
}

#[test]
fn parse_csv_invalid_priority_uses_canonical_parser_error() {
    let csv = "title,priority\nTest,nope";
    let err = parse_csv_tasks(csv, b',').unwrap_err();
    let expected = "nope".parse::<TaskPriority>().unwrap_err().to_string();
    assert!(err.to_string().contains(&expected), "err was: {err}");
}
