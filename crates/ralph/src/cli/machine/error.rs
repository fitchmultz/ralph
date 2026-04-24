//! Machine-command error classification and JSON stderr emission.
//!
//! Purpose:
//! - Machine-command error classification and JSON stderr emission.
//!
//! Responsibilities:
//! - Convert machine command failures into stable, versioned error documents.
//! - Keep app-facing recovery/error codes centralized on the CLI side.
//! - Sanitize/redact error details before they reach stderr.
//!
//! Not handled here:
//! - Machine command routing or success-document emission.
//! - Human CLI error rendering.
//! - App-side recovery presentation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Machine command failures must emit JSON on stderr instead of prose.
//! - Unknown failures stay structured and redacted.
//! - Error codes remain stable unless the machine contract version changes.

use anyhow::Result;

use crate::contracts::{MACHINE_ERROR_VERSION, MachineErrorCode, MachineErrorDocument};

pub fn print_machine_error(err: &anyhow::Error) -> Result<()> {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&build_machine_error_document(err))?
    );
    Ok(())
}

fn build_machine_error_document(err: &anyhow::Error) -> MachineErrorDocument {
    let detail = sanitized_detail(err);
    let normalized = detail.to_ascii_lowercase();

    let (code, message, retryable) = if normalized.contains("task mutation conflict for") {
        (
            MachineErrorCode::TaskMutationConflict,
            "Task changed on disk before Ralph could apply the mutation.",
            false,
        )
    } else if normalized.contains("permission denied") {
        (
            MachineErrorCode::PermissionDenied,
            "Permission denied.",
            false,
        )
    } else if normalized.contains("queue file") && normalized.contains("no such file") {
        (
            MachineErrorCode::QueueCorrupted,
            "No Ralph queue file found.",
            false,
        )
    } else if normalized.contains("queue validation failed")
        || normalized.contains("done archive validation failed")
        || (normalized.contains("queue")
            && (normalized.contains("corrupt") || normalized.contains("invalid")))
        || normalized.contains("duplicate id")
        || normalized.contains("invalid timestamp")
    {
        (
            MachineErrorCode::QueueCorrupted,
            "Queue data appears corrupted.",
            false,
        )
    } else if normalized.contains("load project config")
        || normalized.contains("load global config")
        || normalized.contains("unsupported config version")
        || (normalized.contains("unknown field") && normalized.contains("config"))
    {
        (
            MachineErrorCode::ConfigIncompatible,
            "Workspace config is incompatible with this Ralph version.",
            false,
        )
    } else if normalized.contains("version")
        && (normalized.contains("minimum supported version")
            || normalized.contains("newer than supported")
            || normalized.contains("too old")
            || normalized.contains("too new"))
    {
        (
            MachineErrorCode::VersionMismatch,
            "Ralph CLI version is incompatible with this app.",
            false,
        )
    } else if normalized.contains("network")
        || normalized.contains("connection")
        || normalized.contains("timed out")
    {
        (
            MachineErrorCode::NetworkError,
            "Network operation failed.",
            false,
        )
    } else if normalized.contains("resource temporarily unavailable")
        || normalized.contains("resource busy")
        || normalized.contains("file locked")
        || normalized.contains("operation would block")
        || normalized.contains("device or resource busy")
        || normalized.contains("eagain")
        || normalized.contains("ewouldblock")
        || normalized.contains("ebusy")
    {
        (
            MachineErrorCode::ResourceBusy,
            "Resource temporarily unavailable.",
            true,
        )
    } else if normalized.contains("parse")
        || normalized.contains("decode")
        || normalized.contains("json")
    {
        (
            MachineErrorCode::ParseError,
            "Unable to parse CLI output.",
            false,
        )
    } else {
        (
            MachineErrorCode::Unknown,
            "Ralph CLI command failed.",
            false,
        )
    };

    let detail = if detail == message {
        None
    } else {
        Some(detail)
    };

    MachineErrorDocument {
        version: MACHINE_ERROR_VERSION,
        code,
        message: message.to_string(),
        detail,
        retryable,
    }
}

fn sanitized_detail(err: &anyhow::Error) -> String {
    let redacted = crate::redaction::redact_text(&format!("{:#}", err));
    let trimmed = redacted.trim();
    if trimmed.is_empty() {
        return "Ralph CLI command failed.".to_string();
    }

    let truncated: String = trimmed.chars().take(2_000).collect();
    if truncated.chars().count() == trimmed.chars().count() {
        truncated
    } else {
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_machine_error_document_classifies_queue_missing() {
        let err = anyhow::anyhow!(
            "read queue file /tmp/example/.ralph/queue.jsonc: No such file or directory (os error 2)"
        );

        let document = build_machine_error_document(&err);
        assert_eq!(document.code, MachineErrorCode::QueueCorrupted);
        assert_eq!(document.message, "No Ralph queue file found.");
        assert!(!document.retryable);
        assert!(
            document
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("queue.jsonc")
        );
    }

    #[test]
    fn build_machine_error_document_classifies_task_conflict() {
        let err = anyhow::anyhow!(
            "Task mutation conflict for RQ-0001: expected updated_at 2026-03-30T00:00:00Z, found 2026-03-30T00:01:00Z."
        );

        let document = build_machine_error_document(&err);
        assert_eq!(document.code, MachineErrorCode::TaskMutationConflict);
        assert_eq!(
            document.message,
            "Task changed on disk before Ralph could apply the mutation."
        );
        assert!(!document.retryable);
    }

    #[test]
    fn build_machine_error_document_sanitizes_unknown_failures() {
        let err = anyhow::anyhow!("unexpected bearer sk-test-123 failure");

        let document = build_machine_error_document(&err);
        assert_eq!(document.code, MachineErrorCode::Unknown);
        assert_eq!(document.message, "Ralph CLI command failed.");
        let detail = document
            .detail
            .expect("unknown failures keep sanitized detail");
        assert!(!detail.contains("sk-test-123"));
        assert!(detail.contains("[REDACTED]"));
    }
}
