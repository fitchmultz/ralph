/**
 Workspace

 Responsibilities:
 - Represent an isolated Ralph project workspace with its own working directory,
   recent directories, console output, and execution state.
 - Define the shared workspace state and nested execution/task helper types.
 - Manage per-workspace CLI entry points and queue-derived task helpers.
 - On file-watcher-triggered refreshes, parse queue.json directly when possible
   and fall back to CLI on decode failure for low-latency UI updates.

 Does not handle:
 - Window management or tab bar UI (see WindowState).
 - Cross-workspace communication or shared state.

 Invariants/assumptions callers must respect:
 - Each workspace has a unique ID for persistence.
 - Working directory changes update the recent directories list automatically.
 - CLI client is injected and shared across workspaces (stateless design).
 - All operations must occur on the MainActor due to @Published properties.
 */

public import Foundation
public import Combine
public import SwiftUI

/// Workspace manages UI state for a Ralph project.
/// 
/// Concurrency Safety:
/// - All @Published properties must be accessed from the main thread (SwiftUI requirement)
/// - Uses @unchecked Sendable because Codable/Identifiable conformance conflicts with @MainActor
/// - This is safe because:
///   1. Workspace is always created and used on the main actor (via WorkspaceManager)
///   2. All @Published property mutations happen on main thread
///   3. Codable only accesses persisted properties (id, name, workingDirectoryURL, recentWorkingDirectories)
///   4. Identifiable only accesses id which is immutable after creation
@MainActor
public final class Workspace: ObservableObject, @preconcurrency Identifiable, @preconcurrency Codable, @unchecked Sendable {
    public var id: UUID

    @Published public var name: String
    @Published public var workingDirectoryURL: URL
    @Published public var recentWorkingDirectories: [URL]
    @Published public var output: String
    @Published public var isRunning: Bool
    @Published public var lastExitStatus: RalphCLIExitStatus?
    @Published public var errorMessage: String?

    // Advanced runner state (per workspace)
    @Published public var cliSpec: RalphCLISpecDocument?
    @Published public var cliSpecErrorMessage: String?
    @Published public var cliSpecIsLoading: Bool = false
    @Published public var advancedSearchText: String = ""
    @Published public var advancedShowHiddenCommands: Bool = false
    @Published public var advancedShowHiddenArgs: Bool = false
    @Published public var advancedIncludeNoColor: Bool = true
    @Published public var advancedSelectedCommandID: String?
    @Published public var advancedBoolValues: [String: Bool] = [:]
    @Published public var advancedCountValues: [String: Int] = [:]
    @Published public var advancedSingleValues: [String: String] = [:]
    @Published public var advancedMultiValues: [String: String] = [:]

    // Task browser state
    @Published public var tasks: [RalphTask] = []
    @Published public var tasksLoading: Bool = false
    @Published public var tasksErrorMessage: String?
    @Published public var lastQueueRefreshEvent: QueueRefreshEvent?

    // Task filtering/sorting state
    @Published public var taskFilterText: String = ""
    @Published public var taskStatusFilter: RalphTaskStatus?
    @Published public var taskPriorityFilter: RalphTaskPriority?
    @Published public var taskTagFilter: String?
    @Published public var taskSortBy: TaskSortOption = .priority
    @Published public var taskSortAscending: Bool = false
    
    // Graph data state
    @Published public var graphData: RalphGraphDocument?
    @Published public var graphDataLoading: Bool = false
    @Published public var graphDataErrorMessage: String?

    // Analytics data state
    @Published public var analyticsData: AnalyticsData = AnalyticsData()
    @Published public var analyticsLoading: Bool = false
    @Published public var analyticsErrorMessage: String?

    // MARK: - Error Recovery State
    @Published public var lastRecoveryError: RecoveryError?
    @Published public var showErrorRecovery: Bool = false
    @Published public var retryState: RetryState?

    // MARK: - Offline Mode State
    @Published public var cliHealthStatus: CLIHealthStatus?
    @Published public var isCheckingHealth: Bool = false

    /// Cached tasks for offline viewing
    @Published public var cachedTasks: [RalphTask] = []

    /// Whether to show the offline banner
    public var showOfflineBanner: Bool {
        guard let status = cliHealthStatus else { return false }
        return !status.isAvailable
    }

    /// Whether tasks are being shown from cache
    public var isShowingCachedTasks: Bool {
        showOfflineBanner && !cachedTasks.isEmpty
    }

    // MARK: - Execution State (for Run Control Panel)

    /// The ID of the currently running task (if known)
    @Published public var currentTaskID: String?

    /// Current phase of execution (1=Plan, 2=Implement, 3=Review)
    @Published public var currentPhase: ExecutionPhase?

    /// When the current execution started (for elapsed time calculation)
    @Published public var executionStartTime: Date?

    /// Whether loop mode is active (continuously run tasks)
    @Published public var isLoopMode: Bool = false

    /// Flag to stop after current task completes (graceful stop)
    @Published public var stopAfterCurrent: Bool = false

    /// History of recent execution runs
    @Published public var executionHistory: [ExecutionRecord] = []

    /// Current runner configuration (parsed from output or config)
    @Published public var currentRunnerConfig: RunnerConfig?
    @Published public var runnerConfigLoading: Bool = false
    @Published public var runnerConfigErrorMessage: String?

    /// Optional task selection for Run Control. `nil` means "auto next runnable task".
    @Published public var runControlSelectedTaskID: String?

    /// When enabled, Run Control passes `--force` to `ralph run one`.
    @Published public var runControlForceDirtyRepo: Bool = false

    /// Parsed ANSI-colored output segments for rich console display
    @Published public var attributedOutput: [ANSISegment] = []

    /// Size-limited buffer for console output to prevent memory exhaustion
    @Published public var outputBuffer: ConsoleOutputBuffer

    /// Maximum number of ANSI segments to retain (to limit attributed output memory)
    @Published public var maxANSISegments: Int = 1000 {
        didSet {
            if maxANSISegments != oldValue {
                enforceANSISegmentLimit()
            }
        }
    }

    public enum TaskSortOption: String, CaseIterable {
        case priority = "Priority"
        case created = "Created"
        case updated = "Updated"
        case status = "Status"
        case title = "Title"
    }

    // MARK: - Execution Types

    public enum ExecutionPhase: Int, CaseIterable {
        case plan = 1
        case implement = 2
        case review = 3

        public var displayName: String {
            switch self {
            case .plan: return "Plan"
            case .implement: return "Implement"
            case .review: return "Review"
            }
        }

        public var icon: String {
            switch self {
            case .plan: return "doc.text.magnifyingglass"
            case .implement: return "hammer.fill"
            case .review: return "checkmark.shield.fill"
            }
        }

        public var progressFraction: Double {
            switch self {
            case .plan: return 0.17      // 1/6
            case .implement: return 0.5  // 3/6
            case .review: return 0.83    // 5/6
            }
        }

        public var color: SwiftUI.Color {
            switch self {
            case .plan: return .blue
            case .implement: return .orange
            case .review: return .green
            }
        }
    }

    public struct ExecutionRecord: Identifiable, Codable, Sendable {
        public let id: UUID
        public let taskID: String?
        public let startTime: Date
        public let endTime: Date?
        public let exitCode: Int?
        public let wasCancelled: Bool

        public init(id: UUID = UUID(), taskID: String?, startTime: Date, endTime: Date?, exitCode: Int?, wasCancelled: Bool) {
            self.id = id
            self.taskID = taskID
            self.startTime = startTime
            self.endTime = endTime
            self.exitCode = exitCode
            self.wasCancelled = wasCancelled
        }

        public var duration: TimeInterval? {
            guard let endTime = endTime else { return nil }
            return endTime.timeIntervalSince(startTime)
        }

        public var success: Bool {
            exitCode == 0 && !wasCancelled
        }
    }

    public struct RunnerConfig: Sendable {
        public let model: String?
        public let phases: Int?
        public let maxIterations: Int?

        public init(model: String? = nil, phases: Int? = nil, maxIterations: Int? = nil) {
            self.model = model
            self.phases = phases
            self.maxIterations = maxIterations
        }
    }

    /// Container for all analytics data
    public struct AnalyticsData: Sendable {
        public var productivitySummary: ProductivitySummaryReport?
        public var velocity: ProductivityVelocityReport?
        public var burndown: BurndownReport?
        public var queueStats: QueueStatsReport?
        public var history: HistoryReport?
        
        public init() {}
    }

    public struct QueueRefreshEvent: Identifiable, Sendable, Equatable {
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

    var client: RalphCLIClient?
    private var cancellables = Set<AnyCancellable>()
    var fileWatcher: QueueFileWatcher?
    var activeRun: RalphCLIRun?
    var cancelRequested: Bool = false
    
    // Track last known task state for detecting specific changes
    @Published public var lastTasksSnapshot: [RalphTask] = []

    // MARK: - Initialization

    public init(
        id: UUID = UUID(),
        name: String? = nil,
        workingDirectoryURL: URL,
        client: RalphCLIClient? = nil
    ) {
        self.id = id
        self.workingDirectoryURL = workingDirectoryURL
        self.name = name ?? workingDirectoryURL.lastPathComponent
        self.recentWorkingDirectories = []
        self.output = ""
        self.outputBuffer = ConsoleOutputBuffer.loadFromUserDefaults()
        self.isRunning = false
        self.client = client

        loadState()

        // Persist initial state so restoration can resolve this workspace ID on next launch.
        persistState()
        
        // Start file watching after initialization
        startFileWatching()

        if client != nil {
            Task { @MainActor [weak self] in
                await self?.loadRunnerConfiguration(retryConfiguration: .minimal)
            }
        }
    }

    // MARK: - CLI Operations

    public func injectClient(_ client: RalphCLIClient) {
        self.client = client
        Task { @MainActor in
            await loadCLISpec()
            await loadRunnerConfiguration(retryConfiguration: .minimal)
        }
    }

    public func runVersion() {
        run(arguments: ["--no-color", "version"])
    }

    public func runInit() {
        run(arguments: ["--no-color", "init", "--force", "--non-interactive"])
    }

    public func runQueueListJSON() {
        run(arguments: ["--no-color", "queue", "list", "--format", "json"])
    }

    /// Returns the next task that should be worked on (first todo)
    public func nextTask() -> RalphTask? {
        tasks.first { $0.status == .todo }
    }

    /// Returns todo tasks in queue order for Run Control task selection.
    public var runControlTodoTasks: [RalphTask] {
        tasks.filter { $0.status == .todo }
    }

    /// Returns the currently selected run-control task when it is still runnable.
    public var selectedRunControlTask: RalphTask? {
        guard let selectedID = runControlSelectedTaskID else { return nil }
        return runControlTodoTasks.first { $0.id == selectedID }
    }

    /// Returns the task Run Control would execute if "Run Next Task" is pressed now.
    public var runControlPreviewTask: RalphTask? {
        selectedRunControlTask ?? nextTask()
    }

    /// Refresh queue + resolved runner configuration for Run Control panel.
    public func refreshRunControlData() async {
        await loadTasks(retryConfiguration: .minimal)
        await loadRunnerConfiguration(retryConfiguration: .minimal)
    }

    // MARK: - Task Status Helpers

    /// Check if a task is blocked by checking if any dependency is not done
    public func isTaskBlocked(_ task: RalphTask) -> Bool {
        guard let dependsOn = task.dependsOn, !dependsOn.isEmpty else {
            return false
        }

        // Build dictionary for O(1) lookups instead of O(N) linear search
        let tasksByID = Dictionary(uniqueKeysWithValues: tasks.map { ($0.id, $0) })

        // Task is blocked if any dependency is not in "done" status
        for dependencyID in dependsOn {
            if let dependency = tasksByID[dependencyID] {
                if dependency.status != .done {
                    return true
                }
            }
        }
        return false
    }

    /// Check if a task is overdue (high/critical priority todo task that's been sitting)
    public func isTaskOverdue(_ task: RalphTask) -> Bool {
        guard task.status == .todo || task.status == .draft else { return false }
        guard task.priority == .high || task.priority == .critical else { return false }

        // Consider overdue if created more than 7 days ago
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

    // MARK: - Codable

    enum CodingKeys: String, CodingKey {
        case id, name, workingDirectoryURL, recentWorkingDirectories
    }

    /// Encodes the workspace to an encoder.
    /// Note: This accesses mutable properties but is safe because Workspace
    /// is always used from the main actor.
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(workingDirectoryURL, forKey: .workingDirectoryURL)
        try container.encode(recentWorkingDirectories, forKey: .recentWorkingDirectories)
    }

    /// Required initializer for Codable conformance.
    /// Note: This is called during decoding. The workspace should only be used
    /// from the main actor after creation.
    public required init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        workingDirectoryURL = try container.decode(URL.self, forKey: .workingDirectoryURL)
        recentWorkingDirectories = try container.decode([URL].self, forKey: .recentWorkingDirectories)

        // Initialize runtime state
        output = ""
        outputBuffer = ConsoleOutputBuffer.loadFromUserDefaults()
        isRunning = false

        loadState()
    }
}
