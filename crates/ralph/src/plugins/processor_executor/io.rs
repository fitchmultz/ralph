//! Processor-hook temp-file IO helpers.
//!
//! Purpose:
//! - Processor-hook temp-file IO helpers.
//!
//! Responsibilities:
//! - Materialize task/prompt/stdout payloads into temp files.
//! - Read back modified payloads from hook temp files.
//!
//! Not handled here:
//! - Hook dispatch or subprocess execution.
//! - Plugin discovery and enable policy.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Hook payload files remain UTF-8 text.
//! - Temp payloads live long enough for the full hook chain invocation.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use tempfile::TempPath;

use crate::contracts::Task;

use super::ProcessorExecutor;

impl ProcessorExecutor<'_> {
    pub(super) fn write_task_payload(&self, task: &Task) -> Result<TempPath> {
        let task_json =
            serde_json::to_string_pretty(task).context("serialize task for validate_task hook")?;
        self.write_text_payload("plugin", &task_json, "validate_task")
    }

    pub(super) fn write_text_payload(
        &self,
        label: &str,
        content: &str,
        context_label: &str,
    ) -> Result<TempPath> {
        let mut temp_file = crate::fsutil::create_ralph_temp_file(label)
            .with_context(|| format!("create temp file for {context_label}"))?;
        temp_file
            .write_all(content.as_bytes())
            .with_context(|| format!("write {context_label} payload to temp file"))?;
        Ok(temp_file.into_temp_path())
    }

    pub(super) fn read_text_payload(&self, path: &Path, context_label: &str) -> Result<String> {
        std::fs::read_to_string(path)
            .with_context(|| format!("read {context_label} from temp file"))
    }
}
