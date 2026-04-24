//! Import reporting types.
//!
//! Purpose:
//! - Import reporting types.
//!
//! Responsibilities:
//! - Capture parsed/imported/skipped/renamed counts for queue import.
//! - Render concise operator-facing summaries after dry-runs or writes.
//!
//! Not handled here:
//! - Parsing or normalization.
//! - Duplicate-policy execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Rename mappings are already finalized when the report is rendered.
//! - Summary output remains compact even for large rename sets.

#[derive(Debug, Default, PartialEq, Eq)]
/// Summary of an import operation for logging.
pub(super) struct ImportReport {
    pub(super) parsed: usize,
    pub(super) imported: usize,
    pub(super) skipped_duplicates: usize,
    pub(super) renamed: usize,
    pub(super) rename_mappings: Vec<(String, String)>,
}

impl ImportReport {
    pub(super) fn summary(&self) -> String {
        let mut parts = vec![format!("parsed {} task(s)", self.parsed)];
        if self.imported > 0 {
            parts.push(format!("imported {}", self.imported));
        }
        if self.skipped_duplicates > 0 {
            parts.push(format!("skipped {} duplicate(s)", self.skipped_duplicates));
        }
        if self.renamed > 0 {
            parts.push(format!("renamed {} task(s)", self.renamed));
            let show_count = self.rename_mappings.len().min(50);
            for (old, new) in &self.rename_mappings[..show_count] {
                parts.push(format!("  {} -> {}", old, new));
            }
            if self.rename_mappings.len() > 50 {
                parts.push(format!(
                    "  ... and {} more",
                    self.rename_mappings.len() - 50
                ));
            }
        }
        parts.join("; ")
    }
}
