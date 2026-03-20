//! Purpose: task-builder prompt rendering coverage for prompt command previews.
//!
//! Responsibilities:
//! - Verify task-builder prompt rendering includes the request and hint fields unchanged.
//!
//! Scope:
//! - `ralph::commands::prompt::build_task_builder_prompt` behavior only.
//!
//! Usage:
//! - Run via the root `prompt_cmd_test` integration suite.
//!
//! Invariants/Assumptions:
//! - Assertions and prompt-fragment expectations remain unchanged from the original suite.
//! - Shared fixture setup continues to flow through `make_resolved` and `write_minimal_queue`.

use super::*;

#[test]
fn task_builder_prompt_includes_request_and_hints() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_task_builder_prompt(
        &resolved,
        TaskBuilderPromptOptions {
            request: "Add tests".to_string(),
            hint_tags: "rust,tests".to_string(),
            hint_scope: "crates/ralph".to_string(),
            repoprompt_tool_injection: false,
            explain: false,
        },
    )?;

    assert!(prompt.contains("Add tests"));
    assert!(prompt.contains("rust,tests"));
    assert!(prompt.contains("crates/ralph"));
    Ok(())
}
