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
    enum QueueAccessPreflightResult {
        case ready
        case configResolutionFailed(RecoveryError)
        case missingConfiguredQueueFile(URL)
    }

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
            try validateWorkspaceOverviewDocument(document)

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

            if await shouldFallbackToLegacyWorkspaceOverview(
                for: error,
                client: client,
                workingDirectoryURL: identityState.workingDirectoryURL
            ) {
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
        guard let client else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            taskState.tasksErrorMessage = "CLI client not available."
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            return
        }

        let queueAccess = await ensureQueueAccessPreflight(
            client: client,
            retryConfiguration: retryConfiguration
        )
        guard isCurrentRepositoryContext(repositoryContext), !isShutDown, !Task.isCancelled else {
            return
        }

        switch queueAccess {
        case .ready:
            break
        case .configResolutionFailed(let recoveryError):
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            stopFileWatching()
            taskState.tasks = []
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            taskState.tasksErrorMessage = recoveryError.message
            diagnosticsState.showErrorRecovery = true
            diagnosticsState.lastRecoveryError = recoveryError
            runState.refreshOperatorStateForDisplay()
            return
        case .missingConfiguredQueueFile(let queueURL):
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            stopFileWatching()
            taskState.tasks = []
            taskState.nextRunnableTaskID = nil
            runState.clearQueueBlockingState()
            taskState.tasksErrorMessage = Self.missingConfiguredQueueMessage(for: queueURL)
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
    private enum WorkspaceOverviewCapability: Sendable, Equatable {
        case supported
        case unsupported
        case unknown
    }

    func ensureQueueAccessPreflight(
        client: RalphCLIClient,
        retryConfiguration: RetryConfiguration
    ) async -> QueueAccessPreflightResult {
        do {
            let document = try await decodeMachineRepositoryJSON(
                MachineConfigResolveDocument.self,
                client: client,
                machineArguments: ["config", "resolve"],
                currentDirectoryURL: identityState.workingDirectoryURL,
                retryConfiguration: retryConfiguration
            )
            runnerController.applyConfigResolveDocument(document, workspace: self)
        } catch is CancellationError {
            return .configResolutionFailed(
                RecoveryError.classify(
                    error: CancellationError(),
                    operation: "resolve queue paths",
                    workspaceURL: identityState.workingDirectoryURL
                )
            )
        } catch {
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "resolve queue paths",
                workspaceURL: identityState.workingDirectoryURL
            )
            return .configResolutionFailed(recoveryError)
        }

        guard configuredQueueFileExists else {
            return .missingConfiguredQueueFile(queueFileURL)
        }

        return .ready
    }

    nonisolated static func missingConfiguredQueueMessage(for queueURL: URL) -> String {
        """
        No Ralph queue was found at the configured path:
        \(queueURL.path)

        Confirm the active Ralph queue configuration or inspect `ralph machine config resolve` for the current machine-resolved queue paths.
        """
    }

    private func shouldFallbackToLegacyWorkspaceOverview(
        for error: any Error,
        client: RalphCLIClient,
        workingDirectoryURL: URL
    ) async -> Bool {
        guard case .processError(_, let stderr) = error as? RetryableError else {
            return false
        }
        do {
            if try MachineErrorDocument.decodeIfPresent(
                from: stderr,
                operation: "loadWorkspaceOverview"
            ) != nil {
                return false
            }
        } catch {
            return false
        }

        let capability = await workspaceOverviewCapability(
            client: client,
            workingDirectoryURL: workingDirectoryURL
        )
        return capability == .unsupported
    }

    private func workspaceOverviewCapability(
        client: RalphCLIClient,
        workingDirectoryURL: URL
    ) async -> WorkspaceOverviewCapability {
        let output: RalphCLIClient.CollectedOutput
        do {
            output = try await client.runAndCollect(
                arguments: ["--no-color", "machine", "cli-spec"],
                currentDirectoryURL: workingDirectoryURL
            )
        } catch {
            return .unknown
        }

        guard output.status.code == 0 else {
            return .unknown
        }

        do {
            let document = try RalphMachineContract.decode(
                MachineCLISpecDocument.self,
                from: Data(output.stdout.utf8),
                operation: "machine cli-spec capability probe"
            )
            guard document.spec.version == RalphCLISpecDocument.expectedVersion else {
                return .unknown
            }
            return document.spec.containsCommandSuffix(["machine", "workspace", "overview"])
                ? .supported
                : .unsupported
        } catch {
            return .unknown
        }
    }

    func applyQueueReadDocument(_ snapshot: MachineQueueReadDocument) {
        updateResolvedPaths(snapshot.paths)
        taskState.tasks = snapshot.active.tasks
        taskState.nextRunnableTaskID = snapshot.nextRunnableTaskID
        if !runState.isExecutionActive {
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

    func validateWorkspaceOverviewDocument(_ document: MachineWorkspaceOverviewDocument) throws {
        try RalphMachineContract.requireVersion(
            document.queue.version,
            expected: MachineQueueReadDocument.expectedVersion,
            document: MachineQueueReadDocument.documentName,
            operation: "machine workspace overview"
        )
        try RalphMachineContract.requireVersion(
            document.config.version,
            expected: MachineConfigResolveDocument.expectedVersion,
            document: MachineConfigResolveDocument.documentName,
            operation: "machine workspace overview"
        )
    }

    func startFileWatching() {
        queueRuntime.startWatchingIfNeeded()
    }
}
