//! AGENTS.md validation helpers.
//!
//! Purpose:
//! - AGENTS.md validation helpers.
//!
//! Responsibilities:
//! - Validate required and recommended sections for AGENTS.md content.
//! - Report missing sections and placeholder outdated-section state.
//!
//! Not handled here:
//! - File loading or update merges.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::markdown::extract_section_titles;
use super::types::{ContextValidateOptions, ValidateReport};
use crate::constants::agents_md::{RECOMMENDED_SECTIONS, REQUIRED_SECTIONS};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;

pub(super) fn run_context_validate_impl(opts: ContextValidateOptions) -> Result<ValidateReport> {
    if !opts.path.exists() {
        return Ok(ValidateReport {
            valid: false,
            missing_sections: REQUIRED_SECTIONS.iter().map(|s| s.to_string()).collect(),
            outdated_sections: Vec::new(),
        });
    }

    let content = fs::read_to_string(&opts.path).context("read AGENTS.md")?;
    let sections = extract_section_titles(&content);
    let section_set: HashSet<_> = sections.iter().map(|s| s.as_str()).collect();

    let missing_sections: Vec<String> = REQUIRED_SECTIONS
        .iter()
        .filter(|s| !section_set.contains(**s))
        .map(|s| s.to_string())
        .collect();

    let missing_recommended: Vec<String> = if opts.strict {
        RECOMMENDED_SECTIONS
            .iter()
            .filter(|s| !section_set.contains(**s))
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let outdated_sections = Vec::new();
    let valid = missing_sections.is_empty() && (missing_recommended.is_empty() || !opts.strict);

    Ok(ValidateReport {
        valid,
        missing_sections: if opts.strict {
            missing_recommended
        } else {
            missing_sections
        },
        outdated_sections,
    })
}
