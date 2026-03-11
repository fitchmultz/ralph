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
    public struct RepositoryContext: Sendable, Equatable {
        public let generation: UInt64
        public let workingDirectoryURL: URL

        public init(generation: UInt64, workingDirectoryURL: URL) {
            self.generation = generation
            self.workingDirectoryURL = workingDirectoryURL
        }
    }

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
        runnerController.runMachine(arguments: ["system", "info"])
    }

    public func runInit() {
        runnerController.run(arguments: ["--no-color", "init", "--force", "--non-interactive"])
    }

    public func runQueueListJSON() {
        runnerController.runMachine(arguments: ["queue", "read"])
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

    public static func normalizedWorkingDirectoryURL(_ url: URL) -> URL {
        url.standardizedFileURL.resolvingSymlinksInPath()
    }

    var normalizedWorkingDirectoryURL: URL {
        Self.normalizedWorkingDirectoryURL(identityState.workingDirectoryURL)
    }

    public func matchesWorkingDirectory(_ url: URL) -> Bool {
        normalizedWorkingDirectoryURL == Self.normalizedWorkingDirectoryURL(url)
    }

    public var isURLRoutingPlaceholderWorkspace: Bool {
        guard !runState.isRunning else { return false }

        if !hasRalphQueueFile {
            return true
        }

        guard !taskState.tasksLoading else { return false }
        guard taskState.tasksErrorMessage == nil else { return false }
        return taskState.tasks.isEmpty
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

    func currentRepositoryContext() -> RepositoryContext {
        RepositoryContext(
            generation: identityState.repositoryGeneration,
            workingDirectoryURL: identityState.workingDirectoryURL
        )
    }

    func isCurrentRepositoryContext(_ context: RepositoryContext) -> Bool {
        context.generation == identityState.repositoryGeneration
            && Self.normalizedWorkingDirectoryURL(context.workingDirectoryURL) == normalizedWorkingDirectoryURL
    }

    func beginRepositoryRetarget(to url: URL) -> RepositoryContext {
        let standardizedURL = Self.normalizedWorkingDirectoryURL(url)
        identityState.repositoryGeneration &+= 1
        identityState.retargetRevision &+= 1
        identityState.workingDirectoryURL = standardizedURL
        identityState.name = standardizedURL.lastPathComponent
        return currentRepositoryContext()
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

extension Workspace {
    func performRepositoryLoad<Value>(
        operation: String,
        retryConfiguration: RetryConfiguration,
        setLoading: @escaping @MainActor (Bool) -> Void,
        clearFailure: @escaping @MainActor () -> Void,
        handleMissingClient: @escaping @MainActor () -> Void,
        retryMessage: (@Sendable (Int, Int) -> String)? = nil,
        load: @escaping @Sendable (RalphCLIClient, URL, RetryConfiguration, RetryProgressHandler?) async throws -> Value,
        apply: @escaping @MainActor (Value) -> Void,
        handleRetryMessage: (@MainActor (String) -> Void)? = nil,
        handleFailure: @escaping @MainActor (RecoveryError) -> Void
    ) async {
        let repositoryContext = currentRepositoryContext()
        guard let client else {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            handleMissingClient()
            return
        }

        setLoading(true)
        clearFailure()
        defer {
            if isCurrentRepositoryContext(repositoryContext) {
                setLoading(false)
            }
        }

        let progress: RetryProgressHandler?
        if let retryMessage {
            progress = { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    guard
                        let self,
                        self.isCurrentRepositoryContext(repositoryContext)
                    else { return }
                    handleRetryMessage?(retryMessage(attempt, maxAttempts))
                }
            }
        } else {
            progress = nil
        }

        do {
            let value = try await load(
                client,
                identityState.workingDirectoryURL,
                retryConfiguration,
                progress
            )
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            apply(value)
        } catch {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: operation,
                workspaceURL: identityState.workingDirectoryURL
            )
            diagnosticsState.lastRecoveryError = recoveryError
            diagnosticsState.showErrorRecovery = true
            handleFailure(recoveryError)
        }
    }

    func decodeRepositoryJSON<T: Decodable>(
        _ type: T.Type,
        client: RalphCLIClient,
        arguments: [String],
        currentDirectoryURL: URL,
        retryConfiguration: RetryConfiguration,
        onRetry: RetryProgressHandler? = nil
    ) async throws -> T {
        let collected = try await client.runAndCollectWithRetry(
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            retryConfiguration: retryConfiguration,
            onRetry: onRetry
        )
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(type, from: Data(collected.stdout.utf8))
    }

    func decodeMachineRepositoryJSON<T: Decodable>(
        _ type: T.Type,
        client: RalphCLIClient,
        machineArguments: [String],
        currentDirectoryURL: URL,
        retryConfiguration: RetryConfiguration,
        onRetry: RetryProgressHandler? = nil
    ) async throws -> T {
        try await decodeRepositoryJSON(
            type,
            client: client,
            arguments: ["--no-color", "machine"] + machineArguments,
            currentDirectoryURL: currentDirectoryURL,
            retryConfiguration: retryConfiguration,
            onRetry: onRetry
        )
    }

    public func updateResolvedPaths(_ paths: MachineQueuePaths) {
        identityState.resolvedPaths = paths
    }
}
