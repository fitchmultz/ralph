//! Merge conflict prompt tests.
//!
//! Responsibilities: validate merge conflict prompt rendering and placeholder replacement.
//! Not handled: prompt loading via overrides or merge runner execution.
//! Invariants/assumptions: conflict file list must be non-empty.

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
