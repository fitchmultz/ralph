//!
//! RalphCLIClient+Recovery
//!
//! Purpose:
//! - Define recovery-facing models and surface the canonical error-classification entry point.
//!
//! Responsibilities:
//! - Describe recovery categories, actions, guidance, and retry UI state.
//! - Produce rich recovery payloads for workspace and app error presentation.
//! - Delegate classification to `RalphCLIRecoveryClassifier` so every path shares one rule set.
//!
//! Scope:
//! - Recovery metadata only. Process spawning, retry loops, and health probes live elsewhere.
//!
//! Usage:
//! - Call `RecoveryError.classify` when surfacing failures to the app.
//!
//! Invariants/Assumptions:
//! - Classification is best-effort and falls back to `.unknown`.
//! - Suggested actions remain UI-safe and do not execute work on their own.

public import Foundation

/// Categories of errors for tailored recovery UI.
public enum ErrorCategory: String, CaseIterable, Sendable {
    case cliUnavailable
    case permissionDenied
    case parseError
    case networkError
    case queueCorrupted
    case resourceBusy
    case versionMismatch
    case unknown

    public var displayName: String {
        switch self {
        case .cliUnavailable: return "CLI Not Available"
        case .permissionDenied: return "Permission Denied"
        case .parseError: return "Data Parse Error"
        case .networkError: return "Network Error"
        case .queueCorrupted: return "Queue Corrupted"
        case .resourceBusy: return "Resource Busy"
        case .versionMismatch: return "Version Mismatch"
        case .unknown: return "Unknown Error"
        }
    }

    public var icon: String {
        switch self {
        case .cliUnavailable: return "terminal.fill"
        case .permissionDenied: return "lock.fill"
        case .parseError: return "doc.text.magnifyingglass"
        case .networkError: return "wifi.exclamationmark"
        case .queueCorrupted: return "exclamationmark.triangle.fill"
        case .resourceBusy: return "clock.badge.exclamationmark.fill"
        case .versionMismatch: return "number.circle.fill"
        case .unknown: return "questionmark.circle.fill"
        }
    }
}

/// Available recovery actions for error recovery UI.
public enum RecoveryAction: String, CaseIterable, Sendable {
    case retry
    case diagnose
    case copyErrorDetails
    case openLogs
    case dismiss
    case checkPermissions
    case reinstallCLI
    case validateQueue
}

extension ErrorCategory {
    public var suggestedActions: [RecoveryAction] {
        switch self {
        case .cliUnavailable:
            return [.retry, .checkPermissions, .reinstallCLI, .openLogs, .copyErrorDetails, .dismiss]
        case .permissionDenied:
            return [.retry, .checkPermissions, .openLogs, .copyErrorDetails, .dismiss]
        case .parseError:
            return [.retry, .validateQueue, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .queueCorrupted:
            return [.validateQueue, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .resourceBusy:
            return [.retry, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .networkError, .versionMismatch, .unknown:
            return [.retry, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        }
    }

    public var guidanceMessage: String? {
        switch self {
        case .cliUnavailable:
            return "The Ralph CLI could not be found or is not executable. This may indicate an incomplete installation."
        case .permissionDenied:
            return "Ralph doesn't have permission to access required files. Check that you have read/write access to the workspace directory."
        case .parseError:
            return "The CLI returned data that couldn't be parsed. The queue file may be corrupted or incompatible."
        case .networkError:
            return "A network operation failed. Check your connection and try again."
        case .queueCorrupted:
            return "The queue file appears to be corrupted. Try validating or restoring from backup."
        case .resourceBusy:
            return "A required resource is temporarily unavailable. This usually resolves on retry."
        case .versionMismatch:
            return "The Ralph CLI version is incompatible with this app. Please reinstall to ensure versions match."
        case .unknown:
            return "An unexpected error occurred. Check the logs for more details."
        }
    }
}

/// Rich error type with categorization and recovery context.
public struct RecoveryError: Error, Sendable {
    public let category: ErrorCategory
    public let message: String
    public let underlyingError: String?
    public let operation: String
    public let timestamp: Date
    public let suggestions: [String]
    public let workspaceURL: URL?

    public init(
        category: ErrorCategory,
        message: String,
        underlyingError: String? = nil,
        operation: String,
        suggestions: [String] = [],
        workspaceURL: URL? = nil
    ) {
        self.category = category
        self.message = message
        self.underlyingError = underlyingError
        self.operation = operation
        self.timestamp = Date()
        self.suggestions = suggestions
        self.workspaceURL = workspaceURL
    }

    public var fullErrorDetails: String {
        var lines: [String] = []
        lines.append("=== Ralph Error Report ===")
        lines.append("Timestamp: \(timestamp.formatted(.iso8601))")
        lines.append("Category: \(category.displayName)")
        lines.append("Operation: \(operation)")
        lines.append("Message: \(message)")
        if let underlyingError {
            lines.append("Details: \(underlyingError)")
        }
        if !suggestions.isEmpty {
            lines.append("")
            lines.append("Suggestions:")
            for suggestion in suggestions {
                lines.append("  - \(suggestion)")
            }
        }
        lines.append("==========================")
        return lines.joined(separator: "\n")
    }
}

public extension RecoveryError {
    static func classify(error: any Error, operation: String, workspaceURL: URL? = nil) -> RecoveryError {
        RalphCLIRecoveryClassifier.classify(
            error: error,
            operation: operation,
            workspaceURL: workspaceURL
        )
    }
}

/// Tracks the state of retry attempts for UI feedback.
public struct RetryState: Sendable {
    public let isRetrying: Bool
    public let attempt: Int
    public let maxAttempts: Int
    public let isExhausted: Bool

    public init(isRetrying: Bool, attempt: Int, maxAttempts: Int) {
        self.isRetrying = isRetrying
        self.attempt = attempt
        self.maxAttempts = maxAttempts
        self.isExhausted = attempt >= maxAttempts && !isRetrying
    }

    public var canRetryManually: Bool {
        isExhausted && !isRetrying
    }
}
