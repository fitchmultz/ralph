//!
//! RalphCLIRecoveryClassifier
//!
//! Purpose:
//! - Provide the canonical CLI/app error-classification path used by recovery UI.
//!
//! Responsibilities:
//! - Normalize typed errors and free-form stderr/localized descriptions into one recovery decision tree.
//! - Keep phrase-based classification rules in one place so generic and process-failure paths cannot drift.
//!
//! Scope:
//! - Recovery classification only. It does not execute retries, health probes, or UI presentation.
//!
//! Usage:
//! - `RecoveryError.classify` delegates here for every recovery payload.
//!
//! Invariants/Assumptions:
//! - Phrase matching is best-effort and falls back to `.unknown`.
//! - Process failures with stderr and generic localized descriptions must share the same phrase rules.

import Foundation

enum RalphCLIRecoveryClassifier {
    static func classify(
        error: any Error,
        operation: String,
        workspaceURL: URL?
    ) -> RecoveryError {
        if let retryable = error as? RetryableError {
            switch retryable {
            case .fileLocked, .resourceBusy, .resourceTemporarilyUnavailable:
                return makeRecovery(
                    category: .resourceBusy,
                    message: "Resource temporarily unavailable",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    workspaceURL: workspaceURL
                )
            case .ioTimeout:
                return makeRecovery(
                    category: .networkError,
                    message: "Operation timed out",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    workspaceURL: workspaceURL
                )
            case .processError(let exitCode, let stderr):
                let trimmed = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
                if trimmed.isEmpty {
                    return makeRecovery(
                        category: .unknown,
                        message: "CLI command failed with exit code \(exitCode)",
                        underlyingError: nil,
                        operation: operation,
                        workspaceURL: workspaceURL
                    )
                }
                return classifyDescription(
                    trimmed,
                    operation: operation,
                    workspaceURL: workspaceURL
                )
            case .underlying(let underlying):
                return classify(error: underlying, operation: operation, workspaceURL: workspaceURL)
            }
        }

        if let cliError = error as? RalphCLIClientError {
            switch cliError {
            case .executableNotFound:
                return makeRecovery(
                    category: .cliUnavailable,
                    message: "Ralph CLI executable not found",
                    underlyingError: cliError.localizedDescription,
                    operation: operation,
                    workspaceURL: workspaceURL
                )
            case .executableNotExecutable:
                return makeRecovery(
                    category: .cliUnavailable,
                    message: "Ralph CLI is not executable",
                    underlyingError: cliError.localizedDescription,
                    operation: operation,
                    workspaceURL: workspaceURL
                )
            }
        }

        if error is DecodingError {
            return makeRecovery(
                category: .parseError,
                message: "Unable to parse CLI output",
                underlyingError: error.localizedDescription,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        let nsError = error as NSError
        if nsError.domain == NSURLErrorDomain {
            return makeRecovery(
                category: .networkError,
                message: "Network operation failed",
                underlyingError: error.localizedDescription,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        return classifyDescription(
            error.localizedDescription,
            operation: operation,
            workspaceURL: workspaceURL
        )
    }

    private static func classifyDescription(
        _ description: String,
        operation: String,
        workspaceURL: URL?
    ) -> RecoveryError {
        let normalized = description.lowercased()

        if normalized.contains("permission denied") {
            return makeRecovery(
                category: .permissionDenied,
                message: "Permission denied",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("queue file") && normalized.contains("no such file") {
            return makeRecovery(
                category: .queueCorrupted,
                message: "No Ralph queue file found",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("queue") && (normalized.contains("corrupt") || normalized.contains("invalid")) {
            return makeRecovery(
                category: .queueCorrupted,
                message: "Queue data appears corrupted",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("load project config")
            || normalized.contains("load global config")
            || normalized.contains("unsupported config version")
            || (normalized.contains("unknown field") && normalized.contains("config"))
        {
            return makeRecovery(
                category: .configIncompatible,
                message: "Workspace config is incompatible with this Ralph version",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("version")
            && (
                normalized.contains("minimum supported version")
                    || normalized.contains("newer than supported")
                    || normalized.contains("too old")
                    || normalized.contains("too new")
            )
        {
            return makeRecovery(
                category: .versionMismatch,
                message: "Ralph CLI version is incompatible with this app",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("network")
            || normalized.contains("connection")
            || normalized.contains("timed out")
        {
            return makeRecovery(
                category: .networkError,
                message: "Network operation failed",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("resource temporarily unavailable")
            || normalized.contains("resource busy")
            || normalized.contains("file locked")
        {
            return makeRecovery(
                category: .resourceBusy,
                message: "Resource temporarily unavailable",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        if normalized.contains("parse") || normalized.contains("decode") || normalized.contains("json") {
            return makeRecovery(
                category: .parseError,
                message: "Unable to parse CLI output",
                underlyingError: description,
                operation: operation,
                workspaceURL: workspaceURL
            )
        }

        return makeRecovery(
            category: .unknown,
            message: description,
            underlyingError: nil,
            operation: operation,
            workspaceURL: workspaceURL
        )
    }

    private static func makeRecovery(
        category: ErrorCategory,
        message: String,
        underlyingError: String?,
        operation: String,
        workspaceURL: URL?
    ) -> RecoveryError {
        RecoveryError(
            category: category,
            message: message,
            underlyingError: underlyingError,
            operation: operation,
            suggestions: suggestions(for: category),
            workspaceURL: workspaceURL
        )
    }

    private static func suggestions(for category: ErrorCategory) -> [String] {
        switch category {
        case .cliUnavailable:
            return [
                "Ensure Ralph is installed correctly",
                "Reinstall Ralph",
                "Verify the app bundle contains the CLI",
                "Check file permissions",
            ]
        case .permissionDenied:
            return [
                "Check workspace directory permissions",
                "Ensure Ralph can access the selected folder",
            ]
        case .configIncompatible:
            return [
                "Run `ralph migrate --apply` in the repository",
                "Retry after the migration completes",
            ]
        case .parseError:
            return [
                "Validate the queue file",
                "Check whether the CLI and app versions match",
            ]
        case .networkError:
            return [
                "Check your network connection",
                "Try the operation again",
                "If this persists, inspect logs for blocked operations",
            ]
        case .queueCorrupted:
            return [
                "Run `ralph init --non-interactive` to create or repair queue files",
                "Inspect recent manual edits to queue files",
                "Restore the queue from backup if needed",
            ]
        case .resourceBusy:
            return [
                "Wait a moment and retry",
                "Check if another process is using Ralph",
                "Close other Ralph windows that may be using the same workspace",
            ]
        case .versionMismatch:
            return [
                "Reinstall Ralph",
                "Verify the bundled CLI version matches the app",
            ]
        case .unknown:
            return [
                "Check the logs for more details",
                "Try the operation again",
                "If the problem persists, consider reporting the issue",
            ]
        }
    }
}
