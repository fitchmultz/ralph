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
    private weak var workspace: Workspace?
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
        guard let workspace else { return }
        guard watcher == nil, workspace.hasRalphQueueFile else {
            if !workspace.hasRalphQueueFile {
                workspace.updateWatcherHealth(.idle(for: workspace.identityState.workingDirectoryURL))
            }
            return
        }

        let watcher = QueueFileWatcher(workingDirectoryURL: workspace.identityState.workingDirectoryURL)
        self.watcher = watcher
        workspace.updateWatcherHealth(.idle(for: workspace.identityState.workingDirectoryURL))

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
        guard let workspace else {
            watcherEventsTask?.cancel()
            watcherEventsTask = nil
            refreshTask?.cancel()
            refreshTask = nil
            pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
            lastTasksSnapshot.removeAll(keepingCapacity: false)
            watcher = nil
            return
        }
        watcherEventsTask?.cancel()
        watcherEventsTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
        lastTasksSnapshot.removeAll(keepingCapacity: false)

        let activeWatcher = watcher
        watcher = nil
        workspace.updateWatcherHealth(.stopped(for: workspace.identityState.workingDirectoryURL))

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

    func prepareForRepositoryRetarget() {
        stopWatching()
    }

    func repairWatching() {
        restartWatching()
    }

    private func handleWatcherEvent(_ event: QueueFileWatcher.Event) {
        guard let workspace else { return }
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
        guard let workspace else { return }
        let repositoryContext = workspace.currentRepositoryContext()

        if batch.affectsQueueSnapshot {
            lastTasksSnapshot = workspace.taskState.tasks

            await workspace.loadTasks(retryConfiguration: .minimal)

            guard workspace.isCurrentRepositoryContext(repositoryContext) else { return }
            workspace.taskState.lastQueueRefreshEvent = Workspace.QueueRefreshEvent(
                source: .externalFileChange,
                previousTasks: lastTasksSnapshot,
                currentTasks: workspace.taskState.tasks
            )
        }

        if batch.affectsRunnerConfiguration {
            guard workspace.isCurrentRepositoryContext(repositoryContext) else { return }
            await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        }
    }
}
