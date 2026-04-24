//! Project-type detection tests.
//!
//! Purpose:
//! - Project-type detection tests.
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

use super::*;

#[test]
fn detect_project_type_finds_rust() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("Cargo.toml"), "[package]").expect("write Cargo.toml");
    assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Rust);
}

#[test]
fn detect_project_type_finds_python() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("pyproject.toml"), "").expect("write pyproject.toml");
    assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Python);
}

#[test]
fn detect_project_type_finds_typescript() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("package.json"), "{}").expect("write package.json");
    assert_eq!(
        detect_project_type(dir.path()),
        DetectedProjectType::TypeScript
    );
}

#[test]
fn detect_project_type_finds_go() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join("go.mod"), "module test").expect("write go.mod");
    assert_eq!(detect_project_type(dir.path()), DetectedProjectType::Go);
}

#[test]
fn detect_project_type_defaults_to_generic() {
    let dir = TempDir::new().expect("create temp dir");
    assert_eq!(
        detect_project_type(dir.path()),
        DetectedProjectType::Generic
    );
}
