//! Prompt template source helpers.
//!
//! Purpose:
//! - Prompt template source helpers.
//!
//! Responsibilities:
//! - Describe whether a preview uses an embedded template or a repo override.
//! - Keep explain-header source selection separate from prompt assembly logic.
//!
//! Not handled here:
//! - Template file reading or diffing.
//! - Prompt rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Override paths stay aligned with `crate::constants::paths`.

use std::path::Path;

use crate::constants::paths::{
    SCAN_OVERRIDE_PATH, TASK_BUILDER_OVERRIDE_PATH, WORKER_OVERRIDE_PATH,
};

pub(super) fn worker_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(WORKER_OVERRIDE_PATH).exists() {
        WORKER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

pub(super) fn scan_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(SCAN_OVERRIDE_PATH).exists() {
        SCAN_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}

pub(super) fn task_builder_template_source(repo_root: &Path) -> &'static str {
    if repo_root.join(TASK_BUILDER_OVERRIDE_PATH).exists() {
        TASK_BUILDER_OVERRIDE_PATH
    } else {
        "(embedded default)"
    }
}
