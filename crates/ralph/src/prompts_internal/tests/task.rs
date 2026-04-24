//! Task builder and updater prompt tests.
//!
//! Purpose:
//! - Task builder and updater prompt tests.
//!
//! Responsibilities: validate task builder and updater prompt rendering and loading.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled: worker prompts, scan prompts, or phase-specific rendering.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions: embedded defaults mention next-id command; temp directories are writable.

use super::*;
use crate::commands::task::DecompositionChildPolicy;
use crate::prompts_internal::task_updater::render_task_updater_prompt;

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
    // Should mention --count for multi-task cases
    assert!(
        prompt.contains("next-id --count"),
        "prompt should mention next-id --count for multi-task creation"
    );
    // Should warn that next-id does not reserve IDs
    assert!(
        prompt.contains("does NOT reserve IDs") || prompt.contains("does not reserve IDs"),
        "prompt should warn that next-id does not reserve IDs"
    );
    Ok(())
}

#[test]
fn default_task_builder_prompt_treats_discovery_as_queue_shaping() -> Result<()> {
    let dir = TempDir::new()?;
    let prompt = load_task_builder_prompt(dir.path())?;
    assert!(prompt.contains("DISCOVERY / QUEUE-SHAPING REQUESTS"));
    assert!(prompt.contains("queue-shaping by default"));
    assert!(prompt.contains("Do not create \"write a report\" tasks"));
    assert!(prompt.contains("actionable follow-up tasks"));
    Ok(())
}

#[test]
fn render_task_builder_prompt_expands_queue_file_variable() -> Result<()> {
    let template = "Queue: {{config.queue.file}}\nRequest:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let mut config = default_config();
    config.queue.file = Some(std::path::PathBuf::from(".ralph/custom_queue.jsonc"));
    let rendered = render_task_builder_prompt(
        template,
        "do thing",
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("Queue: .ralph/custom_queue.jsonc"));
    Ok(())
}

#[test]
fn render_task_builder_prompt_uses_default_queue_file_when_unset() -> Result<()> {
    let template = "Queue: {{config.queue.file}}\nRequest:\n{{USER_REQUEST}}\nTags:\n{{HINT_TAGS}}\nScope:\n{{HINT_SCOPE}}\n";
    let config = default_config();
    let rendered = render_task_builder_prompt(
        template,
        "do thing",
        "code",
        "repo",
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("Queue: .ralph/queue.jsonc"));
    assert!(!rendered.contains("{{config.queue.file}}"));
    Ok(())
}

#[test]
fn render_task_updater_prompt_expands_queue_and_done_file_variables() -> Result<()> {
    let template =
        "Queue: {{config.queue.file}}\nDone: {{config.queue.done_file}}\nTask: {{TASK_ID}}";
    let mut config = default_config();
    config.queue.file = Some(std::path::PathBuf::from(".ralph/custom_queue.jsonc"));
    config.queue.done_file = Some(std::path::PathBuf::from(".ralph/custom_done.jsonc"));
    let rendered = render_task_updater_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("Queue: .ralph/custom_queue.jsonc"));
    assert!(rendered.contains("Done: .ralph/custom_done.jsonc"));
    assert!(rendered.contains("Task: RQ-0001"));
    Ok(())
}

#[test]
fn render_task_updater_prompt_uses_default_queue_and_done_when_unset() -> Result<()> {
    let template =
        "Queue: {{config.queue.file}}\nDone: {{config.queue.done_file}}\nTask: {{TASK_ID}}";
    let config = default_config();
    let rendered = render_task_updater_prompt(template, "RQ-0001", ProjectType::Code, &config)?;
    assert!(rendered.contains("Queue: .ralph/queue.jsonc"));
    assert!(rendered.contains("Done: .ralph/done.jsonc"));
    assert!(!rendered.contains("{{config.queue.file}}"));
    assert!(!rendered.contains("{{config.queue.done_file}}"));
    assert!(rendered.contains("Task: RQ-0001"));
    Ok(())
}

#[test]
fn render_task_decompose_prompt_replaces_placeholders() -> Result<()> {
    let template = concat!(
        "Mode={{SOURCE_MODE}}\n",
        "Request={{SOURCE_REQUEST}}\n",
        "Task={{SOURCE_TASK_JSON}}\n",
        "Attach={{ATTACH_TARGET_JSON}}\n",
        "Depth={{MAX_DEPTH}}\n",
        "Children={{MAX_CHILDREN}}\n",
        "Nodes={{MAX_NODES}}\n",
        "Policy={{CHILD_POLICY}}\n",
        "Deps={{WITH_DEPENDENCIES}}\n",
    );
    let config = default_config();
    let rendered = render_task_decompose_prompt(
        template,
        "freeform",
        "Ship OAuth",
        "",
        "",
        3,
        5,
        20,
        DecompositionChildPolicy::Append,
        true,
        ProjectType::Code,
        &config,
    )?;
    assert!(rendered.contains("Mode=freeform"));
    assert!(rendered.contains("Request=Ship OAuth"));
    assert!(rendered.contains("Depth=3"));
    assert!(rendered.contains("Children=5"));
    assert!(rendered.contains("Nodes=20"));
    assert!(rendered.contains("Policy=append"));
    assert!(rendered.contains("Deps=true"));
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
