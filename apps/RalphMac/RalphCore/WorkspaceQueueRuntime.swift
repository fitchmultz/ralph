/**
 WorkspaceQueueRuntime

 Responsibilities:
 - Own the workspace queue watcher and consume its event stream.
 - Serialize watcher-driven queue/config refresh work for one workspace.
 - Surface watcher health into workspace diagnostics as first-class operational state.

 Does not handle:
 - Manual CLI-driven queue loads.
 - Task filtering, sorting, or presentation.
 - Run-loop lifecycle beyond refreshing resolved runner configuration.

 Invariants/assumptions callers must respect:
 - This runtime is workspace-local and must stay confined to one workspace instance.
 - Refresh batches are serialized to preserve task snapshot diffs for UI reactions.
 - Watcher events are authoritative for watcher health state.
 */

import Foundation

@MainActor
final class WorkspaceQueueRuntime {
    private unowned let workspace: Workspace
    private var watcher: QueueFileWatcher?
    private var watcherEventsTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?
    private var pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
    private var lastTasksSnapshot: [RalphTask] = []

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    deinit {
        watcherEventsTask?.cancel()
        refreshTask?.cancel()
        if let watcher {
            Task {
                await watcher.stop()
            }
        }
    }

    func startWatchingIfNeeded() {
        guard watcher == nil, workspace.hasRalphQueueFile else {
            if !workspace.hasRalphQueueFile {
                workspace.updateWatcherHealth(.idle(for: workspace.workingDirectoryURL))
            }
            return
        }

        let watcher = QueueFileWatcher(workingDirectoryURL: workspace.workingDirectoryURL)
        self.watcher = watcher
        workspace.updateWatcherHealth(.idle(for: workspace.workingDirectoryURL))

        watcherEventsTask = Task { [weak self, watcher] in
            for await event in watcher.events {
                guard !Task.isCancelled else { return }
                await MainActor.run {
                    self?.handleWatcherEvent(event)
                }
            }
        }

        Task {
            await watcher.start()
        }
    }

    func stopWatching() {
        watcherEventsTask?.cancel()
        watcherEventsTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
        lastTasksSnapshot.removeAll(keepingCapacity: false)

        let activeWatcher = watcher
        watcher = nil
        workspace.updateWatcherHealth(.stopped(for: workspace.workingDirectoryURL))

        if let activeWatcher {
            Task {
                await activeWatcher.stop()
            }
        }
    }

    func restartWatching() {
        stopWatching()
        startWatchingIfNeeded()
    }

    func repairWatching() {
        restartWatching()
    }

    private func handleWatcherEvent(_ event: QueueFileWatcher.Event) {
        switch event {
        case .healthChanged(let health):
            workspace.updateWatcherHealth(health)
        case .filesChanged(let batch):
            pendingBatch = pendingBatch.merged(with: batch)
            drainRefreshQueueIfNeeded()
        }
    }

    private func drainRefreshQueueIfNeeded() {
        guard refreshTask == nil else { return }

        refreshTask = Task { @MainActor [weak self] in
            guard let self else { return }

            while !Task.isCancelled {
                let batch = pendingBatch
                pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
                guard !batch.fileNames.isEmpty else { break }
                await process(batch: batch)
            }

            refreshTask = nil
            if !pendingBatch.fileNames.isEmpty {
                drainRefreshQueueIfNeeded()
            }
        }
    }

    private func process(batch: QueueFileWatcher.FileChangeBatch) async {
        if batch.affectsQueueSnapshot {
            lastTasksSnapshot = workspace.tasks

            switch await attemptDirectQueueParse() {
            case .success(let parsedTasks):
                workspace.tasks = parsedTasks
                workspace.sanitizeRunControlSelection()
                workspace.tasksErrorMessage = nil
                workspace.lastRecoveryError = nil
                RalphLogger.shared.debug(
                    "Direct queue parse succeeded: \(parsedTasks.count) tasks",
                    category: .fileWatching
                )
            case .failure(let error):
                RalphLogger.shared.info(
                    "Direct parse failed, falling back to CLI: \(error.localizedDescription)",
                    category: .fileWatching
                )
                await workspace.loadTasks()
            }

            workspace.lastQueueRefreshEvent = Workspace.QueueRefreshEvent(
                source: .externalFileChange,
                previousTasks: lastTasksSnapshot,
                currentTasks: workspace.tasks
            )
        }

        if batch.affectsRunnerConfiguration {
            await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        }
    }

    private func attemptDirectQueueParse() async -> DirectParseResult {
        do {
            let tasks = try await WorkspaceQueueSnapshotLoader.loadQueueTasks(from: workspace.queueFileURL)
            return .success(tasks: tasks)
        } catch {
            return .failure(error)
        }
    }
}

private extension WorkspaceQueueRuntime {
    enum DirectParseResult {
        case success(tasks: [RalphTask])
        case failure(any Error)
    }
}
