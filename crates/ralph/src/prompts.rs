use crate::contracts::ProjectType;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::path::Path;

const WORKER_PROMPT_REL_PATH: &str = ".ralph/prompts/worker.md";
const TASK_BUILDER_PROMPT_REL_PATH: &str = ".ralph/prompts/task_builder.md";
const SCAN_PROMPT_REL_PATH: &str = ".ralph/prompts/scan.md";

const DEFAULT_WORKER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/worker.md"
));
const DEFAULT_TASK_BUILDER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/task_builder.md"
));
const DEFAULT_SCAN_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/scan.md"
));

/// Instructions for planning phase: set task status to "doing" as the first action
pub const TASK_STATUS_DOING_INSTRUCTION: &str = r#"
## PLANNING PHASE EXCEPTION: Task Status Update
As the FIRST action, you MUST update the task status:
1. Read `.ralph/queue.yaml`
2. Find the first `todo` task (this is your task)
3. Set its `status` to `doing`
4. Set its `updated_at` to current UTC RFC3339 time
5. Write the updated `.ralph/queue.yaml`

This is the ONLY edit allowed during planning. After this status update, proceed with read-only exploration.
"#;

/// Instructions for implementation phase: complete task and move to done.yaml
pub const TASK_COMPLETION_WORKFLOW: &str = r#"
## IMPLEMENTATION COMPLETION CHECKLIST
When implementation is complete, you MUST:
1. Set task `status: done` with `completed_at` timestamp
2. Add 1-5 `notes` bullets (what changed, how to verify, what's next)
3. Move task from `.ralph/queue.yaml` to END of `.ralph/done.yaml`
4. Run `make ci` - must pass 100%
5. Commit all changes: `RQ-####: <short summary>`
6. Push and verify `git status --porcelain` is empty
"#;

pub fn prompts_reference_readme(repo_root: &Path) -> Result<bool> {
    let worker = load_worker_prompt(repo_root)?;
    let task_builder = load_task_builder_prompt(repo_root)?;
    let scan = load_scan_prompt(repo_root)?;

    Ok(worker.contains(".ralph/README.md")
        || task_builder.contains(".ralph/README.md")
        || scan.contains(".ralph/README.md"))
}

pub fn load_worker_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        WORKER_PROMPT_REL_PATH,
        DEFAULT_WORKER_PROMPT,
        "worker",
    )
}

fn project_type_guidance(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Code => {
            r#"
## PROJECT TYPE: CODE

This is a code repository. Prioritize:
- Implementation correctness and type safety
- Test coverage and regression prevention
- Performance and resource efficiency
- Clean, maintainable code structure
"#
        }
        ProjectType::Docs => {
            r#"
## PROJECT TYPE: DOCS

This is a documentation repository. Prioritize:
- Clear, accurate information
- Consistent formatting and structure
- Accessibility and readability
- Examples and practical guidance
"#
        }
    }
}

pub fn render_worker_prompt(template: &str, project_type: ProjectType) -> Result<String> {
    let guidance = project_type_guidance(project_type);
    let rendered = if template.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        template.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", template, guidance)
    };
    let rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    ensure_no_unresolved_placeholders(&rendered, "worker")?;
    Ok(rendered)
}

pub fn load_task_builder_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        TASK_BUILDER_PROMPT_REL_PATH,
        DEFAULT_TASK_BUILDER_PROMPT,
        "task builder",
    )
}

pub fn render_task_builder_prompt(
    template: &str,
    user_request: &str,
    hint_tags: &str,
    hint_scope: &str,
    project_type: ProjectType,
) -> Result<String> {
    if !template.contains("{{USER_REQUEST}}") {
        bail!("Template error: task builder prompt template is missing the required '{{USER_REQUEST}}' placeholder. Ensure the template in .ralph/prompts/task_builder.md includes this placeholder.");
    }
    if !template.contains("{{HINT_TAGS}}") {
        bail!("Template error: task builder prompt template is missing the required '{{HINT_TAGS}}' placeholder. Ensure the template includes this placeholder.");
    }
    if !template.contains("{{HINT_SCOPE}}") {
        bail!("Template error: task builder prompt template is missing the required '{{HINT_SCOPE}}' placeholder. Ensure the template includes this placeholder.");
    }

    let request = user_request.trim();
    if request.is_empty() {
        bail!("Missing request: user request must be non-empty. Provide a descriptive request for the task builder.");
    }

    let guidance = project_type_guidance(project_type);
    let mut rendered = if template.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        template.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", template, guidance)
    };
    rendered = rendered.replace("{{USER_REQUEST}}", request);
    rendered = rendered.replace("{{HINT_TAGS}}", hint_tags.trim());
    rendered = rendered.replace("{{HINT_SCOPE}}", hint_scope.trim());
    rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    ensure_no_unresolved_placeholders(&rendered, "task builder")?;
    Ok(rendered)
}

pub fn load_scan_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(repo_root, SCAN_PROMPT_REL_PATH, DEFAULT_SCAN_PROMPT, "scan")
}

pub fn render_scan_prompt(
    template: &str,
    user_focus: &str,
    project_type: ProjectType,
) -> Result<String> {
    if !template.contains("{{USER_FOCUS}}") {
        bail!("Template error: scan prompt template is missing the required '{{USER_FOCUS}}' placeholder. Ensure the template in .ralph/prompts/scan.md includes this placeholder.");
    }
    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };

    let guidance = project_type_guidance(project_type);
    let rendered = if template.contains("{{PROJECT_TYPE_GUIDANCE}}") {
        template.replace("{{PROJECT_TYPE_GUIDANCE}}", guidance)
    } else {
        format!("{}\n{}", template, guidance)
    };
    let rendered = rendered.replace("{{USER_FOCUS}}", focus);
    ensure_no_unresolved_placeholders(&rendered, "scan")?;
    Ok(rendered)
}

fn unresolved_placeholders(rendered: &str) -> Vec<String> {
    let mut placeholders = Vec::new();
    let bytes = rendered.as_bytes();
    let mut i = 0;

    while i < bytes.len().saturating_sub(3) {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = bytes[i..].iter().position(|&b| b == b'}') {
                let end_idx = i + end;
                if end_idx < bytes.len().saturating_sub(1) && bytes[end_idx + 1] == b'}' {
                    let placeholder = &rendered[i..end_idx + 2];
                    let trimmed = placeholder.trim_matches(|c| c == '{' || c == '}');
                    if !trimmed.is_empty() {
                        placeholders.push(trimmed.to_uppercase());
                    }
                    i = end_idx + 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    let mut unique: Vec<String> = placeholders
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    unique.sort();
    unique
}

fn ensure_no_unresolved_placeholders(rendered: &str, label: &str) -> Result<()> {
    let placeholders = unresolved_placeholders(rendered);
    if !placeholders.is_empty() {
        bail!(
            "Prompt validation failed for {}: unresolved placeholders remain after rendering: {}. Review the {} prompt template and ensure all placeholders are either removed or correctly formatted.",
            label,
            placeholders.join(", "),
            label
        );
    }
    Ok(())
}

fn load_prompt_with_fallback(
    repo_root: &Path,
    rel_path: &str,
    embedded_default: &'static str,
    label: &str,
) -> Result<String> {
    let path = repo_root.join(rel_path);
    match fs::read_to_string(&path) {
        Ok(contents) => Ok(contents),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(embedded_default.to_string()),
        Err(err) => Err(err).with_context(|| format!("read {label} prompt {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn render_worker_prompt_replaces_interactive_instructions() -> Result<()> {
        let template = "Hello\n{{INTERACTIVE_INSTRUCTIONS}}\n";
        let rendered = render_worker_prompt(template, ProjectType::Code)?;
        assert!(!rendered.contains("{{INTERACTIVE_INSTRUCTIONS}}"));
        Ok(())
    }

    #[test]
    fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
        let template = "FOCUS:\n{{USER_FOCUS}}\n";
        let rendered = render_scan_prompt(template, "hello world", ProjectType::Code)?;
        assert!(rendered.contains("hello world"));
        assert!(!rendered.contains("{{USER_FOCUS}}"));
        Ok(())
    }

    #[test]
    fn render_task_builder_prompt_replaces_placeholders() -> Result<()> {
        let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
        let rendered =
            render_task_builder_prompt(template, "do thing", "code", "repo", ProjectType::Code)?;
        assert!(rendered.contains("do thing"));
        assert!(rendered.contains("code"));
        assert!(rendered.contains("repo"));
        assert!(!rendered.contains("{{USER_REQUEST}}"));
        Ok(())
    }

    #[test]
    fn load_worker_prompt_falls_back_to_embedded_default_when_missing() -> Result<()> {
        let dir = TempDir::new()?;
        let prompt = load_worker_prompt(dir.path())?;
        assert!(prompt.contains("# MISSION"));
        Ok(())
    }

    #[test]
    fn load_worker_prompt_uses_override_when_present() -> Result<()> {
        let dir = TempDir::new()?;
        let overrides = dir.path().join(".ralph/prompts");
        fs::create_dir_all(&overrides)?;
        fs::write(overrides.join("worker.md"), "override")?;
        let prompt = load_worker_prompt(dir.path())?;
        assert_eq!(prompt, "override");
        Ok(())
    }

    #[test]
    fn ensure_no_unresolved_placeholders_passes_when_none_remain() -> Result<()> {
        let rendered = "Hello world";
        assert!(ensure_no_unresolved_placeholders(rendered, "test").is_ok());
        Ok(())
    }

    #[test]
    fn ensure_no_unresolved_placeholders_fails_with_placeholder() -> Result<()> {
        let rendered = "Hello {{MISSING}} world";
        let err = ensure_no_unresolved_placeholders(rendered, "test").unwrap_err();
        assert!(err.to_string().contains("MISSING"));
        assert!(err.to_string().contains("unresolved placeholders"));
        Ok(())
    }

    #[test]
    fn unresolved_placeholders_finds_all_placeholders() {
        let rendered = "Test {{ONE}} and {{TWO}} and {{three}}";
        let placeholders = unresolved_placeholders(rendered);
        assert_eq!(placeholders.len(), 3);
        assert!(placeholders.contains(&"ONE".to_string()));
        assert!(placeholders.contains(&"TWO".to_string()));
        assert!(placeholders.contains(&"THREE".to_string()));
    }

    #[test]
    fn unresolved_placeholders_returns_sorted_unique() {
        let rendered = "Test {{Z}} and {{A}} and {{B}} and {{A}}";
        let placeholders = unresolved_placeholders(rendered);
        assert_eq!(placeholders, vec!["A", "B", "Z"]);
    }
}
