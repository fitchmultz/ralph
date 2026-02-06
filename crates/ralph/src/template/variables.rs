//! Template variable substitution for dynamic task fields.
//!
//! Responsibilities:
//! - Define supported template variables ({{target}}, {{module}}, {{file}}, {{branch}}).
//! - Substitute variables in template strings with context-aware values.
//! - Auto-detect context from git and filesystem.
//! - Validate templates and report warnings for unknown variables.
//!
//! Not handled here:
//! - Template loading (see `loader.rs`).
//! - Template merging (see `merge.rs`).
//!
//! Invariants/assumptions:
//! - Variable syntax is {{variable_name}}.
//! - Unknown variables are left as-is by default (not an error).
//! - Use strict mode to fail on unknown variables.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;

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

/// Warning types for template validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateWarning {
    /// Unknown template variable found (variable name, optional field context)
    UnknownVariable { name: String, field: Option<String> },
    /// Git branch detection failed (error message)
    GitBranchDetectionFailed { error: String },
}

impl std::fmt::Display for TemplateWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateWarning::UnknownVariable { name, field: None } => {
                write!(f, "Unknown template variable: {{{{{}}}}}", name)
            }
            TemplateWarning::UnknownVariable {
                name,
                field: Some(field),
            } => {
                write!(
                    f,
                    "Unknown template variable in {}: {{{{{}}}}}",
                    field, name
                )
            }
            TemplateWarning::GitBranchDetectionFailed { error } => {
                write!(f, "Git branch detection failed: {}", error)
            }
        }
    }
}

/// Result of template validation
#[derive(Debug, Clone, Default)]
pub struct TemplateValidation {
    /// Warnings collected during validation
    pub warnings: Vec<TemplateWarning>,
    /// Whether the template uses {{branch}} variable
    pub uses_branch: bool,
}

impl TemplateValidation {
    /// Check if there are any unknown variable warnings
    pub fn has_unknown_variables(&self) -> bool {
        self.warnings
            .iter()
            .any(|w| matches!(w, TemplateWarning::UnknownVariable { .. }))
    }

    /// Get list of unknown variable names (deduplicated)
    pub fn unknown_variable_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .warnings
            .iter()
            .filter_map(|w| match w {
                TemplateWarning::UnknownVariable { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// The set of known/supported template variables
const KNOWN_VARIABLES: &[&str] = &["target", "module", "file", "branch"];

/// Extract template variable occurrences from a string
///
/// Returns a set of variable names found in the input (without braces).
fn extract_variables(input: &str) -> HashSet<String> {
    let mut variables = HashSet::new();
    // Use lazy_static or thread_local for regex if performance is critical,
    // but for template loading (not hot path), we can compile on demand.
    // This function is called infrequently during template loading.
    let re = match Regex::new(r"\{\{(\w+)\}\}") {
        Ok(re) => re,
        Err(_) => return variables, // Should never happen with static pattern
    };

    for cap in re.captures_iter(input) {
        if let Some(matched) = cap.get(1) {
            variables.insert(matched.as_str().to_string());
        }
    }
    variables
}

/// Check if the input contains the {{branch}} variable
fn uses_branch_variable(input: &str) -> bool {
    input.contains("{{branch}}")
}

/// Validate a template task and collect warnings
///
/// This scans all string fields in the task for:
/// - Unknown template variables (not in KNOWN_VARIABLES)
/// - Presence of {{branch}} variable (to determine if git detection is needed)
pub fn validate_task_template(task: &crate::contracts::Task) -> TemplateValidation {
    let mut validation = TemplateValidation::default();
    let mut all_variables: HashSet<String> = HashSet::new();

    // Collect variables from all string fields
    let fields = [
        ("title", task.title.clone()),
        ("request", task.request.clone().unwrap_or_default()),
    ];

    for (field_name, value) in fields.iter() {
        if uses_branch_variable(value) {
            validation.uses_branch = true;
        }
        let vars = extract_variables(value);
        for var in &vars {
            if !KNOWN_VARIABLES.contains(&var.as_str()) {
                validation.warnings.push(TemplateWarning::UnknownVariable {
                    name: var.clone(),
                    field: Some(field_name.to_string()),
                });
            }
            all_variables.insert(var.clone());
        }
    }

    // Check array fields
    let array_fields: [(&str, &[String]); 5] = [
        ("tags", &task.tags),
        ("scope", &task.scope),
        ("evidence", &task.evidence),
        ("plan", &task.plan),
        ("notes", &task.notes),
    ];

    for (field_name, values) in array_fields.iter() {
        for value in *values {
            if uses_branch_variable(value) {
                validation.uses_branch = true;
            }
            let vars = extract_variables(value);
            for var in &vars {
                if !KNOWN_VARIABLES.contains(&var.as_str()) {
                    validation.warnings.push(TemplateWarning::UnknownVariable {
                        name: var.clone(),
                        field: Some(field_name.to_string()),
                    });
                }
                all_variables.insert(var.clone());
            }
        }
    }

    validation
}

/// Detect context from target path and git repository
///
/// Returns the context and any warnings (e.g., git branch detection failures).
/// Only attempts git branch detection if the template uses {{branch}}.
pub fn detect_context_with_warnings(
    target: Option<&str>,
    repo_root: &Path,
    needs_branch: bool,
) -> (TemplateContext, Vec<TemplateWarning>) {
    let mut warnings = Vec::new();
    let target_opt = target.map(|s| s.to_string());

    let file = target_opt.as_ref().map(|t| {
        Path::new(t)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t.clone())
    });

    let module = target_opt.as_ref().map(|t| derive_module_name(t));

    let branch = if needs_branch {
        match detect_git_branch(repo_root) {
            Ok(branch_opt) => branch_opt,
            Err(e) => {
                warnings.push(TemplateWarning::GitBranchDetectionFailed {
                    error: e.to_string(),
                });
                None
            }
        }
    } else {
        None
    };

    let context = TemplateContext {
        target: target_opt,
        file,
        module,
        branch,
    };

    (context, warnings)
}

/// Detect context from target path and git repository (legacy, ignores warnings)
pub fn detect_context(target: Option<&str>, repo_root: &Path) -> TemplateContext {
    let (context, _) = detect_context_with_warnings(target, repo_root, true);
    context
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
        // Try to find .git in parent directories using git command
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_root)
            .output()
            .context("failed to execute git command")?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch != "HEAD" {
                return Ok(Some(branch));
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("git rev-parse failed: {}", stderr.trim()));
        }
        return Ok(None);
    }

    let head_content = std::fs::read_to_string(&head_path)
        .with_context(|| format!("failed to read {:?}", head_path))?;
    let head_ref = head_content.trim();

    // HEAD content is like: "ref: refs/heads/main"
    if head_ref.starts_with("ref: refs/heads/") {
        let branch = head_ref
            .strip_prefix("ref: refs/heads/")
            .unwrap_or(head_ref)
            .to_string();
        Ok(Some(branch))
    } else if head_ref.len() == 40 && head_ref.chars().all(|c| c.is_ascii_hexdigit()) {
        // Detached HEAD state (40-character hex commit SHA)
        Ok(None)
    } else if head_ref.is_empty() {
        Err(anyhow::anyhow!("HEAD file is empty"))
    } else {
        // Invalid HEAD content
        Err(anyhow::anyhow!("invalid HEAD content: {}", head_ref))
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
}
