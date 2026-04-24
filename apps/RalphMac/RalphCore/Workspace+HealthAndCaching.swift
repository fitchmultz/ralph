//! Workspace+HealthAndCaching
//!
//! Purpose:
//! - Perform workspace-scoped CLI health checks.
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
//!
//! Usage:
//! - Used by the RalphMac app or RalphCore tests through its owning feature surface.
//! Invariants/assumptions callers must respect:
//! - Health checks are main-actor UI operations.
//! - Cached tasks are stored per workspace ID.
//! - Offline display prefers cache only when the CLI is unavailable.
//!
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
    @Published public var navigationPersistenceIssue: PersistenceIssue?
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
        let repositoryContext = currentRepositoryContext()
        guard !isShutDown, !Task.isCancelled else {
            return CLIHealthStatus(
                availability: .unknown,
                lastChecked: Date(),
                workspaceURL: identityState.workingDirectoryURL
            )
        }

        diagnosticsState.isCheckingHealth = true
        defer {
            if !isShutDown, isCurrentRepositoryContext(repositoryContext) {
                diagnosticsState.isCheckingHealth = false
            }
        }

        let status = await cliHealthChecker.checkHealth(
            workspaceID: id,
            workspaceURL: repositoryContext.workingDirectoryURL,
            timeout: timeout,
            executableURL: client?.executableURL
        )

        guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else {
            return status
        }

        diagnosticsState.cliHealthStatus = status

        if status.isAvailable {
            refreshCachedTasks()
        }

        refreshOperationalHealth()

        return status
    }

    @MainActor
    func checkHealthIfNeeded() async {
        if let status = diagnosticsState.cliHealthStatus,
            Date().timeIntervalSince(status.lastChecked) < 30 {
            return
        }

        _ = await checkHealth()
    }

    @MainActor
    func refreshCachedTasks() {
        diagnosticsState.cachedTasks = taskState.tasks

        do {
            let data = try JSONEncoder().encode(taskState.tasks)
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
            diagnosticsState.cachedTasks = []
            return
        }

        do {
            diagnosticsState.cachedTasks = try JSONDecoder().decode([RalphTask].self, from: data)
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
            diagnosticsState.cachedTasks = []
        }
        refreshOperationalHealth()
    }

    @MainActor
    func displayTasks() -> [RalphTask] {
        if diagnosticsState.cliHealthStatus?.isAvailable == false,
            !diagnosticsState.cachedTasks.isEmpty {
            return diagnosticsState.cachedTasks
        }
        return taskState.tasks
    }

    @MainActor
    func clearCachedTasks() {
        diagnosticsState.cachedTasks = []
        RalphAppDefaults.userDefaults.removeObject(forKey: defaultsKey("cachedTasks"))
        clearPersistenceIssue(domain: .cachedTasks)
        refreshOperationalHealth()
    }

    @MainActor
    func updateWatcherHealth(_ health: QueueWatcherHealth) {
        diagnosticsState.watcherHealth = health
        refreshOperationalHealth()
    }

    @MainActor
    func refreshOperationalHealth() {
        var issues: [WorkspaceOperationalIssue] = []

        if let cliIssue = diagnosticsState.cliHealthStatus.flatMap(WorkspaceOperationalIssue.fromCLIStatus) {
            issues.append(cliIssue)
        }

        if let persistenceIssue = diagnosticsState.persistenceIssue {
            issues.append(
                .fromPersistenceIssue(
                    persistenceIssue,
                    source: .workspacePersistence
                )
            )
        }

        if let navigationPersistenceIssue = diagnosticsState.navigationPersistenceIssue {
            issues.append(
                .fromPersistenceIssue(
                    navigationPersistenceIssue,
                    source: .workspacePersistence
                )
            )
        }

        if let watcherIssue = WorkspaceOperationalIssue.fromWatcherHealth(diagnosticsState.watcherHealth) {
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

        diagnosticsState.operationalIssues = issues
        diagnosticsState.operationalSummary = WorkspaceOperationalSummary(issues: issues)
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
