//! Workspace+QueueLoading
//!
//! Responsibilities:
//! - Load queue tasks through the Ralph machine contract.
//! - Coordinate queue file watching and workspace-local refresh state.
//!
//! Does not handle:
//! - Task mutations or task creation.
//! - Task filtering or task presentation.
//! - Runner execution state beyond refreshing config after queue changes.
//!
//! Invariants/assumptions callers must respect:
//! - The workspace must point at a Ralph-initialized directory to load tasks.
//! - Queue snapshots are always sourced from `ralph machine queue read`.
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
        let repositoryContext = currentRepositoryContext()
        guard client != nil else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            taskState.tasksErrorMessage = "CLI client not available."
            return
        }

        guard hasRalphQueueFile else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            stopFileWatching()
            taskState.tasks = []
            taskState.tasksErrorMessage = """
            No Ralph queue found in this directory. Run `ralph init --non-interactive` in \(identityState.workingDirectoryURL.path).
            """
            diagnosticsState.showErrorRecovery = false
            diagnosticsState.lastRecoveryError = nil
            return
        }

        startFileWatching()
        await performRepositoryLoad(
            operation: "loadTasks",
            retryConfiguration: retryConfiguration,
            setLoading: { [taskState] in taskState.tasksLoading = $0 },
            clearFailure: { [taskState, diagnosticsState] in
                taskState.tasksErrorMessage = nil
                diagnosticsState.showErrorRecovery = false
                diagnosticsState.lastRecoveryError = nil
            },
            handleMissingClient: { [taskState] in
                taskState.tasksErrorMessage = "CLI client not available."
            },
            retryMessage: { attempt, maxAttempts in
                "Retrying load tasks (attempt \(attempt)/\(maxAttempts))..."
            },
            load: { client, workingDirectoryURL, _, onRetry in
                try await self.decodeMachineRepositoryJSON(
                    MachineQueueReadDocument.self,
                    client: client,
                    machineArguments: ["queue", "read"],
                    currentDirectoryURL: workingDirectoryURL,
                    retryConfiguration: retryConfiguration,
                    onRetry: onRetry
                )
            },
            apply: { [self, taskState] snapshot in
                self.updateResolvedPaths(snapshot.paths)
                taskState.tasks = snapshot.active.tasks
                self.sanitizeRunControlSelection()
                taskState.tasksErrorMessage = nil
            },
            handleRetryMessage: { [taskState] in
                taskState.tasksErrorMessage = $0
            },
            handleFailure: { [taskState] recoveryError in
                taskState.tasksErrorMessage = recoveryError.message
            }
        )
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
