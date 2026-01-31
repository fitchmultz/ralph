//! Task builder and updater prompt tests.
//!
//! Responsibilities: validate task builder and updater prompt rendering and loading.
//! Not handled: worker prompts, scan prompts, or phase-specific rendering.
//! Invariants/assumptions: embedded defaults mention next-id command; temp directories are writable.

use super::*;

#[test]
fn render_task_builder_prompt_replaces_placeholders() -> Result<()> {
    let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let rendered = render_task_builder_prompt(
        template,
        "do thing",
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("do thing"));
    assert!(rendered.contains("code"));
    assert!(rendered.contains("repo"));
    assert!(!rendered.contains("{{USER_REQUEST}}"));
    Ok(())
}

#[test]
fn render_task_builder_prompt_allows_placeholder_like_request() -> Result<()> {
    let template = "Request:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let request = "use {{config.agent.model}}";
    let rendered = render_task_builder_prompt(
        template,
        request,
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains(request));
    Ok(())
}

#[test]
fn default_task_builder_prompt_mentions_next_id_command() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_task_builder_prompt(dir.path())?;
    assert!(prompt.contains("ralph queue next-id"));
    assert!(!prompt.contains("ralph queue next` for each new task ID"));
    Ok(())
}

#[test]
fn render_iteration_checklist_replaces_task_id() -> Result<()> {
    let template = "ID={{TASK_ID}}\n";
    let config = default_config();
    let rendered = render_iteration_checklist(template, "RQ-0001", &config)?;
    assert!(rendered.contains("ID=RQ-0001"));
    assert!(!rendered.contains("{{TASK_ID}}"));
    Ok(())
}
