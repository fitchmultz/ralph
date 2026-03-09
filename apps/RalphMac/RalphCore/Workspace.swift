/**
 Workspace

 Responsibilities:
 - Coordinate the domain-specific state owners that represent a single Ralph project workspace.
 - Expose the stable workspace API used by SwiftUI views and helper extensions.
 - Bridge nested state changes into a single observable object for workspace-scoped rendering.
 - Manage workspace-local runtime collaborators such as the CLI client, queue watcher, and active run.

 Does not handle:
 - Window management or tab bar UI (see WindowState).
 - Cross-workspace communication or shared app state.
 - Persisting/restoring snapshots directly (delegated to workspace persistence helpers).

 Invariants/assumptions callers must respect:
 - Each workspace has a unique ID for persistence and routing.
 - All mutations occur on the main actor.
 - Domain owners are the canonical storage boundaries; Workspace is a facade/coordinator.
 - Runtime collaborators remain workspace-local and must not leak across workspaces.
 */

public import Foundation
public import Combine
import SwiftUI

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

    public var name: String {
        get { identityState.name }
        set { identityState.name = newValue }
    }

    public var workingDirectoryURL: URL {
        get { identityState.workingDirectoryURL }
        set { identityState.workingDirectoryURL = newValue }
    }

    public var recentWorkingDirectories: [URL] {
        get { identityState.recentWorkingDirectories }
        set { identityState.recentWorkingDirectories = newValue }
    }

    public var output: String {
        get { runState.output }
        set { runState.output = newValue }
    }

    public var isRunning: Bool {
        get { runState.isRunning }
        set { runState.isRunning = newValue }
    }

    public var lastExitStatus: RalphCLIExitStatus? {
        get { runState.lastExitStatus }
        set { runState.lastExitStatus = newValue }
    }

    public var errorMessage: String? {
        get { runState.errorMessage }
        set { runState.errorMessage = newValue }
    }

    public var cliSpec: RalphCLISpecDocument? {
        get { commandState.cliSpec }
        set { commandState.cliSpec = newValue }
    }

    public var cliSpecErrorMessage: String? {
        get { commandState.cliSpecErrorMessage }
        set { commandState.cliSpecErrorMessage = newValue }
    }

    public var cliSpecIsLoading: Bool {
        get { commandState.cliSpecIsLoading }
        set { commandState.cliSpecIsLoading = newValue }
    }

    public var advancedSearchText: String {
        get { commandState.advancedSearchText }
        set { commandState.advancedSearchText = newValue }
    }

    public var advancedShowHiddenCommands: Bool {
        get { commandState.advancedShowHiddenCommands }
        set { commandState.advancedShowHiddenCommands = newValue }
    }

    public var advancedShowHiddenArgs: Bool {
        get { commandState.advancedShowHiddenArgs }
        set { commandState.advancedShowHiddenArgs = newValue }
    }

    public var advancedIncludeNoColor: Bool {
        get { commandState.advancedIncludeNoColor }
        set { commandState.advancedIncludeNoColor = newValue }
    }

    public var advancedSelectedCommandID: String? {
        get { commandState.advancedSelectedCommandID }
        set { commandState.advancedSelectedCommandID = newValue }
    }

    public var advancedBoolValues: [String: Bool] {
        get { commandState.advancedBoolValues }
        set { commandState.advancedBoolValues = newValue }
    }

    public var advancedCountValues: [String: Int] {
        get { commandState.advancedCountValues }
        set { commandState.advancedCountValues = newValue }
    }

    public var advancedSingleValues: [String: String] {
        get { commandState.advancedSingleValues }
        set { commandState.advancedSingleValues = newValue }
    }

    public var advancedMultiValues: [String: String] {
        get { commandState.advancedMultiValues }
        set { commandState.advancedMultiValues = newValue }
    }

    public var tasks: [RalphTask] {
        get { taskState.tasks }
        set { taskState.tasks = newValue }
    }

    public var tasksLoading: Bool {
        get { taskState.tasksLoading }
        set { taskState.tasksLoading = newValue }
    }

    public var tasksErrorMessage: String? {
        get { taskState.tasksErrorMessage }
        set { taskState.tasksErrorMessage = newValue }
    }

    public var lastQueueRefreshEvent: QueueRefreshEvent? {
        get { taskState.lastQueueRefreshEvent }
        set { taskState.lastQueueRefreshEvent = newValue }
    }

    public var taskFilterText: String {
        get { taskState.taskFilterText }
        set { taskState.taskFilterText = newValue }
    }

    public var taskStatusFilter: RalphTaskStatus? {
        get { taskState.taskStatusFilter }
        set { taskState.taskStatusFilter = newValue }
    }

    public var taskPriorityFilter: RalphTaskPriority? {
        get { taskState.taskPriorityFilter }
        set { taskState.taskPriorityFilter = newValue }
    }

    public var taskTagFilter: String? {
        get { taskState.taskTagFilter }
        set { taskState.taskTagFilter = newValue }
    }

    public var taskSortBy: TaskSortOption {
        get { taskState.taskSortBy }
        set { taskState.taskSortBy = newValue }
    }

    public var taskSortAscending: Bool {
        get { taskState.taskSortAscending }
        set { taskState.taskSortAscending = newValue }
    }

    public var graphData: RalphGraphDocument? {
        get { insightsState.graphData }
        set { insightsState.graphData = newValue }
    }

    public var graphDataLoading: Bool {
        get { insightsState.graphDataLoading }
        set { insightsState.graphDataLoading = newValue }
    }

    public var graphDataErrorMessage: String? {
        get { insightsState.graphDataErrorMessage }
        set { insightsState.graphDataErrorMessage = newValue }
    }

    public var analytics: AnalyticsDashboardState {
        get { insightsState.analytics }
        set { insightsState.analytics = newValue }
    }

    public var lastRecoveryError: RecoveryError? {
        get { diagnosticsState.lastRecoveryError }
        set { diagnosticsState.lastRecoveryError = newValue }
    }

    public var showErrorRecovery: Bool {
        get { diagnosticsState.showErrorRecovery }
        set { diagnosticsState.showErrorRecovery = newValue }
    }

    public var retryState: RetryState? {
        get { diagnosticsState.retryState }
        set { diagnosticsState.retryState = newValue }
    }

    public var cliHealthStatus: CLIHealthStatus? {
        get { diagnosticsState.cliHealthStatus }
        set { diagnosticsState.cliHealthStatus = newValue }
    }

    public var isCheckingHealth: Bool {
        get { diagnosticsState.isCheckingHealth }
        set { diagnosticsState.isCheckingHealth = newValue }
    }

    public var cachedTasks: [RalphTask] {
        get { diagnosticsState.cachedTasks }
        set { diagnosticsState.cachedTasks = newValue }
    }

    public var persistenceIssue: PersistenceIssue? {
        get { diagnosticsState.persistenceIssue }
        set { diagnosticsState.persistenceIssue = newValue }
    }

    public var watcherHealth: QueueWatcherHealth {
        get { diagnosticsState.watcherHealth }
        set { diagnosticsState.watcherHealth = newValue }
    }

    public var operationalIssues: [WorkspaceOperationalIssue] {
        get { diagnosticsState.operationalIssues }
        set { diagnosticsState.operationalIssues = newValue }
    }

    public var operationalSummary: WorkspaceOperationalSummary {
        get { diagnosticsState.operationalSummary }
        set { diagnosticsState.operationalSummary = newValue }
    }

    public var showOfflineBanner: Bool {
        guard let status = cliHealthStatus else { return false }
        return !status.isAvailable
    }

    public var isShowingCachedTasks: Bool {
        showOfflineBanner && !cachedTasks.isEmpty
    }

    public var showsOperationalBanner: Bool {
        !operationalSummary.isHealthy
    }

    public var currentTaskID: String? {
        get { runState.currentTaskID }
        set { runState.currentTaskID = newValue }
    }

    public var currentPhase: ExecutionPhase? {
        get { runState.currentPhase }
        set { runState.currentPhase = newValue }
    }

    public var executionStartTime: Date? {
        get { runState.executionStartTime }
        set { runState.executionStartTime = newValue }
    }

    public var isLoopMode: Bool {
        get { runState.isLoopMode }
        set { runState.isLoopMode = newValue }
    }

    public var stopAfterCurrent: Bool {
        get { runState.stopAfterCurrent }
        set { runState.stopAfterCurrent = newValue }
    }

    public var executionHistory: [ExecutionRecord] {
        get { runState.executionHistory }
        set { runState.executionHistory = newValue }
    }

    public var currentRunnerConfig: RunnerConfig? {
        get { runState.currentRunnerConfig }
        set { runState.currentRunnerConfig = newValue }
    }

    public var runnerConfigLoading: Bool {
        get { runState.runnerConfigLoading }
        set { runState.runnerConfigLoading = newValue }
    }

    public var runnerConfigErrorMessage: String? {
        get { runState.runnerConfigErrorMessage }
        set { runState.runnerConfigErrorMessage = newValue }
    }

    public var runControlSelectedTaskID: String? {
        get { runState.runControlSelectedTaskID }
        set { runState.runControlSelectedTaskID = newValue }
    }

    public var runControlForceDirtyRepo: Bool {
        get { runState.runControlForceDirtyRepo }
        set { runState.runControlForceDirtyRepo = newValue }
    }

    public var attributedOutput: [ANSISegment] {
        get { runState.attributedOutput }
        set { runState.attributedOutput = newValue }
    }

    public var outputBuffer: ConsoleOutputBuffer {
        get { runState.outputBuffer }
        set { runState.outputBuffer = newValue }
    }

    public var maxANSISegments: Int {
        get { runState.maxANSISegments }
        set { runState.maxANSISegments = newValue }
    }

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
        tasks.first { $0.status == .todo }
    }

    public var runControlTodoTasks: [RalphTask] {
        tasks.filter { $0.status == .todo }
    }

    public var selectedRunControlTask: RalphTask? {
        guard let selectedID = runControlSelectedTaskID else { return nil }
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

        let tasksByID = Dictionary(uniqueKeysWithValues: tasks.map { ($0.id, $0) })
        for dependencyID in dependsOn {
            if let dependency = tasksByID[dependencyID], dependency.status != .done {
                return true
            }
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
        guard let selectedID = runControlSelectedTaskID else { return }
        let isRunnable = runControlTodoTasks.contains { $0.id == selectedID }
        if !isRunnable {
            runControlSelectedTaskID = nil
        }
    }

    private func bindDomainStateChanges() {
        let publishers = [
            identityState.objectWillChange.eraseToAnyPublisher(),
            commandState.objectWillChange.eraseToAnyPublisher(),
            taskState.objectWillChange.eraseToAnyPublisher(),
            insightsState.objectWillChange.eraseToAnyPublisher(),
            diagnosticsState.objectWillChange.eraseToAnyPublisher(),
            runState.objectWillChange.eraseToAnyPublisher(),
        ]

        publishers.forEach { publisher in
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
