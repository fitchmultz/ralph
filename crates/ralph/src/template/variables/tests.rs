//! Purpose: Preserve regression coverage for template-variable validation,
//! detection, and substitution after the facade split.
//!
//! Responsibilities:
//! - Verify substitution, validation, warning formatting, and context detection.
//! - Keep the former inline `template::variables` test coverage intact.
//!
//! Scope:
//! - Variables-specific behavior only; template loading and merging stay covered
//!   elsewhere.
//!
//! Usage:
//! - Runs as the `template::variables` unit test suite.
//!
//! Invariants/Assumptions:
//! - Assertions and behavior remain aligned with the former monolithic test
//!   block.
//! - Unknown variables remain warnings and unresolved placeholders stay intact.

use crate::testsupport::git as git_test;

use super::context::{TemplateContext, TemplateWarning};
use super::detect::{derive_module_name, detect_context, detect_context_with_warnings};
use super::substitute::{substitute_variables, substitute_variables_in_task};
use super::validate::{extract_variables, validate_task_template};

#[test]
fn test_substitute_variables_all_vars() {
    let context = TemplateContext {
        target: Some("src/cli/task.rs".to_string()),
        module: Some("cli::task".to_string()),
        file: Some("task.rs".to_string()),
        branch: Some("main".to_string()),
    };

    let input =
        "Add tests for {{target}} in module {{module}} (file: {{file}}) on branch {{branch}}";
    let result = substitute_variables(input, &context);

    assert_eq!(
        result,
        "Add tests for src/cli/task.rs in module cli::task (file: task.rs) on branch main"
    );
}

#[test]
fn test_substitute_variables_partial() {
    let context = TemplateContext {
        target: Some("src/main.rs".to_string()),
        module: None,
        file: Some("main.rs".to_string()),
        branch: None,
    };

    let input = "Fix {{target}} - {{file}} - {{unknown}}";
    let result = substitute_variables(input, &context);

    assert_eq!(result, "Fix src/main.rs - main.rs - {{unknown}}");
}

#[test]
fn test_substitute_variables_empty_context() {
    let context = TemplateContext::default();

    let input = "Test {{target}} {{module}}";
    let result = substitute_variables(input, &context);

    // Variables with no value are left as-is
    assert_eq!(result, "Test {{target}} {{module}}");
}

#[test]
fn test_derive_module_name_simple() {
    assert_eq!(derive_module_name("src/main.rs"), "main");
    assert_eq!(derive_module_name("src/cli/task.rs"), "cli::task");
    assert_eq!(derive_module_name("lib/utils.js"), "utils");
}

#[test]
fn test_derive_module_name_nested() {
    assert_eq!(
        derive_module_name("crates/ralph/src/template/builtin.rs"),
        "ralph::template::builtin"
    );
    assert_eq!(
        derive_module_name("src/commands/task/build.rs"),
        "commands::task::build"
    );
}

#[test]
fn test_derive_module_name_no_extension() {
    assert_eq!(derive_module_name("src/cli"), "cli");
    assert_eq!(derive_module_name("src"), "src");
}

#[test]
fn test_detect_context_with_target() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    git_test::init_repo(repo_root).expect("Failed to init git repo");

    let context = detect_context(Some("src/cli/task.rs"), repo_root);

    assert_eq!(context.target, Some("src/cli/task.rs".to_string()));
    assert_eq!(context.file, Some("task.rs".to_string()));
    assert_eq!(context.module, Some("cli::task".to_string()));
    // Branch should be detected (usually "main" or "master" for new repos)
    assert!(context.branch.is_some());
}

#[test]
fn test_detect_context_without_target() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    let context = detect_context(None, repo_root);

    assert_eq!(context.target, None);
    assert_eq!(context.file, None);
    assert_eq!(context.module, None);
}

#[test]
fn test_substitute_variables_in_task() {
    let mut task = crate::contracts::Task {
        id: "test".to_string(),
        title: "Add tests for {{target}}".to_string(),
        description: None,
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::High,
        tags: vec!["test".to_string(), "{{module}}".to_string()],
        scope: vec!["{{target}}".to_string()],
        evidence: vec!["Need tests for {{file}}".to_string()],
        plan: vec![
            "Analyze {{target}}".to_string(),
            "Test {{module}}".to_string(),
        ],
        notes: vec!["Branch: {{branch}}".to_string()],
        request: Some("Add tests for {{target}}".to_string()),
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    let context = TemplateContext {
        target: Some("src/main.rs".to_string()),
        module: Some("main".to_string()),
        file: Some("main.rs".to_string()),
        branch: Some("feature-branch".to_string()),
    };

    substitute_variables_in_task(&mut task, &context);

    assert_eq!(task.title, "Add tests for src/main.rs");
    assert_eq!(task.tags, vec!["test", "main"]);
    assert_eq!(task.scope, vec!["src/main.rs"]);
    assert_eq!(task.evidence, vec!["Need tests for main.rs"]);
    assert_eq!(task.plan, vec!["Analyze src/main.rs", "Test main"]);
    assert_eq!(task.notes, vec!["Branch: feature-branch"]);
    assert_eq!(task.request, Some("Add tests for src/main.rs".to_string()));
}

#[test]
fn test_extract_variables() {
    let input = "{{target}} and {{module}} and {{unknown}}";
    let vars = extract_variables(input);
    assert!(vars.contains("target"));
    assert!(vars.contains("module"));
    assert!(vars.contains("unknown"));
    assert!(!vars.contains("file"));
}

#[test]
fn test_extract_variables_empty() {
    let input = "no variables here";
    let vars = extract_variables(input);
    assert!(vars.is_empty());
}

#[test]
fn test_validate_task_template_unknown_variables() {
    let task = crate::contracts::Task {
        id: "test".to_string(),
        title: "Fix {{target}} and {{unknown_var}}".to_string(),
        description: None,
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::High,
        tags: vec!["{{another_unknown}}".to_string()],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: Some("Check {{unknown_var}}".to_string()),
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    let validation = validate_task_template(&task);

    // Should have warnings for unknown_var and another_unknown
    assert!(validation.has_unknown_variables());
    let unknown_names = validation.unknown_variable_names();
    assert!(unknown_names.contains(&"unknown_var".to_string()));
    assert!(unknown_names.contains(&"another_unknown".to_string()));
}

#[test]
fn test_validate_task_template_uses_branch() {
    let task = crate::contracts::Task {
        id: "test".to_string(),
        title: "Fix on {{branch}}".to_string(),
        description: None,
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::High,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    let validation = validate_task_template(&task);
    assert!(validation.uses_branch);
}

#[test]
fn test_validate_task_template_no_branch() {
    let task = crate::contracts::Task {
        id: "test".to_string(),
        title: "Fix {{target}}".to_string(),
        description: None,
        status: crate::contracts::TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::High,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    };

    let validation = validate_task_template(&task);
    assert!(!validation.uses_branch);
}

#[test]
fn test_detect_context_skips_git_when_not_needed() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    // Not a git repo, but we don't need branch
    let (context, warnings) = detect_context_with_warnings(None, repo_root, false);

    assert!(context.branch.is_none());
    // Should have no warnings since we didn't try git detection
    assert!(warnings.is_empty());
}

#[test]
fn test_detect_context_warns_when_branch_lookup_fails() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    let (context, warnings) = detect_context_with_warnings(None, repo_root, true);

    assert!(context.branch.is_none());
    assert_eq!(warnings.len(), 1);
    assert!(matches!(
        warnings.first(),
        Some(TemplateWarning::GitBranchDetectionFailed { .. })
    ));
}

#[test]
fn test_template_warning_display() {
    let w1 = TemplateWarning::UnknownVariable {
        name: "foo".to_string(),
        field: None,
    };
    assert_eq!(w1.to_string(), "Unknown template variable: {{foo}}");

    let w2 = TemplateWarning::UnknownVariable {
        name: "bar".to_string(),
        field: Some("title".to_string()),
    };
    assert_eq!(
        w2.to_string(),
        "Unknown template variable in title: {{bar}}"
    );

    let w3 = TemplateWarning::GitBranchDetectionFailed {
        error: "not a git repo".to_string(),
    };
    assert_eq!(
        w3.to_string(),
        "Git branch detection failed: not a git repo"
    );
}
