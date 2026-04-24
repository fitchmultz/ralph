//! Merge conflict prompt tests.
//!
//! Purpose:
//! - Merge conflict prompt tests.
//!
//! Responsibilities: validate merge conflict prompt rendering and placeholder replacement.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled: prompt loading via overrides or merge runner execution.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions: conflict file list must be non-empty.

use super::super::registry::{PromptTemplateId, prompt_template};
use super::*;

#[test]
fn render_merge_conflict_prompt_replaces_conflicts() -> Result<()> {
    let template = "Conflicts:\n{{CONFLICT_FILES}}\n";
    let files = vec!["src/lib.rs".to_string(), "README.md".to_string()];
    let config = default_config();
    let rendered = render_merge_conflict_prompt(template, &files, &config)?;
    assert!(rendered.contains("- src/lib.rs"));
    assert!(rendered.contains("- README.md"));
    assert!(!rendered.contains("{{CONFLICT_FILES}}"));
    Ok(())
}

#[test]
fn render_merge_conflict_prompt_rejects_empty_list() {
    let template = "Conflicts:\n{{CONFLICT_FILES}}\n";
    let config = default_config();
    let err = render_merge_conflict_prompt(template, &[], &config).unwrap_err();
    assert!(
        err.to_string()
            .contains("Conflict file list must be non-empty")
    );
}

#[test]
fn embedded_template_includes_queue_guidance() {
    let template_meta = prompt_template(PromptTemplateId::MergeConflicts);
    let embedded = template_meta.embedded_default;

    // Check for configured queue/done path placeholders
    assert!(
        embedded.contains("`{{config.queue.file}}`"),
        "Template should mention configured queue file placeholder"
    );
    assert!(
        embedded.contains("`{{config.queue.done_file}}`"),
        "Template should mention configured done file placeholder"
    );

    // Check for ordering semantics mention
    assert!(
        embedded.contains("file order is execution order"),
        "Template should mention that file order is execution order"
    );

    // Check for other key guidance elements
    assert!(
        embedded.contains("Preserve **all** tasks"),
        "Template should mention preserving all tasks"
    );
    assert!(
        embedded.contains("Do not renumber/rename task IDs"),
        "Template should mention not renumbering task IDs"
    );
    assert!(
        embedded.contains("terminal tasks (`done`/`rejected`)"),
        "Template should mention terminal tasks"
    );
}

#[test]
fn rendered_prompt_includes_queue_guidance_with_queue_conflicts() -> Result<()> {
    let template_meta = prompt_template(PromptTemplateId::MergeConflicts);
    let template = template_meta.embedded_default;

    let files = vec![".ralph/queue.jsonc".to_string(), "src/lib.rs".to_string()];
    let config = default_config();
    let rendered = render_merge_conflict_prompt(template, &files, &config)?;

    // Check that conflict files are listed
    assert!(
        rendered.contains("- .ralph/queue.jsonc"),
        "Rendered prompt should list queue.jsonc"
    );
    assert!(
        rendered.contains("- src/lib.rs"),
        "Rendered prompt should list src/lib.rs"
    );

    // Check that queue guidance is included
    assert!(
        rendered.contains("Special Guidance for `.ralph/queue.jsonc` / `.ralph/done.jsonc`"),
        "Rendered prompt should include queue guidance section"
    );
    assert!(
        rendered.contains("file order is execution order"),
        "Rendered prompt should mention ordering semantics"
    );

    // Check that placeholder was replaced
    assert!(
        !rendered.contains("{{CONFLICT_FILES}}"),
        "Rendered prompt should not contain unresolved placeholder"
    );

    Ok(())
}

#[test]
fn rendered_prompt_includes_queue_guidance_with_done_conflicts() -> Result<()> {
    let template_meta = prompt_template(PromptTemplateId::MergeConflicts);
    let template = template_meta.embedded_default;

    let files = vec![".ralph/done.jsonc".to_string(), "README.md".to_string()];
    let config = default_config();
    let rendered = render_merge_conflict_prompt(template, &files, &config)?;

    // Check that conflict files are listed
    assert!(
        rendered.contains("- .ralph/done.jsonc"),
        "Rendered prompt should list done.jsonc"
    );
    assert!(
        rendered.contains("- README.md"),
        "Rendered prompt should list README.md"
    );

    // Check that done-file guidance is included
    assert!(
        rendered.contains("terminal tasks (`done`/`rejected`)"),
        "Rendered prompt should mention terminal tasks for done archive"
    );

    // Check that placeholder was replaced
    assert!(
        !rendered.contains("{{CONFLICT_FILES}}"),
        "Rendered prompt should not contain unresolved placeholder"
    );

    Ok(())
}
