//! Purpose: scan prompt rendering coverage for prompt command previews.
//!
//! Responsibilities:
//! - Verify scan prompt focus substitution.
//! - Verify RepoPrompt tooling wrapper text is preserved when requested.
//!
//! Scope:
//! - `ralph::commands::prompt::build_scan_prompt` behavior only.
//!
//! Usage:
//! - Run via the root `prompt_cmd_test` integration suite.
//!
//! Invariants/Assumptions:
//! - Assertions and prompt-fragment expectations remain unchanged from the original suite.
//! - Shared fixture setup continues to flow through `make_resolved` and `write_minimal_queue`.

use super::*;

#[test]
fn scan_prompt_replaces_focus_and_can_wrap_rp() -> Result<()> {
    let temp = TempDir::new()?;
    write_minimal_queue(&temp)?;
    let resolved = make_resolved(&temp);

    let prompt = prompt_cmd::build_scan_prompt(
        &resolved,
        ScanPromptOptions {
            focus: "CI gaps".to_string(),
            mode: ScanMode::Maintenance,
            repoprompt_tool_injection: true,
            explain: false,
        },
    )?;

    assert!(prompt.contains("CI gaps"));
    assert!(prompt.contains("TOOLING REQUIREMENT: RepoPrompt"));
    assert!(prompt.contains("Targeting: use `list_windows` + `select_window`"));
    assert!(prompt.contains("_tabID"));
    Ok(())
}
