/**
 RalphCLIClient+Recovery

 Responsibilities:
 - Classify CLI/app failures into user-facing recovery categories.
 - Define recovery actions, guidance, and retry UI state.
 - Produce rich recovery payloads for workspace error presentation.

 Does not handle:
 - Process spawning or IO streaming.
 - CLI health probing.
 - Retry execution loops.

 Invariants/assumptions callers must respect:
 - Recovery classification is best-effort and falls back to `.unknown`.
 - Suggested actions remain UI-safe and do not execute work on their own.
 */

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
        case .queueCorrupted:
            return "The queue file appears to be corrupted. Try validating or restoring from backup."
        case .resourceBusy:
            return "A required resource is temporarily unavailable. This usually resolves on retry."
        case .networkError:
            return "A network operation failed. Check your connection and try again."
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
        if let underlying = underlyingError {
            lines.append("Details: \(underlying)")
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
        if let retryable = error as? RetryableError {
            switch retryable {
            case .fileLocked, .resourceBusy, .resourceTemporarilyUnavailable:
                return RecoveryError(
                    category: .resourceBusy,
                    message: "Resource temporarily unavailable",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Wait a moment and retry",
                        "Check if another process is using Ralph",
                        "Close other Ralph windows that may be using the same workspace"
                    ],
                    workspaceURL: workspaceURL
                )
            case .ioTimeout:
                return RecoveryError(
                    category: .networkError,
                    message: "Operation timed out",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Try the operation again",
                        "Check system load and available resources",
                        "If this persists, inspect logs for blocked operations"
                    ],
                    workspaceURL: workspaceURL
                )
            case .underlying(let underlying):
                return classify(error: underlying, operation: operation, workspaceURL: workspaceURL)
            default:
                break
            }
        }

        if let cliError = error as? RalphCLIClientError {
            switch cliError {
            case .executableNotFound:
                return RecoveryError(
                    category: .cliUnavailable,
                    message: "Ralph CLI executable not found",
                    underlyingError: cliError.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Reinstall Ralph",
                        "Verify the app bundle contains the CLI",
                        "Check file permissions"
                    ],
                    workspaceURL: workspaceURL
                )
            case .executableNotExecutable:
                return RecoveryError(
                    category: .cliUnavailable,
                    message: "Ralph CLI is not executable",
                    underlyingError: cliError.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Reinstall Ralph",
                        "Check execute permissions on the CLI binary"
                    ],
                    workspaceURL: workspaceURL
                )
            }
        }

        let description = error.localizedDescription.lowercased()
        if description.contains("permission denied") {
            return RecoveryError(
                category: .permissionDenied,
                message: "Permission denied",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Check workspace directory permissions",
                    "Ensure Ralph can access the selected folder"
                ],
                workspaceURL: workspaceURL
            )
        }

        if description.contains("parse") || description.contains("decode") || description.contains("json") {
            return RecoveryError(
                category: .parseError,
                message: "Unable to parse CLI output",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Validate the queue file",
                    "Check whether the CLI and app versions match"
                ],
                workspaceURL: workspaceURL
            )
        }

        if description.contains("queue") && (description.contains("corrupt") || description.contains("invalid")) {
            return RecoveryError(
                category: .queueCorrupted,
                message: "Queue data appears corrupted",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Run queue validation",
                    "Inspect recent manual edits to queue files"
                ],
                workspaceURL: workspaceURL
            )
        }

        return RecoveryError(
            category: .unknown,
            message: error.localizedDescription,
            underlyingError: nil,
            operation: operation,
            suggestions: [
                "Check the logs for more details",
                "Try the operation again",
                "If the problem persists, consider reporting the issue"
            ],
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
