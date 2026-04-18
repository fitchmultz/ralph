//! Built-in task templates for common patterns.
//!
//! Responsibilities:
//! - Define embedded JSON templates for standard task types.
//! - Provide lookup functions for built-in templates by name.
//!
//! Not handled here:
//! - Custom template loading from filesystem (see `loader.rs`).
//! - Template merging with user options (see `merge.rs`).
//!
//! Invariants/assumptions:
//! - Template JSON is valid and parses to Task structs.
//! - Template names are lowercase ASCII without spaces.

/// Built-in bug fix template
pub const BUG_TEMPLATE: &str = r#"{
  "id": "",
  "title": "",
  "status": "todo",
  "priority": "high",
  "tags": ["bug", "fix"],
  "plan": [
    "Reproduce the issue",
    "Identify root cause",
    "Implement fix",
    "Add regression test",
    "Verify fix with the configured CI gate"
  ],
  "evidence": []
}"#;

/// Built-in feature template
pub const FEATURE_TEMPLATE: &str = r#"{
  "id": "",
  "title": "",
  "status": "draft",
  "priority": "medium",
  "tags": ["feature", "enhancement"],
  "plan": [
    "Design the feature interface",
    "Implement core functionality",
    "Add tests",
    "Update documentation",
    "Run the configured CI gate"
  ],
  "evidence": []
}"#;

/// Built-in refactor template
pub const REFACTOR_TEMPLATE: &str = r#"{
  "id": "",
  "title": "",
  "status": "todo",
  "priority": "medium",
  "tags": ["refactor", "cleanup"],
  "plan": [
    "Analyze current implementation",
    "Identify improvement opportunities",
    "Refactor with tests passing",
    "Verify no behavior changes",
    "Run the configured CI gate"
  ],
  "evidence": []
}"#;

/// Built-in test template
pub const TEST_TEMPLATE: &str = r#"{
  "id": "",
  "title": "",
  "status": "todo",
  "priority": "high",
  "tags": ["test", "coverage"],
  "plan": [
    "Identify untested scenarios",
    "Write test cases",
    "Ensure tests fail before fix",
    "Implement/fix as needed",
    "Verify coverage with the configured CI gate"
  ],
  "evidence": []
}"#;

/// Built-in documentation template
pub const DOCS_TEMPLATE: &str = r#"{
  "id": "",
  "title": "",
  "status": "todo",
  "priority": "low",
  "tags": ["docs", "documentation"],
  "plan": [
    "Identify documentation gaps",
    "Write clear explanations",
    "Add code examples",
    "Review for accuracy",
    "Update related docs"
  ],
  "evidence": []
}"#;

/// Built-in add-tests template - for adding tests to existing code
pub const ADD_TESTS_TEMPLATE: &str = r#"{
  "id": "",
  "title": "Add tests for {{target}}",
  "status": "todo",
  "priority": "high",
  "tags": ["test", "coverage", "quality"],
  "scope": ["{{target}}"],
  "plan": [
    "Analyze {{target}} to understand functionality",
    "Identify test scenarios and edge cases",
    "Write unit tests for {{module}}",
    "Write integration tests if applicable",
    "Verify test coverage with cargo tarpaulin or similar",
    "Run the configured CI gate to ensure all tests pass"
  ],
  "evidence": [
    "Current test coverage gaps in {{target}}",
    "Functionality that needs testing"
  ]
}"#;

/// Built-in refactor-performance template - for performance optimization
pub const REFACTOR_PERFORMANCE_TEMPLATE: &str = r#"{
  "id": "",
  "title": "Optimize performance of {{target}}",
  "status": "todo",
  "priority": "medium",
  "tags": ["refactor", "performance", "optimization"],
  "scope": ["{{target}}"],
  "plan": [
    "Profile current performance of {{target}}",
    "Identify bottlenecks and hot paths",
    "Implement targeted optimizations",
    "Benchmark before/after performance",
    "Verify correctness is preserved",
    "Run the configured CI gate to validate changes"
  ],
  "evidence": [
    "Performance measurements showing bottleneck",
    "Profiling data from {{target}}"
  ]
}"#;

/// Built-in fix-error-handling template - for improving error handling
pub const FIX_ERROR_HANDLING_TEMPLATE: &str = r#"{
  "id": "",
  "title": "Fix error handling in {{target}}",
  "status": "todo",
  "priority": "high",
  "tags": ["bug", "error-handling", "reliability"],
  "scope": ["{{target}}"],
  "plan": [
    "Audit current error handling in {{target}}",
    "Identify gaps and anti-patterns",
    "Implement proper error types with thiserror/anyhow",
    "Add error context and logging where needed",
    "Test error paths and edge cases",
    "Run the configured CI gate to validate all error scenarios"
  ],
  "evidence": [
    "Error handling gaps in {{target}}",
    "Panics or unwraps that should be proper errors"
  ]
}"#;

/// Built-in add-docs template - for documentation improvements
pub const ADD_DOCS_TEMPLATE: &str = r#"{
  "id": "",
  "title": "Add documentation for {{target}}",
  "status": "todo",
  "priority": "low",
  "tags": ["docs", "documentation"],
  "scope": ["{{target}}"],
  "plan": [
    "Identify undocumented public APIs in {{target}}",
    "Add module-level documentation (//!)",
    "Add function/struct documentation (///)",
    "Include code examples in doc comments",
    "Review for accuracy and completeness",
    "Run the configured CI gate to check doc tests"
  ],
  "evidence": [
    "Missing documentation in {{target}}",
    "Public APIs without doc comments"
  ]
}"#;

/// Built-in security-audit template - for security improvements
pub const SECURITY_AUDIT_TEMPLATE: &str = r#"{
  "id": "",
  "title": "Security audit of {{target}}",
  "status": "todo",
  "priority": "critical",
  "tags": ["security", "audit", "compliance"],
  "scope": ["{{target}}"],
  "plan": [
    "Review security-sensitive code in {{target}}",
    "Check for common vulnerabilities (OWASP top 10)",
    "Audit input validation and sanitization",
    "Implement security fixes",
    "Add security-focused tests",
    "Run the configured CI gate and security scans"
  ],
  "evidence": [
    "Security-sensitive code in {{target}}",
    "Potential vulnerability indicators"
  ]
}"#;

/// Get built-in template by name
pub fn get_builtin_template(name: &str) -> Option<&'static str> {
    match name {
        "bug" => Some(BUG_TEMPLATE),
        "feature" => Some(FEATURE_TEMPLATE),
        "refactor" => Some(REFACTOR_TEMPLATE),
        "test" => Some(TEST_TEMPLATE),
        "docs" => Some(DOCS_TEMPLATE),
        "add-tests" => Some(ADD_TESTS_TEMPLATE),
        "refactor-performance" => Some(REFACTOR_PERFORMANCE_TEMPLATE),
        "fix-error-handling" => Some(FIX_ERROR_HANDLING_TEMPLATE),
        "add-docs" => Some(ADD_DOCS_TEMPLATE),
        "security-audit" => Some(SECURITY_AUDIT_TEMPLATE),
        _ => None,
    }
}

/// List all built-in template names
pub fn list_builtin_templates() -> Vec<&'static str> {
    vec![
        "add-docs",
        "add-tests",
        "bug",
        "docs",
        "feature",
        "fix-error-handling",
        "refactor",
        "refactor-performance",
        "security-audit",
        "test",
    ]
}

/// Get a human-readable description for a built-in template
pub fn get_template_description(name: &str) -> &'static str {
    match name {
        "add-docs" => "Add documentation for a specific file or module",
        "add-tests" => "Add tests for existing code with coverage verification",
        "bug" => "Bug fix with reproduction steps and regression tests",
        "docs" => "Documentation update or creation",
        "feature" => "New feature with design, implementation, and documentation",
        "fix-error-handling" => "Fix error handling with proper types and context",
        "refactor" => "Code refactoring with behavior preservation",
        "refactor-performance" => "Optimize performance with profiling and benchmarking",
        "security-audit" => "Security audit with vulnerability checks",
        "test" => "Test addition or improvement",
        _ => "Task template",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("bug").is_some());
        assert!(get_builtin_template("feature").is_some());
        assert!(get_builtin_template("refactor").is_some());
        assert!(get_builtin_template("test").is_some());
        assert!(get_builtin_template("docs").is_some());
        assert!(get_builtin_template("add-tests").is_some());
        assert!(get_builtin_template("refactor-performance").is_some());
        assert!(get_builtin_template("fix-error-handling").is_some());
        assert!(get_builtin_template("add-docs").is_some());
        assert!(get_builtin_template("security-audit").is_some());
        assert!(get_builtin_template("unknown").is_none());
    }

    #[test]
    fn test_list_builtin_templates() {
        let templates = list_builtin_templates();
        assert_eq!(templates.len(), 10);
        assert!(templates.contains(&"bug"));
        assert!(templates.contains(&"feature"));
        assert!(templates.contains(&"refactor"));
        assert!(templates.contains(&"test"));
        assert!(templates.contains(&"docs"));
        assert!(templates.contains(&"add-tests"));
        assert!(templates.contains(&"refactor-performance"));
        assert!(templates.contains(&"fix-error-handling"));
        assert!(templates.contains(&"add-docs"));
        assert!(templates.contains(&"security-audit"));
    }

    #[test]
    fn test_templates_are_valid_json() {
        for name in list_builtin_templates() {
            let template_json = get_builtin_template(name).unwrap();
            let result: Result<crate::contracts::Task, _> = serde_json::from_str(template_json);
            assert!(result.is_ok(), "Template {} should be valid JSON", name);
        }
    }

    #[test]
    fn test_get_template_description() {
        assert!(get_template_description("bug").contains("Bug fix"));
        assert!(get_template_description("feature").contains("feature"));
        assert!(get_template_description("add-tests").contains("tests"));
        assert!(get_template_description("refactor-performance").contains("performance"));
        assert!(get_template_description("fix-error-handling").contains("error"));
        assert!(get_template_description("add-docs").contains("documentation"));
        assert!(get_template_description("security-audit").contains("Security"));
        assert!(get_template_description("unknown").contains("Task template"));
    }
}
