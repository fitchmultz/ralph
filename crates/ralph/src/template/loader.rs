//! Template loading with override support.
//!
//! Responsibilities:
//! - Load templates from `.ralph/templates/{name}.json` first.
//! - Fall back to built-in templates if no custom template exists.
//! - List all available templates (built-in + custom).
//!
//! Not handled here:
//! - Template content validation beyond JSON parsing.
//! - Template merging with user options (see `merge.rs`).
//!
//! Invariants/assumptions:
//! - Custom templates override built-ins with the same name.
//! - Template files must have `.json` extension.
//! - Template names are case-sensitive.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::contracts::Task;
use crate::template::builtin::{get_builtin_template, get_template_description};

/// Source of a loaded template
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateSource {
    /// Custom template from .ralph/templates/
    Custom(PathBuf),
    /// Built-in embedded template (stores the name, not the content)
    Builtin(String),
}

/// Metadata for a template (used for listing)
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub name: String,
    pub source: TemplateSource,
    pub description: String,
}

/// Error type for template operations
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),
    #[error("Failed to read template file: {0}")]
    ReadError(String),
    #[error("Invalid template JSON: {0}")]
    InvalidJson(String),
}

/// Load a template by name
///
/// Checks `.ralph/templates/{name}.json` first, then falls back to built-in templates.
pub fn load_template(name: &str, project_root: &Path) -> Result<(Task, TemplateSource)> {
    // Check for custom template first
    let custom_path = project_root
        .join(".ralph/templates")
        .join(format!("{}.json", name));
    if custom_path.exists() {
        let content = std::fs::read_to_string(&custom_path)
            .map_err(|e| TemplateError::ReadError(e.to_string()))?;
        let task: Task = serde_json::from_str(&content)
            .map_err(|e| TemplateError::InvalidJson(e.to_string()))?;
        return Ok((task, TemplateSource::Custom(custom_path)));
    }

    // Fall back to built-in
    if let Some(template_json) = get_builtin_template(name) {
        let task: Task = serde_json::from_str(template_json)
            .map_err(|e| TemplateError::InvalidJson(e.to_string()))?;
        return Ok((task, TemplateSource::Builtin(name.to_string())));
    }

    Err(TemplateError::NotFound(name.to_string()).into())
}

/// List all available templates (built-in + custom)
///
/// Custom templates override built-ins with the same name.
pub fn list_templates(project_root: &Path) -> Vec<TemplateInfo> {
    let mut templates = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // Add custom templates first (so they take precedence in listing)
    let custom_dir = project_root.join(".ralph/templates");
    if let Ok(entries) = std::fs::read_dir(&custom_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(name) = path.file_stem() {
                    let name = name.to_string_lossy().to_string();
                    seen_names.insert(name.clone());

                    // Try to read description from template if possible
                    let description = if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(task) = serde_json::from_str::<Task>(&content) {
                            // Use first plan item as description if available
                            task.plan
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "Custom template".to_string())
                        } else {
                            "Custom template".to_string()
                        }
                    } else {
                        "Custom template".to_string()
                    };

                    templates.push(TemplateInfo {
                        name,
                        source: TemplateSource::Custom(path),
                        description,
                    });
                }
            }
        }
    }

    // Add built-ins that aren't overridden
    for name in crate::template::builtin::list_builtin_templates() {
        if !seen_names.contains(name) {
            templates.push(TemplateInfo {
                name: name.to_string(),
                source: TemplateSource::Builtin(name.to_string()),
                description: get_template_description(name).to_string(),
            });
        }
    }

    // Sort by name for consistent ordering
    templates.sort_by(|a, b| a.name.cmp(&b.name));

    templates
}

/// Check if a template exists (either custom or built-in)
pub fn template_exists(name: &str, project_root: &Path) -> bool {
    let custom_path = project_root
        .join(".ralph/templates")
        .join(format!("{}.json", name));
    custom_path.exists() || get_builtin_template(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_project() -> TempDir {
        TempDir::new().expect("Failed to create temp dir")
    }

    #[test]
    fn test_load_builtin_template() {
        let temp_dir = create_test_project();
        let result = load_template("bug", temp_dir.path());
        assert!(result.is_ok());

        let (task, source) = result.unwrap();
        assert_eq!(task.priority, crate::contracts::TaskPriority::High);
        assert!(matches!(source, TemplateSource::Builtin(s) if s == "bug"));
    }

    #[test]
    fn test_load_custom_template() {
        let temp_dir = create_test_project();
        let templates_dir = temp_dir.path().join(".ralph/templates");
        std::fs::create_dir_all(&templates_dir).unwrap();

        let custom_template = r#"{
            "id": "",
            "title": "",
            "status": "todo",
            "priority": "critical",
            "tags": ["custom", "test"],
            "plan": ["Step 1", "Step 2"]
        }"#;

        let mut file = std::fs::File::create(templates_dir.join("custom.json")).unwrap();
        file.write_all(custom_template.as_bytes()).unwrap();

        let result = load_template("custom", temp_dir.path());
        assert!(result.is_ok());

        let (task, source) = result.unwrap();
        assert_eq!(task.priority, crate::contracts::TaskPriority::Critical);
        assert!(matches!(source, TemplateSource::Custom(_)));
    }

    #[test]
    fn test_custom_overrides_builtin() {
        let temp_dir = create_test_project();
        let templates_dir = temp_dir.path().join(".ralph/templates");
        std::fs::create_dir_all(&templates_dir).unwrap();

        // Create a custom "bug" template that overrides the built-in
        let custom_template = r#"{
            "id": "",
            "title": "",
            "status": "todo",
            "priority": "low",
            "tags": ["custom-bug"]
        }"#;

        let mut file = std::fs::File::create(templates_dir.join("bug.json")).unwrap();
        file.write_all(custom_template.as_bytes()).unwrap();

        let result = load_template("bug", temp_dir.path());
        assert!(result.is_ok());

        let (task, source) = result.unwrap();
        assert_eq!(task.priority, crate::contracts::TaskPriority::Low);
        assert!(matches!(source, TemplateSource::Custom(_)));
    }

    #[test]
    fn test_load_nonexistent_template() {
        let temp_dir = create_test_project();
        let result = load_template("nonexistent", temp_dir.path());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found") || err_msg.contains("NotFound"));
    }

    #[test]
    fn test_list_templates() {
        let temp_dir = create_test_project();
        let templates_dir = temp_dir.path().join(".ralph/templates");
        std::fs::create_dir_all(&templates_dir).unwrap();

        // Create a custom template
        let custom_template = r#"{"title": "", "priority": "low"}"#;
        let mut file = std::fs::File::create(templates_dir.join("custom.json")).unwrap();
        file.write_all(custom_template.as_bytes()).unwrap();

        let templates = list_templates(temp_dir.path());

        // Should have 5 built-ins + 1 custom = 6 total
        assert_eq!(templates.len(), 6);

        // Custom should be in the list
        assert!(templates.iter().any(|t| t.name == "custom"));

        // Built-ins should be in the list
        assert!(templates.iter().any(|t| t.name == "bug"));
        assert!(templates.iter().any(|t| t.name == "feature"));
    }

    #[test]
    fn test_template_exists() {
        let temp_dir = create_test_project();

        // Built-in should exist
        assert!(template_exists("bug", temp_dir.path()));
        assert!(template_exists("feature", temp_dir.path()));

        // Nonexistent should not exist
        assert!(!template_exists("nonexistent", temp_dir.path()));

        // Custom should exist after creation
        let templates_dir = temp_dir.path().join(".ralph/templates");
        std::fs::create_dir_all(&templates_dir).unwrap();
        let mut file = std::fs::File::create(templates_dir.join("custom.json")).unwrap();
        file.write_all(b"{}").unwrap();

        assert!(template_exists("custom", temp_dir.path()));
    }
}
