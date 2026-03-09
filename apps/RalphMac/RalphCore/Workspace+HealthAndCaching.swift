//! Workspace+HealthAndCaching
//!
//! Responsibilities:
//! - Perform workspace-scoped CLI health checks.
//! - Persist and load cached tasks for offline viewing.
//! - Provide a display-task accessor that prefers cache when offline.
//!
//! Does not handle:
//! - Queue loading itself.
//! - Task mutations.
//! - Graph or analytics loading.
//!
//! Invariants/assumptions callers must respect:
//! - Health checks are main-actor UI operations.
//! - Cached tasks are stored per workspace ID.
//! - Offline display prefers cache only when the CLI is unavailable.

public import Foundation
public import Combine

@MainActor
public final class WorkspaceDiagnosticsState: ObservableObject {
    @Published public var lastRecoveryError: RecoveryError?
    @Published public var showErrorRecovery = false
    @Published public var retryState: RetryState?
    @Published public var cliHealthStatus: CLIHealthStatus?
    @Published public var isCheckingHealth = false
    @Published public var cachedTasks: [RalphTask] = []
    @Published public var persistenceIssue: PersistenceIssue?
    @Published public var watcherHealth: QueueWatcherHealth
    @Published public var operationalIssues: [WorkspaceOperationalIssue] = []
    @Published public var operationalSummary: WorkspaceOperationalSummary = .healthy

    public init(
        watcherHealth: QueueWatcherHealth = .idle(
            for: FileManager.default.homeDirectoryForCurrentUser
        )
    ) {
        self.watcherHealth = watcherHealth
    }
}

public extension Workspace {
    @MainActor
    func checkHealth(timeout: TimeInterval = CLIHealthChecker.defaultTimeout) async -> CLIHealthStatus {
        isCheckingHealth = true
        defer { isCheckingHealth = false }

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: id,
            workspaceURL: workingDirectoryURL,
            timeout: timeout,
            executableURL: client?.executableURL
        )
        cliHealthStatus = status

        if status.isAvailable {
            refreshCachedTasks()
        }

        refreshOperationalHealth()

        return status
    }

    @MainActor
    func checkHealthIfNeeded() async {
        if let status = cliHealthStatus,
            Date().timeIntervalSince(status.lastChecked) < 30 {
            return
        }

        _ = await checkHealth()
    }

    @MainActor
    func refreshCachedTasks() {
        cachedTasks = tasks

        do {
            let data = try JSONEncoder().encode(tasks)
            RalphAppDefaults.userDefaults.set(data, forKey: defaultsKey("cachedTasks"))
            clearPersistenceIssue(domain: .cachedTasks)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .cachedTasks,
                    operation: .save,
                    context: defaultsKey("cachedTasks"),
                    error: error
                )
            )
        }
        refreshOperationalHealth()
    }

    @MainActor
    func loadCachedTasks() {
        guard let data = RalphAppDefaults.userDefaults.data(forKey: defaultsKey("cachedTasks")) else {
            cachedTasks = []
            return
        }

        do {
            cachedTasks = try JSONDecoder().decode([RalphTask].self, from: data)
            clearPersistenceIssue(domain: .cachedTasks)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .cachedTasks,
                    operation: .load,
                    context: defaultsKey("cachedTasks"),
                    error: error
                )
            )
            cachedTasks = []
        }
        refreshOperationalHealth()
    }

    @MainActor
    func displayTasks() -> [RalphTask] {
        if showOfflineBanner && !cachedTasks.isEmpty {
            return cachedTasks
        }
        return tasks
    }

    @MainActor
    func clearCachedTasks() {
        cachedTasks = []
        RalphAppDefaults.userDefaults.removeObject(forKey: defaultsKey("cachedTasks"))
        clearPersistenceIssue(domain: .cachedTasks)
        refreshOperationalHealth()
    }

    @MainActor
    func updateWatcherHealth(_ health: QueueWatcherHealth) {
        watcherHealth = health
        refreshOperationalHealth()
    }

    @MainActor
    func refreshOperationalHealth() {
        var issues: [WorkspaceOperationalIssue] = []

        if let cliIssue = cliHealthStatus.flatMap(WorkspaceOperationalIssue.fromCLIStatus) {
            issues.append(cliIssue)
        }

        if let persistenceIssue {
            issues.append(
                .fromPersistenceIssue(
                    persistenceIssue,
                    source: .workspacePersistence
                )
            )
        }

        if let watcherIssue = WorkspaceOperationalIssue.fromWatcherHealth(watcherHealth) {
            issues.append(watcherIssue)
        }

        if let managerIssue = WorkspaceManager.shared.persistenceIssue {
            issues.append(
                .fromPersistenceIssue(
                    managerIssue,
                    source: .appPersistence
                )
            )
        }

        issues.append(
            contentsOf: CrashReporter.shared.operationalIssues.map {
                .fromPersistenceIssue($0, source: .crashReporting)
            }
        )

        issues.sort {
            if $0.severity != $1.severity {
                return $0.severity > $1.severity
            }
            return $0.timestamp > $1.timestamp
        }

        operationalIssues = issues
        operationalSummary = WorkspaceOperationalSummary(issues: issues)
    }

    @MainActor
    func repairOperationalHealth() async {
        queueRuntime.repairWatching()
        let status = await checkHealth()
        if status.isAvailable {
            await loadTasks()
        } else {
            loadCachedTasks()
        }
    }
}
