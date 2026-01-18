use anyhow::{bail, Context, Result};
use std::fs;
use std::io;
use std::path::Path;

const WORKER_PROMPT_REL_PATH: &str = "ralph/prompts/worker.md";
const TASK_BUILDER_PROMPT_REL_PATH: &str = "ralph/prompts/task_builder.md";
const SCAN_PROMPT_REL_PATH: &str = "ralph/prompts/scan.md";

const DEFAULT_WORKER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/worker.md"
));
const DEFAULT_TASK_BUILDER_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/prompts/task_builder.md"
));
const DEFAULT_SCAN_PROMPT: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/prompts/scan.md"));

pub fn load_worker_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        WORKER_PROMPT_REL_PATH,
        DEFAULT_WORKER_PROMPT,
        "worker",
    )
}

pub fn render_worker_prompt(template: &str) -> Result<String> {
    Ok(template.replace("{{INTERACTIVE_INSTRUCTIONS}}", ""))
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
) -> Result<String> {
    if !template.contains("{{USER_REQUEST}}") {
        bail!("task builder prompt template missing {{USER_REQUEST}} placeholder");
    }
    if !template.contains("{{HINT_TAGS}}") {
        bail!("task builder prompt template missing {{HINT_TAGS}} placeholder");
    }
    if !template.contains("{{HINT_SCOPE}}") {
        bail!("task builder prompt template missing {{HINT_SCOPE}} placeholder");
    }

    let request = user_request.trim();
    if request.is_empty() {
        bail!("user request must be non-empty");
    }

    let mut rendered = template.replace("{{USER_REQUEST}}", request);
    rendered = rendered.replace("{{HINT_TAGS}}", hint_tags.trim());
    rendered = rendered.replace("{{HINT_SCOPE}}", hint_scope.trim());
    rendered = rendered.replace("{{INTERACTIVE_INSTRUCTIONS}}", "");
    Ok(rendered)
}

pub fn load_scan_prompt(repo_root: &Path) -> Result<String> {
    load_prompt_with_fallback(
        repo_root,
        SCAN_PROMPT_REL_PATH,
        DEFAULT_SCAN_PROMPT,
        "scan",
    )
}

pub fn render_scan_prompt(template: &str, user_focus: &str) -> Result<String> {
    if !template.contains("{{USER_FOCUS}}") {
        bail!("scan prompt template missing {{USER_FOCUS}} placeholder");
    }
    let focus = user_focus.trim();
    let focus = if focus.is_empty() { "(none)" } else { focus };
    Ok(template.replace("{{USER_FOCUS}}", focus))
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
        let rendered = render_worker_prompt(template)?;
        assert!(!rendered.contains("{{INTERACTIVE_INSTRUCTIONS}}"));
        Ok(())
    }

    #[test]
    fn render_scan_prompt_replaces_focus_placeholder() -> Result<()> {
        let template = "FOCUS:\n{{USER_FOCUS}}\n";
        let rendered = render_scan_prompt(template, "hello world")?;
        assert!(rendered.contains("hello world"));
        assert!(!rendered.contains("{{USER_FOCUS}}"));
        Ok(())
    }

    #[test]
    fn render_task_builder_prompt_replaces_placeholders() -> Result<()> {
        let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
        let rendered = render_task_builder_prompt(template, "do thing", "code", "repo")?;
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
        let overrides = dir.path().join("ralph/prompts");
        fs::create_dir_all(&overrides)?;
        fs::write(overrides.join("worker.md"), "override")?;
        let prompt = load_worker_prompt(dir.path())?;
        assert_eq!(prompt, "override");
        Ok(())
    }
}
