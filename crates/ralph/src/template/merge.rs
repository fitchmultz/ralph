//! Merge template fields with user-provided options.
//!
//! Responsibilities:
//! - Merge template tags and scope with user hints.
//! - Format template context for inclusion in task builder prompts.
//!
//! Not handled here:
//! - Template loading (see `loader.rs`).
//! - Task creation or queue operations (see `crate::commands::task`).
//!
//! Invariants/assumptions:
//! - Template fields are merged as defaults; user hints take precedence or append.
//! - Empty template fields are ignored during merge.

use crate::commands::task::TaskBuildOptions;
use crate::contracts::Task;

/// Merge template fields into build options
///
/// Template provides defaults; user hints override/append.
/// - Tags: template tags + user tags (deduplicated)
/// - Scope: template scope + user scope (deduplicated)
/// - Priority: user priority overrides template priority
pub fn merge_template_with_options(template: &Task, options: &mut TaskBuildOptions) {
    // Merge tags: template tags + user hint tags (deduplicate)
    if !template.tags.is_empty() {
        let template_tags: std::collections::HashSet<_> = template.tags.iter().cloned().collect();
        let user_tags: std::collections::HashSet<_> = options
            .hint_tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let merged: Vec<_> = template_tags.union(&user_tags).cloned().collect();
        options.hint_tags = merged.join(", ");
    }

    // Merge scope: template scope + user hint scope (deduplicate)
    if !template.scope.is_empty() {
        let template_scope: std::collections::HashSet<_> = template.scope.iter().cloned().collect();
        let user_scope: std::collections::HashSet<_> = options
            .hint_scope
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let merged: Vec<_> = template_scope.union(&user_scope).cloned().collect();
        options.hint_scope = merged.join(", ");
    }
}

/// Format template context for the task builder prompt
///
/// Returns a formatted string with template suggestions that can be
/// appended to the prompt to guide task creation.
pub fn format_template_context(template: &Task) -> String {
    let mut context = String::new();

    if !template.tags.is_empty() {
        context.push_str(&format!("Suggested tags: {}\n", template.tags.join(", ")));
    }
    if !template.scope.is_empty() {
        context.push_str(&format!("Suggested scope: {}\n", template.scope.join(", ")));
    }
    if template.priority != crate::contracts::TaskPriority::Medium {
        context.push_str(&format!("Suggested priority: {}\n", template.priority));
    }
    if !template.plan.is_empty() {
        context.push_str("Suggested plan:\n");
        for (i, step) in template.plan.iter().enumerate() {
            context.push_str(&format!("  {}. {}\n", i + 1, step));
        }
    }
    if !template.evidence.is_empty() {
        context.push_str(&format!(
            "Suggested evidence: {}\n",
            template.evidence.join(", ")
        ));
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority};

    fn create_test_task() -> Task {
        Task {
            id: "test".to_string(),
            title: "Test Task".to_string(),
            status: crate::contracts::TaskStatus::Todo,
            priority: TaskPriority::High,
            tags: vec!["bug".to_string(), "fix".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["Error logs".to_string()],
            plan: vec!["Step 1".to_string(), "Step 2".to_string()],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_merge_template_tags_with_empty_user_tags() {
        let template = create_test_task();
        let mut options = TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: String::new(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };

        merge_template_with_options(&template, &mut options);
        assert!(options.hint_tags.contains("bug"));
        assert!(options.hint_tags.contains("fix"));
    }

    #[test]
    fn test_merge_template_tags_with_user_tags() {
        let template = create_test_task();
        let mut options = TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: "critical, ui".to_string(),
            hint_scope: String::new(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };

        merge_template_with_options(&template, &mut options);
        // Should have both template tags and user tags
        assert!(options.hint_tags.contains("bug"));
        assert!(options.hint_tags.contains("critical"));
        assert!(options.hint_tags.contains("ui"));
    }

    #[test]
    fn test_merge_template_scope() {
        let template = create_test_task();
        let mut options = TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: String::new(),
            hint_scope: "docs".to_string(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };

        merge_template_with_options(&template, &mut options);
        assert!(options.hint_scope.contains("crates/ralph"));
        assert!(options.hint_scope.contains("docs"));
    }

    #[test]
    fn test_merge_with_empty_template() {
        let template = Task {
            id: "empty".to_string(),
            title: "Empty".to_string(),
            status: crate::contracts::TaskStatus::Todo,
            priority: TaskPriority::Medium,
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        };

        let mut options = TaskBuildOptions {
            request: "test".to_string(),
            hint_tags: "user-tag".to_string(),
            hint_scope: "user-scope".to_string(),
            runner_override: None,
            model_override: None,
            reasoning_effort_override: None,
            runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(),
            force: false,
            repoprompt_tool_injection: false,
            template_hint: None,
            template_target: None,
        };

        merge_template_with_options(&template, &mut options);
        // User tags should remain unchanged
        assert_eq!(options.hint_tags, "user-tag");
        assert_eq!(options.hint_scope, "user-scope");
    }

    #[test]
    fn test_format_template_context() {
        let template = create_test_task();
        let context = format_template_context(&template);

        assert!(context.contains("Suggested tags:"));
        assert!(context.contains("bug"));
        assert!(context.contains("Suggested scope:"));
        assert!(context.contains("crates/ralph"));
        assert!(context.contains("Suggested priority:"));
        assert!(context.contains("high"));
        assert!(context.contains("Suggested plan:"));
        assert!(context.contains("Step 1"));
        assert!(context.contains("Step 2"));
        assert!(context.contains("Suggested evidence:"));
        assert!(context.contains("Error logs"));
    }

    #[test]
    fn test_format_template_context_empty_fields() {
        let template = Task {
            id: "empty".to_string(),
            title: "Empty".to_string(),
            status: crate::contracts::TaskStatus::Todo,
            priority: TaskPriority::Medium, // Default priority - should not appear
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        };

        let context = format_template_context(&template);
        assert!(context.is_empty());
    }
}
