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
        } catch {
            RalphLogger.shared.error("Failed to cache tasks: \(error)", category: .workspace)
        }
    }

    @MainActor
    func loadCachedTasks() {
        guard let data = RalphAppDefaults.userDefaults.data(forKey: defaultsKey("cachedTasks")) else {
            cachedTasks = []
            return
        }

        do {
            cachedTasks = try JSONDecoder().decode([RalphTask].self, from: data)
        } catch {
            RalphLogger.shared.error("Failed to load cached tasks: \(error)", category: .workspace)
            cachedTasks = []
        }
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
    }
}
