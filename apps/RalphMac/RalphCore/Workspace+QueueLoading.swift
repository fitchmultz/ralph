//! Workspace+QueueLoading
//!
//! Responsibilities:
//! - Load queue tasks through the Ralph CLI.
//! - Parse queue snapshots directly from disk for watcher-triggered refreshes.
//! - Coordinate queue file watching and workspace-local refresh state.
//!
//! Does not handle:
//! - Task mutations or task creation.
//! - Task filtering or task presentation.
//! - Runner execution state beyond refreshing config after queue changes.
//!
//! Invariants/assumptions callers must respect:
//! - The workspace must point at a Ralph-initialized directory to load tasks.
//! - Direct file parsing is a fast path and may fall back to the CLI.
//! - Queue refresh events retain previous and current task snapshots for view-local reactions.

public import Foundation
public import Combine

@MainActor
public final class WorkspaceTaskState: ObservableObject {
    @Published public var tasks: [RalphTask] = []
    @Published public var tasksLoading = false
    @Published public var tasksErrorMessage: String?
    @Published public var lastQueueRefreshEvent: Workspace.QueueRefreshEvent?
    @Published public var taskFilterText = ""
    @Published public var taskStatusFilter: RalphTaskStatus?
    @Published public var taskPriorityFilter: RalphTaskPriority?
    @Published public var taskTagFilter: String?
    @Published public var taskSortBy: Workspace.TaskSortOption = .priority
    @Published public var taskSortAscending = false

    public init() {}
}

public extension Workspace {
    enum TaskSortOption: String, CaseIterable {
        case priority = "Priority"
        case created = "Created"
        case updated = "Updated"
        case status = "Status"
        case title = "Title"
    }

    struct QueueRefreshEvent: Identifiable, Sendable, Equatable {
        public enum Source: String, Sendable, Equatable {
            case externalFileChange
        }

        public let id: UUID
        public let source: Source
        public let previousTasks: [RalphTask]
        public let currentTasks: [RalphTask]
        public let highlightedTaskIDs: Set<String>

        public init(
            id: UUID = UUID(),
            source: Source,
            previousTasks: [RalphTask],
            currentTasks: [RalphTask]
        ) {
            let changes = TaskChanges.diff(previous: previousTasks, current: currentTasks)
            var highlightedTaskIDs = Set(changes.changed.map(\.id))
            highlightedTaskIDs.formUnion(changes.added.map(\.id))

            self.id = id
            self.source = source
            self.previousTasks = previousTasks
            self.currentTasks = currentTasks
            self.highlightedTaskIDs = highlightedTaskIDs
        }
    }
}

public extension Workspace {
    func loadTasks(retryConfiguration: RetryConfiguration = .default) async {
        guard let client else {
            tasksErrorMessage = "CLI client not available."
            return
        }

        guard hasRalphQueueFile else {
            stopFileWatching()
            tasks = []
            tasksErrorMessage = "No Ralph queue found in this directory. Run `ralph init --non-interactive` in \(workingDirectoryURL.path)."
            showErrorRecovery = false
            lastRecoveryError = nil
            return
        }

        startFileWatching()

        tasksLoading = true
        tasksErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "list", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.tasksErrorMessage = "Retrying load tasks (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            guard collected.status.code == 0 else {
                tasksErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load tasks (exit \(collected.status.code))."
                    : collected.stderr
                tasksLoading = false
                return
            }

            tasks = try await WorkspaceQueueSnapshotLoader.decodeQueueTasks(
                fromCLIOutput: collected.stdout
            )
            sanitizeRunControlSelection()
            tasksErrorMessage = nil
        } catch {
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadTasks",
                workspaceURL: workingDirectoryURL
            )
            tasksErrorMessage = recoveryError.message
            lastRecoveryError = recoveryError
            showErrorRecovery = true
        }

        tasksLoading = false
    }

    func stopFileWatching() {
        queueRuntime.stopWatching()
    }
}

extension Workspace {
    func startFileWatching() {
        queueRuntime.startWatchingIfNeeded()
    }
}
