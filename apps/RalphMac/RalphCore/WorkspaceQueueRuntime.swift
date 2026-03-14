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
    private var watcherStartTask: Task<Void, Never>?
    private var watcherStopTask: Task<Void, Never>?
    private var watchTargetSyncTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?
    private var pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
    private var lastTasksSnapshot: [RalphTask] = []

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    deinit {
        watcherEventsTask?.cancel()
        watcherStartTask?.cancel()
        watcherStopTask?.cancel()
        watchTargetSyncTask?.cancel()
        refreshTask?.cancel()
    }

    func startWatchingIfNeeded() {
        guard let workspace, !workspace.isShutDown else { return }
        guard watcher == nil, workspace.hasRalphQueueFile else {
            if !workspace.hasRalphQueueFile {
                workspace.updateWatcherHealth(.idle(for: workspace.identityState.workingDirectoryURL))
            }
            return
        }

        let watcher = QueueFileWatcher(targets: workspace.queueWatcherTargets)
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

        watcherStartTask?.cancel()
        watcherStartTask = Task { [weak self, watcher] in
            guard !Task.isCancelled else { return }
            await watcher.start()
            await MainActor.run {
                guard let self, self.watcher === watcher else { return }
                self.watcherStartTask = nil
            }
        }
    }

    func stopWatching() {
        guard let workspace else {
            watcherEventsTask?.cancel()
            watcherEventsTask = nil
            watcherStartTask?.cancel()
            watcherStartTask = nil
            watcherStopTask?.cancel()
            watcherStopTask = nil
            watchTargetSyncTask?.cancel()
            watchTargetSyncTask = nil
            refreshTask?.cancel()
            refreshTask = nil
            pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
            lastTasksSnapshot.removeAll(keepingCapacity: false)
            watcher = nil
            return
        }
        watcherEventsTask?.cancel()
        watcherEventsTask = nil
        watcherStartTask?.cancel()
        watcherStartTask = nil
        watcherStopTask?.cancel()
        watchTargetSyncTask?.cancel()
        watchTargetSyncTask = nil
        refreshTask?.cancel()
        refreshTask = nil
        pendingBatch = QueueFileWatcher.FileChangeBatch(fileNames: [])
        lastTasksSnapshot.removeAll(keepingCapacity: false)

        let activeWatcher = watcher
        watcher = nil
        workspace.updateWatcherHealth(.stopped(for: workspace.identityState.workingDirectoryURL))

        if let activeWatcher {
            watcherStopTask = Task {
                await activeWatcher.stop()
            }
        } else {
            watcherStopTask = nil
        }
    }

    func restartWatching() {
        stopWatching()
        startWatchingIfNeeded()
    }

    func syncWatchTargetsIfNeeded() {
        guard let workspace, !workspace.isShutDown else { return }

        guard workspace.hasRalphQueueFile else {
            stopWatching()
            workspace.updateWatcherHealth(.idle(for: workspace.identityState.workingDirectoryURL))
            return
        }

        guard let watcher else {
            startWatchingIfNeeded()
            return
        }

        let targets = workspace.queueWatcherTargets
        watchTargetSyncTask?.cancel()
        watchTargetSyncTask = Task {
            await watcher.updateTargets(targets)
        }
    }

    func prepareForRepositoryRetarget() {
        stopWatching()
    }

    func repairWatching() {
        restartWatching()
    }

    private func handleWatcherEvent(_ event: QueueFileWatcher.Event) {
        guard let workspace, !workspace.isShutDown else { return }
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
        guard let workspace, !workspace.isShutDown else { return }
        let repositoryContext = workspace.currentRepositoryContext()

        if batch.affectsQueueSnapshot {
            lastTasksSnapshot = workspace.taskState.tasks

            await workspace.refreshRepositoryState(
                retryConfiguration: .minimal,
                includeCLISpec: false
            )

            guard !workspace.isShutDown, !Task.isCancelled, workspace.isCurrentRepositoryContext(repositoryContext) else { return }
            workspace.taskState.lastQueueRefreshEvent = Workspace.QueueRefreshEvent(
                source: .externalFileChange,
                previousTasks: lastTasksSnapshot,
                currentTasks: workspace.taskState.tasks
            )
        }

        guard batch.affectsRunnerConfiguration else { return }
        guard !workspace.isShutDown, !Task.isCancelled, workspace.isCurrentRepositoryContext(repositoryContext) else { return }
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
    }
}
