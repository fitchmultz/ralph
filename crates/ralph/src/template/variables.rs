//! Template variable substitution for dynamic task fields.
//!
//! Responsibilities:
//! - Define supported template variables ({{target}}, {{module}}, {{file}}, {{branch}}).
//! - Substitute variables in template strings with context-aware values.
//! - Auto-detect context from git and filesystem.
//!
//! Not handled here:
//! - Template loading (see `loader.rs`).
//! - Template merging (see `merge.rs`).
//!
//! Invariants/assumptions:
//! - Variable syntax is {{variable_name}}.
//! - Unknown variables are left as-is (not an error).

use std::path::Path;

use anyhow::Result;

/// Context for template variable substitution
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    /// The target file/path provided by user
    pub target: Option<String>,
    /// Module name derived from target (e.g., "src/cli/task.rs" -> "cli::task")
    pub module: Option<String>,
    /// Filename only (e.g., "src/cli/task.rs" -> "task.rs")
    pub file: Option<String>,
    /// Current git branch name
    pub branch: Option<String>,
}

/// Detect context from target path and git repository
pub fn detect_context(target: Option<&str>, repo_root: &Path) -> TemplateContext {
    let target_opt = target.map(|s| s.to_string());

    let file = target_opt.as_ref().map(|t| {
        Path::new(t)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t.clone())
    });

    let module = target_opt.as_ref().map(|t| derive_module_name(t));

    let branch = detect_git_branch(repo_root).ok().flatten();

    TemplateContext {
        target: target_opt,
        file,
        module,
        branch,
    }
}

/// Derive a module name from a file path
///
/// Examples:
/// - "src/cli/task.rs" -> "cli::task"
/// - "crates/ralph/src/main.rs" -> "ralph::main"
/// - "lib/utils.js" -> "utils"
fn derive_module_name(path: &str) -> String {
    let path_obj = Path::new(path);

    // Get the file stem (filename without extension)
    let file_stem = path_obj
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    // Collect path components that might be module names
    let mut components: Vec<String> = Vec::new();

    // Walk through parent directories looking for meaningful names
    for component in path_obj.components() {
        let comp_str = component.as_os_str().to_string_lossy().to_string();

        // Skip common non-module directories
        if comp_str == "src"
            || comp_str == "lib"
            || comp_str == "bin"
            || comp_str == "tests"
            || comp_str == "examples"
            || comp_str == "crates"
        {
            continue;
        }

        // Skip the filename itself (we use file_stem separately)
        if comp_str
            == path_obj
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        {
            continue;
        }

        components.push(comp_str);
    }

    // If we found meaningful components, combine with file stem
    if !components.is_empty() {
        components.push(file_stem);
        components.join("::")
    } else {
        file_stem
    }
}

/// Detect the current git branch name
fn detect_git_branch(repo_root: &Path) -> Result<Option<String>> {
    // Try to read from git HEAD
    let head_path = repo_root.join(".git/HEAD");

    if !head_path.exists() {
        // Try to find .git in parent directories
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_root)
            .output()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch != "HEAD" {
                return Ok(Some(branch));
            }
        }
        return Ok(None);
    }

    let head_content = std::fs::read_to_string(&head_path)?;
    let head_ref = head_content.trim();

    // HEAD content is like: "ref: refs/heads/main"
    if head_ref.starts_with("ref: refs/heads/") {
        let branch = head_ref
            .strip_prefix("ref: refs/heads/")
            .unwrap_or(head_ref)
            .to_string();
        Ok(Some(branch))
    } else {
        // Detached HEAD state
        Ok(None)
    }
}

/// Substitute variables in a template string
///
/// Supported variables:
/// - {{target}} - The target file/path provided by user
/// - {{module}} - Module name derived from target
/// - {{file}} - Filename only
/// - {{branch}} - Current git branch name
pub fn substitute_variables(input: &str, context: &TemplateContext) -> String {
    let mut result = input.to_string();

    if let Some(target) = &context.target {
        result = result.replace("{{target}}", target);
    }

    if let Some(module) = &context.module {
        result = result.replace("{{module}}", module);
    }

    if let Some(file) = &context.file {
        result = result.replace("{{file}}", file);
    }

    if let Some(branch) = &context.branch {
        result = result.replace("{{branch}}", branch);
    }

    result
}

/// Substitute variables in all string fields of a Task
pub fn substitute_variables_in_task(task: &mut crate::contracts::Task, context: &TemplateContext) {
    task.title = substitute_variables(&task.title, context);

    for tag in &mut task.tags {
        *tag = substitute_variables(tag, context);
    }

    for scope in &mut task.scope {
        *scope = substitute_variables(scope, context);
    }

    for evidence in &mut task.evidence {
        *evidence = substitute_variables(evidence, context);
    }

    for plan in &mut task.plan {
        *plan = substitute_variables(plan, context);
    }

    for note in &mut task.notes {
        *note = substitute_variables(note, context);
    }

    if let Some(request) = &mut task.request {
        *request = substitute_variables(request, context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Initialize a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .expect("Failed to init git repo");

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
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
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
}
