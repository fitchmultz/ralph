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
    public enum LaunchDisposition: Sendable, Equatable {
        case regular
        case startupPlaceholder
    }

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

    let cliHealthChecker = CLIHealthChecker()

    public private(set) var launchDisposition: LaunchDisposition
    var client: RalphCLIClient?
    var isShutDown = false

    private var relayCancellables = Set<AnyCancellable>()
    private var operationalDependencyCancellables = Set<AnyCancellable>()
    private var repositoryActivityTask: Task<Void, Never>?
    private var operationalRefreshTask: Task<Void, Never>?
    private var healthCheckTask: Task<Void, Never>?
    private var repositoryActivityRevision: UInt64 = 0
    private var healthCheckRevision: UInt64 = 0

    public init(
        id: UUID = UUID(),
        name: String? = nil,
        workingDirectoryURL: URL,
        launchDisposition: LaunchDisposition = .regular,
        client: RalphCLIClient? = nil,
        bootstrapRepositoryStateOnInit: Bool = true
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
        self.launchDisposition = launchDisposition
        self.client = client

        bindDomainStateChanges()
        bindOperationalDependencies()
        loadState()
        backfillWorkingDirectoryBookmarkIfNeeded()
        persistState()
        refreshOperationalHealth()

        if client != nil, bootstrapRepositoryStateOnInit {
            scheduleRepositoryActivity {
                await $0.refreshWorkspaceOverviewState(retryConfiguration: .minimal)
            }
        }
    }

    public func injectClient(_ client: RalphCLIClient) {
        guard !isShutDown else { return }
        self.client = client
        scheduleRepositoryActivity {
            await $0.refreshWorkspaceOverviewState(retryConfiguration: .minimal)
            $0.refreshOperationalHealth()
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
        guard let nextRunnableTaskID = taskState.nextRunnableTaskID else { return nil }
        return taskState.tasks.first { $0.id == nextRunnableTaskID }
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
        launchDisposition == .startupPlaceholder && !runState.isRunning
    }

    public func markStartupPlaceholderConsumed() {
        launchDisposition = .regular
    }

    public func refreshRunControlData() async {
        await awaitPendingRepositoryActivityIfNeeded()
        await refreshWorkspaceOverviewState(retryConfiguration: .minimal)
    }

    public func scheduleInitialRepositoryBootstrapIfNeeded() {
        guard !isShutDown else { return }
        guard client != nil else { return }
        guard repositoryActivityTask == nil else { return }
        guard !taskState.tasksLoading, !runState.runnerConfigLoading else { return }
        guard taskState.tasks.isEmpty else { return }
        guard taskState.tasksErrorMessage == nil else { return }
        guard runState.currentRunnerConfig == nil else { return }
        guard runState.runnerConfigErrorMessage == nil else { return }

        scheduleRepositoryActivity {
            await $0.refreshWorkspaceOverviewState(retryConfiguration: .minimal)
        }
    }

    func refreshWorkspaceOverviewState(
        retryConfiguration: RetryConfiguration
    ) async {
        guard !isShutDown, !Task.isCancelled else { return }
        let overviewResult = await loadWorkspaceOverview(retryConfiguration: retryConfiguration)
        if overviewResult == .fallbackToLegacy {
            await loadTasks(retryConfiguration: retryConfiguration)
            guard !isShutDown, !Task.isCancelled else { return }
            await loadRunnerConfiguration(retryConfiguration: retryConfiguration)
        } else if overviewResult == .failed {
            return
        }
        guard !isShutDown, !Task.isCancelled else { return }
        await refreshParallelStatusIfNeeded(retryConfiguration: retryConfiguration)
    }

    public func refreshRunControlStatusData() async {
        await awaitPendingRepositoryActivityIfNeeded()
        guard !isShutDown, !Task.isCancelled else { return }
        await loadRunnerConfiguration(retryConfiguration: .minimal)
        guard !isShutDown, !Task.isCancelled else { return }
        await refreshParallelStatusIfNeeded(retryConfiguration: .minimal)
    }

    public func refreshRepositoryState(
        retryConfiguration: RetryConfiguration = .minimal,
        includeCLISpec: Bool = true
    ) async {
        guard !isShutDown, !Task.isCancelled else { return }
        await loadTasks(retryConfiguration: retryConfiguration)

        guard !isShutDown, !Task.isCancelled else { return }
        let queueSnapshotLoaded = taskState.tasksErrorMessage == nil
        guard queueSnapshotLoaded else {
            if includeCLISpec {
                await loadCLISpec(retryConfiguration: retryConfiguration)
                guard !isShutDown, !Task.isCancelled else { return }
            }
            await loadRunnerConfiguration(retryConfiguration: retryConfiguration)
            guard !isShutDown, !Task.isCancelled else { return }
            await refreshParallelStatusIfNeeded(retryConfiguration: retryConfiguration)
            return
        }

        await loadGraphData(retryConfiguration: retryConfiguration)
        guard !isShutDown, !Task.isCancelled else { return }
        await loadAnalytics(timeRange: insightsState.analytics.timeRange)
        guard !isShutDown, !Task.isCancelled else { return }

        if includeCLISpec {
            await loadCLISpec(retryConfiguration: retryConfiguration)
            guard !isShutDown, !Task.isCancelled else { return }
        }

        await loadRunnerConfiguration(retryConfiguration: retryConfiguration)
        guard !isShutDown, !Task.isCancelled else { return }
        await refreshParallelStatusIfNeeded(retryConfiguration: retryConfiguration)
    }

    func awaitPendingRepositoryActivityIfNeeded() async {
        if let repositoryActivityTask {
            await repositoryActivityTask.value
        }
    }

    func refreshParallelStatusIfNeeded(retryConfiguration: RetryConfiguration) async {
        guard !isShutDown, !Task.isCancelled else { return }
        guard runState.currentRunnerConfig?.safety?.parallelConfigured == true
            || runState.runControlParallelWorkersOverride != nil
            || runState.hasMeaningfulParallelStatus else {
            runState.clearParallelStatus()
            return
        }
        await loadParallelStatus(retryConfiguration: retryConfiguration)
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
        runState.clearParallelStatus()
        runState.clearRunControlOperatorState()
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
                self?.scheduleOperationalHealthRefresh()
            }
            .store(in: &operationalDependencyCancellables)

        CrashReporter.shared.objectWillChange
            .sink { [weak self] _ in
                self?.scheduleOperationalHealthRefresh()
            }
            .store(in: &operationalDependencyCancellables)
    }

    func scheduleRepositoryActivity(
        _ operation: @escaping @MainActor (Workspace) async -> Void
    ) {
        guard !isShutDown else { return }

        repositoryActivityTask?.cancel()
        repositoryActivityRevision &+= 1
        let revision = repositoryActivityRevision

        repositoryActivityTask = Task { @MainActor [weak self] in
            guard let self, !self.isShutDown else { return }
            await operation(self)
            guard self.repositoryActivityRevision == revision else { return }
            self.repositoryActivityTask = nil
        }
    }

    func scheduleOperationalHealthRefresh() {
        guard !isShutDown else { return }

        operationalRefreshTask?.cancel()
        operationalRefreshTask = Task { @MainActor [weak self] in
            await Task.yield()
            guard let self, !Task.isCancelled, !self.isShutDown else { return }
            self.refreshOperationalHealth()
            self.operationalRefreshTask = nil
        }
    }

    func cancelOperationalHealthRefresh() {
        operationalRefreshTask?.cancel()
        operationalRefreshTask = nil
    }

    public func scheduleHealthCheck(loadCachedTasksOnUnavailable: Bool = true) {
        guard !isShutDown else { return }

        healthCheckTask?.cancel()
        healthCheckRevision &+= 1
        let revision = healthCheckRevision
        let repositoryContext = currentRepositoryContext()

        healthCheckTask = Task { @MainActor [weak self] in
            guard let self, !Task.isCancelled, !self.isShutDown else { return }
            let status = await self.checkHealth()
            guard
                !Task.isCancelled,
                !self.isShutDown,
                self.isCurrentRepositoryContext(repositoryContext),
                self.healthCheckRevision == revision
            else {
                return
            }
            if loadCachedTasksOnUnavailable, status.isAvailable == false {
                self.loadCachedTasks()
            }
            self.healthCheckTask = nil
        }
    }

    func cancelHealthCheck() {
        healthCheckTask?.cancel()
        healthCheckTask = nil
        diagnosticsState.isCheckingHealth = false
        healthCheckRevision &+= 1
    }

    func cancelRepositoryActivity() {
        repositoryActivityTask?.cancel()
        repositoryActivityTask = nil
        repositoryActivityRevision &+= 1
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
        guard !isShutDown, !Task.isCancelled else { return }

        let repositoryContext = currentRepositoryContext()
        guard let client else {
            guard !isShutDown, isCurrentRepositoryContext(repositoryContext) else { return }
            handleMissingClient()
            return
        }

        setLoading(true)
        clearFailure()
        defer {
            if !isShutDown, isCurrentRepositoryContext(repositoryContext) {
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
            guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else { return }
            apply(value)
        } catch is CancellationError {
            return
        } catch {
            guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else { return }
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
        let collected = try await collectRepositoryCommand(
            client: client,
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            retryConfiguration: retryConfiguration,
            onRetry: onRetry
        )
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(type, from: Data(collected.stdout.utf8))
    }

    func collectRepositoryCommand(
        client: RalphCLIClient,
        arguments: [String],
        currentDirectoryURL: URL,
        retryConfiguration: RetryConfiguration,
        onRetry: RetryProgressHandler? = nil
    ) async throws -> RalphCLIClient.CollectedOutput {
        try await client.runAndCollectWithRetry(
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            retryConfiguration: retryConfiguration,
            onRetry: onRetry
        )
    }

    func decodeMachineRepositoryJSON<T: Decodable & VersionedMachineDocument>(
        _ type: T.Type,
        client: RalphCLIClient,
        machineArguments: [String],
        currentDirectoryURL: URL,
        retryConfiguration: RetryConfiguration,
        onRetry: RetryProgressHandler? = nil
    ) async throws -> T {
        let value = try await decodeRepositoryJSON(
            type,
            client: client,
            arguments: ["--no-color", "machine"] + machineArguments,
            currentDirectoryURL: currentDirectoryURL,
            retryConfiguration: retryConfiguration,
            onRetry: onRetry
        )
        try RalphMachineContract.requireVersion(
            value.version,
            expected: T.expectedVersion,
            document: T.documentName,
            operation: "machine " + machineArguments.joined(separator: " ")
        )
        return value
    }

    public func updateResolvedPaths(_ paths: MachineQueuePaths) {
        identityState.resolvedPaths = paths
        queueRuntime.syncWatchTargetsIfNeeded()
    }
}
