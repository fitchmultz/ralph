//!
//! Workspace
//!
//! Purpose:
//! - Coordinate the domain-specific state owners that represent one Ralph workspace.
//!
//! Responsibilities:
//! - Hold the canonical workspace state owners used by the app surface.
//! - Bridge nested state changes into a single observable object for SwiftUI.
//! - Own workspace-local runtime collaborators such as the CLI client, queue runtime, and runner controller.
//!
//! Scope:
//! - One workspace only. App-wide lifecycle, routing, and shared persistence stay in `WorkspaceManager`.
//!
//! Usage:
//! - Views and helpers should read and mutate `identityState`, `commandState`, `taskState`, `insightsState`,
//!   `diagnosticsState`, and `runState` directly instead of depending on facade pass-through properties.
//!
//! Invariants/Assumptions:
//! - All mutations occur on the main actor.
//! - Domain owners are the storage boundary; `Workspace` remains an orchestrator, not a proxy bag.
//! - Runtime collaborators remain workspace-local and must not leak across workspaces.

public import Combine
public import Foundation

@MainActor
public final class Workspace: ObservableObject, Identifiable {
    public let id: UUID

    public let identityState: WorkspaceIdentityState
    public let commandState: WorkspaceCommandState
    public let taskState: WorkspaceTaskState
    public let insightsState: WorkspaceInsightsState
    public let diagnosticsState: WorkspaceDiagnosticsState
    public let runState: WorkspaceRunState

    lazy var queueRuntime = WorkspaceQueueRuntime(workspace: self)
    lazy var runnerController = WorkspaceRunnerController(workspace: self)

    var client: RalphCLIClient?

    private var relayCancellables = Set<AnyCancellable>()
    private var operationalDependencyCancellables = Set<AnyCancellable>()

    public init(
        id: UUID = UUID(),
        name: String? = nil,
        workingDirectoryURL: URL,
        client: RalphCLIClient? = nil
    ) {
        self.id = id
        identityState = WorkspaceIdentityState(
            name: name ?? workingDirectoryURL.lastPathComponent,
            workingDirectoryURL: workingDirectoryURL,
            recentWorkingDirectories: []
        )
        commandState = WorkspaceCommandState()
        taskState = WorkspaceTaskState()
        insightsState = WorkspaceInsightsState()
        diagnosticsState = WorkspaceDiagnosticsState(
            watcherHealth: .idle(for: workingDirectoryURL)
        )
        runState = WorkspaceRunState(outputBuffer: ConsoleOutputBuffer.loadFromUserDefaults())
        self.client = client

        bindDomainStateChanges()
        bindOperationalDependencies()
        loadState()
        persistState()
        startFileWatching()
        refreshOperationalHealth()

        if client != nil {
            Task { @MainActor [weak self] in
                await self?.loadRunnerConfiguration(retryConfiguration: .minimal)
            }
        }
    }

    public func injectClient(_ client: RalphCLIClient) {
        self.client = client
        Task { @MainActor in
            await loadCLISpec()
            await loadRunnerConfiguration(retryConfiguration: .minimal)
            refreshOperationalHealth()
        }
    }

    public func runVersion() {
        runnerController.run(arguments: ["--no-color", "version"])
    }

    public func runInit() {
        runnerController.run(arguments: ["--no-color", "init", "--force", "--non-interactive"])
    }

    public func runQueueListJSON() {
        runnerController.run(arguments: ["--no-color", "queue", "list", "--format", "json"])
    }

    public func nextTask() -> RalphTask? {
        taskState.tasks.first { $0.status == .todo }
    }

    public var runControlTodoTasks: [RalphTask] {
        taskState.tasks.filter { $0.status == .todo }
    }

    public var selectedRunControlTask: RalphTask? {
        guard let selectedID = runState.runControlSelectedTaskID else { return nil }
        return runControlTodoTasks.first { $0.id == selectedID }
    }

    public var runControlPreviewTask: RalphTask? {
        selectedRunControlTask ?? nextTask()
    }

    public func refreshRunControlData() async {
        await loadTasks(retryConfiguration: .minimal)
        await loadRunnerConfiguration(retryConfiguration: .minimal)
    }

    public func isTaskBlocked(_ task: RalphTask) -> Bool {
        guard let dependsOn = task.dependsOn, !dependsOn.isEmpty else {
            return false
        }

        let tasksByID = Dictionary(uniqueKeysWithValues: taskState.tasks.map { ($0.id, $0) })
        for dependencyID in dependsOn where tasksByID[dependencyID]?.status != .done {
            return true
        }
        return false
    }

    public func isTaskOverdue(_ task: RalphTask) -> Bool {
        guard task.status == .todo || task.status == .draft else { return false }
        guard task.priority == .high || task.priority == .critical else { return false }
        guard let createdAt = task.createdAt else { return false }
        let daysSinceCreation = Date().timeIntervalSince(createdAt) / (24 * 3600)
        return daysSinceCreation > 7
    }

    func sanitizeRunControlSelection() {
        guard let selectedID = runState.runControlSelectedTaskID else { return }
        let isRunnable = runControlTodoTasks.contains { $0.id == selectedID }
        if !isRunnable {
            runState.runControlSelectedTaskID = nil
        }
    }

    private func bindDomainStateChanges() {
        [
            identityState.objectWillChange.eraseToAnyPublisher(),
            commandState.objectWillChange.eraseToAnyPublisher(),
            taskState.objectWillChange.eraseToAnyPublisher(),
            insightsState.objectWillChange.eraseToAnyPublisher(),
            diagnosticsState.objectWillChange.eraseToAnyPublisher(),
            runState.objectWillChange.eraseToAnyPublisher(),
        ]
        .forEach { publisher in
            publisher
                .sink { [weak self] _ in
                    self?.objectWillChange.send()
                }
                .store(in: &relayCancellables)
        }
    }

    private func bindOperationalDependencies() {
        WorkspaceManager.shared.objectWillChange
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.refreshOperationalHealth()
                }
            }
            .store(in: &operationalDependencyCancellables)

        CrashReporter.shared.objectWillChange
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.refreshOperationalHealth()
                }
            }
            .store(in: &operationalDependencyCancellables)
    }
}
