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
            tasks = []
            tasksErrorMessage = "No Ralph queue found in this directory. Run `ralph init --non-interactive` in \(workingDirectoryURL.path)."
            showErrorRecovery = false
            lastRecoveryError = nil
            return
        }

        if fileWatcher == nil {
            startFileWatching()
        }

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
        fileWatcher?.stop()
        fileWatcher = nil
    }
}

extension Workspace {
    func startFileWatching() {
        fileWatcher?.stop()
        fileWatcher = nil

        guard hasRalphQueueFile else { return }

        let watcher = QueueFileWatcher(workingDirectoryURL: workingDirectoryURL)
        watcher.onFileChanged = { [weak self] in
            Task { @MainActor [weak self] in
                await self?.handleExternalFileChange()
            }
        }
        watcher.start()
        fileWatcher = watcher
    }
}

private extension Workspace {
    enum DirectParseResult {
        case success(tasks: [RalphTask])
        case failure(any Error)
    }

    func attemptDirectQueueParse() async -> DirectParseResult {
        do {
            let tasks = try await WorkspaceQueueSnapshotLoader.loadQueueTasks(from: queueFileURL)
            return .success(tasks: tasks)
        } catch {
            return .failure(error)
        }
    }

    func handleExternalFileChange() async {
        lastTasksSnapshot = tasks

        switch await attemptDirectQueueParse() {
        case .success(let parsedTasks):
            tasks = parsedTasks
            sanitizeRunControlSelection()
            tasksErrorMessage = nil

            RalphLogger.shared.debug(
                "Direct queue parse succeeded: \(parsedTasks.count) tasks",
                category: .fileWatching
            )

        case .failure(let error):
            RalphLogger.shared.info(
                "Direct parse failed, falling back to CLI: \(error.localizedDescription)",
                category: .fileWatching
            )
            await loadTasks()
        }

        await loadRunnerConfiguration(retryConfiguration: .minimal)

        lastQueueRefreshEvent = QueueRefreshEvent(
            source: .externalFileChange,
            previousTasks: lastTasksSnapshot,
            currentTasks: tasks
        )
    }
}
