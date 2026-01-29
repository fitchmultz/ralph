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
    "Verify fix with make ci"
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
    "Run make ci"
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
    "Run make ci"
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
    "Verify coverage with make ci"
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

/// Get built-in template by name
pub fn get_builtin_template(name: &str) -> Option<&'static str> {
    match name {
        "bug" => Some(BUG_TEMPLATE),
        "feature" => Some(FEATURE_TEMPLATE),
        "refactor" => Some(REFACTOR_TEMPLATE),
        "test" => Some(TEST_TEMPLATE),
        "docs" => Some(DOCS_TEMPLATE),
        _ => None,
    }
}

/// List all built-in template names
pub fn list_builtin_templates() -> Vec<&'static str> {
    vec!["bug", "feature", "refactor", "test", "docs"]
}

/// Get a human-readable description for a built-in template
pub fn get_template_description(name: &str) -> &'static str {
    match name {
        "bug" => "Bug fix with reproduction steps and regression tests",
        "feature" => "New feature with design, implementation, and documentation",
        "refactor" => "Code refactoring with behavior preservation",
        "test" => "Test addition or improvement",
        "docs" => "Documentation update or creation",
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
        assert!(get_builtin_template("unknown").is_none());
    }

    #[test]
    fn test_list_builtin_templates() {
        let templates = list_builtin_templates();
        assert_eq!(templates.len(), 5);
        assert!(templates.contains(&"bug"));
        assert!(templates.contains(&"feature"));
        assert!(templates.contains(&"refactor"));
        assert!(templates.contains(&"test"));
        assert!(templates.contains(&"docs"));
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
        assert!(get_template_description("unknown").contains("Task template"));
    }
}
