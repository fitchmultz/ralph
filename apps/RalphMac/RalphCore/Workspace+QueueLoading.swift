//! Workspace+QueueLoading
//!
//! Purpose:
//! - Load queue tasks through the Ralph machine contract.
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
//!
//! Usage:
//! - Used by the RalphMac app or RalphCore tests through its owning feature surface.
//! Invariants/assumptions callers must respect:
//! - The workspace must point at a Ralph-initialized directory to load tasks.
//! - Queue snapshots are always sourced from `ralph machine queue read`.
//! - Queue refresh events retain previous and current task snapshots for view-local reactions.
//!
public import Foundation
public import Combine

public enum WorkspaceOverviewLoadResult: Sendable, Equatable {
    case loaded
    case fallbackToLegacy
    case failed
}

@MainActor
public final class WorkspaceTaskState: ObservableObject {
    @Published public var tasks: [RalphTask] = []
    @Published public var tasksLoading = false
    @Published public var tasksErrorMessage: String?
    @Published public var lastQueueRefreshEvent: Workspace.QueueRefreshEvent?
    @Published public var nextRunnableTaskID: String?
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
    func loadWorkspaceOverview(
        retryConfiguration: RetryConfiguration = .minimal
    ) async -> WorkspaceOverviewLoadResult {
        let repositoryContext = currentRepositoryContext()
        guard let client else {
            guard isCurrentRepositoryContext(repositoryContext) else { return .failed }
            taskState.tasksErrorMessage = "CLI client not available."
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            runnerController.clearRunnerConfigState(runState)
            runState.runnerConfigErrorMessage = "CLI client not available."
            return .failed
        }

        taskState.tasksLoading = true
        runState.runnerConfigLoading = true
        taskState.tasksErrorMessage = nil
        runState.runnerConfigErrorMessage = nil
        diagnosticsState.showErrorRecovery = false
        diagnosticsState.lastRecoveryError = nil

        defer {
            if !isShutDown, isCurrentRepositoryContext(repositoryContext) {
                taskState.tasksLoading = false
                runState.runnerConfigLoading = false
            }
        }

        do {
            let document = try await decodeMachineRepositoryJSON(
                MachineWorkspaceOverviewDocument.self,
                client: client,
                machineArguments: ["workspace", "overview"],
                currentDirectoryURL: identityState.workingDirectoryURL,
                retryConfiguration: retryConfiguration
            )
            guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else {
                return .failed
            }

            applyQueueReadDocument(document.queue)
            runnerController.applyConfigResolveDocument(document.config, workspace: self)
            startFileWatching()
            return .loaded
        } catch is CancellationError {
            return .failed
        } catch {
            guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else {
                return .failed
            }

            if Self.shouldFallbackToLegacyWorkspaceOverview(for: error) {
                return .fallbackToLegacy
            }

            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadWorkspaceOverview",
                workspaceURL: identityState.workingDirectoryURL
            )
            diagnosticsState.lastRecoveryError = recoveryError
            diagnosticsState.showErrorRecovery = true
            taskState.tasksErrorMessage = recoveryError.message
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            runnerController.clearRunnerConfigState(runState)
            runState.runnerConfigErrorMessage = recoveryError.message
            runState.refreshOperatorStateForDisplay()
            stopFileWatching()
            return .failed
        }
    }

    func loadTasks(retryConfiguration: RetryConfiguration = .default) async {
        let repositoryContext = currentRepositoryContext()
        guard client != nil else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            taskState.tasksErrorMessage = "CLI client not available."
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            return
        }

        guard hasRalphQueueFile else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            stopFileWatching()
            taskState.tasks = []
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            taskState.tasksErrorMessage = """
            No Ralph queue found in this directory. Run `ralph init --non-interactive` in \(identityState.workingDirectoryURL.path).
            """
            diagnosticsState.showErrorRecovery = false
            diagnosticsState.lastRecoveryError = nil
            return
        }

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
                taskState.nextRunnableTaskID = nil
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
            apply: { [self] snapshot in
                self.applyQueueReadDocument(snapshot)
                self.startFileWatching()
            },
            handleRetryMessage: { [taskState] in
                taskState.tasksErrorMessage = $0
            },
            handleFailure: { [taskState, runState = self.runState] recoveryError in
                taskState.tasksErrorMessage = recoveryError.message
                taskState.nextRunnableTaskID = nil
                runState.refreshOperatorStateForDisplay()
            }
        )
    }

    func stopFileWatching() {
        queueRuntime.stopWatching()
    }
}

extension Workspace {
    nonisolated static func shouldFallbackToLegacyWorkspaceOverview(
        for error: any Error
    ) -> Bool {
        guard case .processError(let exitCode, let stderr) = error as? RetryableError else {
            return false
        }
        if MachineErrorDocument.decode(from: stderr) != nil {
            return false
        }
        let normalized = stderr.lowercased()
        let trimmed = normalized.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.contains("unrecognized subcommand")
            || normalized.contains("unexpected argument")
            || normalized.contains("unknown command")
            || normalized.contains("unexpected args:")
            || normalized.contains("usage:")
            || (trimmed.isEmpty && (exitCode == 2 || exitCode == 64))
    }

    func applyQueueReadDocument(_ snapshot: MachineQueueReadDocument) {
        updateResolvedPaths(snapshot.paths)
        taskState.tasks = snapshot.active.tasks
        taskState.nextRunnableTaskID = snapshot.nextRunnableTaskID
        if !runState.isRunning {
            runState.clearLiveBlockingState()
        }
        runState.setQueueBlockingState(
            snapshot.runnability.decode(
                WorkspaceRunnerController.MachineBlockingState.self,
                at: ["summary", "blocking"]
            )?.asWorkspaceBlockingState()
        )
        sanitizeRunControlSelection()
        taskState.tasksErrorMessage = nil
    }

    func startFileWatching() {
        queueRuntime.startWatchingIfNeeded()
    }
}
