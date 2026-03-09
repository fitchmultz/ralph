/**
 WorkspaceOperationalHealth

 Responsibilities:
 - Define the shared operational-health model used across workspace runtime, diagnostics, and UI.
 - Normalize watcher, CLI, persistence, and crash-reporting failures into one issue format.
 - Provide summary helpers for rendering workspace health consistently.

 Does not handle:
 - Executing health checks or watcher retries.
 - SwiftUI presentation details.
 - Persisting operational-health state.

 Invariants/assumptions callers must respect:
 - Operational issues are append-only snapshots and should be replaced wholesale on recompute.
 - Higher severity values always represent more urgent operator action.
 - Watcher health snapshots describe the latest runtime state for one workspace watcher.
 */

public import Foundation

public enum OperationalIssueSeverity: Int, Comparable, Sendable, CaseIterable {
    case info = 0
    case warning = 1
    case error = 2

    public static func < (lhs: OperationalIssueSeverity, rhs: OperationalIssueSeverity) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    public var statusText: String {
        switch self {
        case .info:
            return "Info"
        case .warning:
            return "Warning"
        case .error:
            return "Error"
        }
    }
}

public struct WorkspaceOperationalIssue: Identifiable, Equatable, Sendable {
    public enum Source: String, Sendable {
        case cli
        case watcher
        case workspacePersistence
        case appPersistence
        case crashReporting
    }

    public let id: String
    public let source: Source
    public let severity: OperationalIssueSeverity
    public let title: String
    public let message: String
    public let recoverySuggestion: String?
    public let timestamp: Date

    public init(
        id: String,
        source: Source,
        severity: OperationalIssueSeverity,
        title: String,
        message: String,
        recoverySuggestion: String? = nil,
        timestamp: Date = Date()
    ) {
        self.id = id
        self.source = source
        self.severity = severity
        self.title = title
        self.message = message
        self.recoverySuggestion = recoverySuggestion
        self.timestamp = timestamp
    }
}

public struct WorkspaceOperationalSummary: Equatable, Sendable {
    public let severity: OperationalIssueSeverity?
    public let title: String
    public let subtitle: String?
    public let primaryIssue: WorkspaceOperationalIssue?

    public init(
        severity: OperationalIssueSeverity?,
        title: String,
        subtitle: String?,
        primaryIssue: WorkspaceOperationalIssue?
    ) {
        self.severity = severity
        self.title = title
        self.subtitle = subtitle
        self.primaryIssue = primaryIssue
    }

    public static let healthy = WorkspaceOperationalSummary(
        severity: nil,
        title: "Operational",
        subtitle: "All monitored workspace systems are healthy.",
        primaryIssue: nil
    )

    public var isHealthy: Bool {
        primaryIssue == nil
    }
}

public struct QueueWatcherHealth: Equatable, Sendable {
    public enum State: Equatable, Sendable {
        case idle
        case starting(attempt: Int)
        case watching
        case degraded(reason: String, retryCount: Int, nextRetryAt: Date?)
        case failed(reason: String, attempts: Int)
        case stopped
    }

    public let state: State
    public let workingDirectoryURL: URL
    public let timestamp: Date

    public init(
        state: State,
        workingDirectoryURL: URL,
        timestamp: Date = Date()
    ) {
        self.state = state
        self.workingDirectoryURL = workingDirectoryURL
        self.timestamp = timestamp
    }

    public var isWatching: Bool {
        if case .watching = state {
            return true
        }
        return false
    }
}

public extension QueueWatcherHealth {
    static func idle(for workingDirectoryURL: URL) -> QueueWatcherHealth {
        QueueWatcherHealth(state: .idle, workingDirectoryURL: workingDirectoryURL)
    }

    static func stopped(for workingDirectoryURL: URL) -> QueueWatcherHealth {
        QueueWatcherHealth(state: .stopped, workingDirectoryURL: workingDirectoryURL)
    }
}

public extension WorkspaceOperationalIssue {
    static func fromCLIStatus(_ status: CLIHealthStatus) -> WorkspaceOperationalIssue? {
        guard case .unavailable(let reason) = status.availability else {
            return nil
        }

        let details: (String, String, String?) = switch reason {
        case .cliNotFound:
            (
                "Ralph CLI missing",
                "The workspace cannot reach the bundled `ralph` executable.",
                "Reinstall Ralph or restore the bundled CLI binary."
            )
        case .cliNotExecutable:
            (
                "Ralph CLI not executable",
                "The workspace found `ralph`, but macOS cannot execute it.",
                "Check quarantine and file permissions, then retry."
            )
        case .workspaceInaccessible:
            (
                "Workspace unavailable",
                "The workspace directory is no longer accessible.",
                "Reconnect the workspace location or choose a different directory."
            )
        case .timeout:
            (
                "CLI health check timed out",
                "The workspace health probe did not complete before the timeout.",
                "Retry the health check after the system load settles."
            )
        case .permissionDenied:
            (
                "Workspace permission denied",
                "Ralph cannot read the workspace directory.",
                "Review filesystem permissions for this workspace."
            )
        case .unknown(let description):
            (
                "CLI health check failed",
                description,
                "Inspect workspace diagnostics and recent logs for more detail."
            )
        }

        let severity: OperationalIssueSeverity = switch reason {
        case .timeout:
            .warning
        default:
            .error
        }

        return WorkspaceOperationalIssue(
            id: "cli.\(status.workspaceURL.path)",
            source: .cli,
            severity: severity,
            title: details.0,
            message: details.1,
            recoverySuggestion: details.2,
            timestamp: status.lastChecked
        )
    }

    static func fromPersistenceIssue(
        _ issue: PersistenceIssue,
        source: Source
    ) -> WorkspaceOperationalIssue {
        WorkspaceOperationalIssue(
            id: "\(source.rawValue).\(issue.domain.rawValue).\(issue.operation.rawValue).\(issue.context)",
            source: source,
            severity: .error,
            title: issue.domain.displayTitle,
            message: issue.message,
            recoverySuggestion: issue.domain.recoverySuggestion,
            timestamp: issue.timestamp
        )
    }

    static func fromWatcherHealth(_ health: QueueWatcherHealth) -> WorkspaceOperationalIssue? {
        switch health.state {
        case .idle, .watching, .stopped:
            return nil
        case .starting(let attempt):
            return WorkspaceOperationalIssue(
                id: "watcher.starting.\(health.workingDirectoryURL.path)",
                source: .watcher,
                severity: .info,
                title: "Queue watcher starting",
                message: "The workspace is initializing queue-file observation (attempt \(attempt)).",
                recoverySuggestion: "Wait for the watcher to finish starting.",
                timestamp: health.timestamp
            )
        case .degraded(let reason, let retryCount, let nextRetryAt):
            let retryMessage: String
            if let nextRetryAt {
                retryMessage = " Next retry at \(nextRetryAt.formatted(date: .omitted, time: .standard))."
            } else {
                retryMessage = ""
            }

            return WorkspaceOperationalIssue(
                id: "watcher.degraded.\(health.workingDirectoryURL.path)",
                source: .watcher,
                severity: .warning,
                title: "Queue watcher degraded",
                message: "Queue watching hit \(retryCount) retry \(retryCount == 1 ? "attempt" : "attempts"): \(reason).\(retryMessage)",
                recoverySuggestion: "Keep the workspace open while the watcher retries, or manually refresh if changes seem stale.",
                timestamp: health.timestamp
            )
        case .failed(let reason, let attempts):
            return WorkspaceOperationalIssue(
                id: "watcher.failed.\(health.workingDirectoryURL.path)",
                source: .watcher,
                severity: .error,
                title: "Queue watcher failed",
                message: "Queue watching exhausted \(attempts) start attempts: \(reason). Workspace data may now drift stale until the watcher restarts.",
                recoverySuggestion: "Restart queue watching or reload the workspace to restore live queue updates.",
                timestamp: health.timestamp
            )
        }
    }
}

public extension WorkspaceOperationalSummary {
    init(issues: [WorkspaceOperationalIssue]) {
        let sortedIssues = issues.sorted {
            if $0.severity != $1.severity {
                return $0.severity > $1.severity
            }
            return $0.timestamp > $1.timestamp
        }

        guard let primaryIssue = sortedIssues.first else {
            self = .healthy
            return
        }

        self.init(
            severity: primaryIssue.severity,
            title: primaryIssue.title,
            subtitle: primaryIssue.message,
            primaryIssue: primaryIssue
        )
    }
}

private extension PersistenceIssue.Domain {
    var displayTitle: String {
        switch self {
        case .workspaceState:
            return "Workspace persistence failed"
        case .cachedTasks:
            return "Cached task persistence failed"
        case .navigationState:
            return "Navigation persistence failed"
        case .temporaryFiles:
            return "Temporary file cleanup failed"
        case .windowRestoration:
            return "Window restoration failed"
        case .versionCache:
            return "Version cache failed"
        case .appDefaultsPreparation:
            return "App defaults preparation failed"
        case .crashReporting:
            return "Crash reporting failed"
        }
    }

    var recoverySuggestion: String? {
        switch self {
        case .workspaceState, .cachedTasks, .navigationState, .temporaryFiles:
            return "Retry the action after checking local app-storage permissions."
        case .windowRestoration, .appDefaultsPreparation:
            return "Restart the app if window or defaults state remains inconsistent."
        case .versionCache:
            return "Retry the version check; Ralph will regenerate the cache on success."
        case .crashReporting:
            return "Inspect recent logs and the app support directory for crash-reporting failures."
        }
    }
}
