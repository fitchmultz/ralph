/**
 Workspace

 Responsibilities:
 - Represent an isolated Ralph project workspace with its own working directory,
   recent directories, console output, and execution state.
 - Manage per-workspace CLI operations (version, init, queue list, etc.).
 - Persist workspace-specific state to UserDefaults with namespace isolation.

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
        public let phases: [String]?
        public let maxIterations: Int?

        public init(model: String? = nil, phases: [String]? = nil, maxIterations: Int? = nil) {
            self.model = model
            self.phases = phases
            self.maxIterations = maxIterations
        }
    }

    /// Represents a segment of ANSI-parsed console output
    public struct ANSISegment: Identifiable, Sendable {
        public let id = UUID()
        public let text: String
        public let color: ANSIColor
        public let isBold: Bool
        public let isItalic: Bool

        public init(text: String, color: ANSIColor = .default, isBold: Bool = false, isItalic: Bool = false) {
            self.text = text
            self.color = color
            self.isBold = isBold
            self.isItalic = isItalic
        }
    }

    public enum ANSIColor: Sendable, Hashable {
        case `default`
        case black
        case red
        case green
        case yellow
        case blue
        case magenta
        case cyan
        case white
        case brightBlack
        case brightRed
        case brightGreen
        case brightYellow
        case brightBlue
        case brightMagenta
        case brightCyan
        case brightWhite
        // Extended colors
        case indexed(UInt8)              // 256-color palette (0-255)
        case rgb(UInt8, UInt8, UInt8)    // True color (24-bit)

        public var swiftUIColor: SwiftUI.Color {
            switch self {
            case .default: return .primary
            case .black: return .black
            case .red: return .red
            case .green: return .green
            case .yellow: return .yellow
            case .blue: return .blue
            case .magenta: return .purple
            case .cyan: return .cyan
            case .white: return .white
            case .brightBlack: return .gray
            case .brightRed: return .red.opacity(0.8)
            case .brightGreen: return .green.opacity(0.8)
            case .brightYellow: return .yellow.opacity(0.8)
            case .brightBlue: return .blue.opacity(0.8)
            case .brightMagenta: return .purple.opacity(0.8)
            case .brightCyan: return .cyan.opacity(0.8)
            case .brightWhite: return .white.opacity(0.9)
            case .indexed(let index):
                return Self.colorFrom256(index)
            case .rgb(let r, let g, let b):
                return Color(red: Double(r)/255, green: Double(g)/255, blue: Double(b)/255)
            }
        }

        /// Convert 256-color index to SwiftUI Color
        private static func colorFrom256(_ index: UInt8) -> Color {
            // 0-15: Standard colors (same as 16-color palette)
            // 16-231: 6x6x6 RGB cube
            // 232-255: Grayscale ramp

            if index < 16 {
                // Map to standard colors
                let colors: [ANSIColor] = [
                    .black, .red, .green, .yellow, .blue, .magenta, .cyan, .white,
                    .brightBlack, .brightRed, .brightGreen, .brightYellow,
                    .brightBlue, .brightMagenta, .brightCyan, .brightWhite
                ]
                return colors[Int(index)].swiftUIColor
            } else if index < 232 {
                // RGB cube: 16 + 36*r + 6*g + b where r,g,b in 0-5
                let i = Int(index) - 16
                let r = i / 36
                let g = (i % 36) / 6
                let b = i % 6
                // Map 0-5 to 0-255
                let rf = Double(r == 0 ? 0 : r * 40 + 55) / 255
                let gf = Double(g == 0 ? 0 : g * 40 + 55) / 255
                let bf = Double(b == 0 ? 0 : b * 40 + 55) / 255
                return Color(red: rf, green: gf, blue: bf)
            } else {
                // Grayscale: 232-255 maps to 8-238 (step 10)
                let gray = 8 + (Int(index) - 232) * 10
                let gf = Double(gray) / 255
                return Color(red: gf, green: gf, blue: gf)
            }
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

    private var client: RalphCLIClient?
    private var cancellables = Set<AnyCancellable>()
    private var fileWatcher: QueueFileWatcher?
    
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
        
        // Start file watching after initialization
        startFileWatching()
    }

    // MARK: - Persistence Keys

    private func defaultsKey(_ suffix: String) -> String {
        "com.mitchfultz.ralph.workspace.\(id.uuidString).\(suffix)"
    }

    // MARK: - State Persistence

    private func loadState() {
        let defaults = UserDefaults.standard

        // Load recent directories
        if let stored = defaults.array(forKey: defaultsKey("recentPaths")) as? [String] {
            recentWorkingDirectories = stored
                .map { URL(fileURLWithPath: $0, isDirectory: true) }
                .filter { url in
                    var isDir: ObjCBool = false
                    return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
                }
        }

        // Load working directory if valid
        if let stored = defaults.string(forKey: defaultsKey("workingPath")) {
            let url = URL(fileURLWithPath: stored, isDirectory: true)
            if FileManager.default.fileExists(atPath: url.path) {
                workingDirectoryURL = url
            }
        }

        // Load name override if present
        if let storedName = defaults.string(forKey: defaultsKey("name")) {
            name = storedName
        }
    }

    private func persistState() {
        let defaults = UserDefaults.standard
        defaults.set(workingDirectoryURL.path, forKey: defaultsKey("workingPath"))
        defaults.set(recentWorkingDirectories.map(\.path), forKey: defaultsKey("recentPaths"))
        defaults.set(name, forKey: defaultsKey("name"))
    }

    // MARK: - Working Directory Management

    public func setWorkingDirectory(_ url: URL) {
        workingDirectoryURL = url
        name = url.lastPathComponent

        // Update recents
        var newRecents = recentWorkingDirectories.filter { $0.path != url.path }
        newRecents.insert(url, at: 0)
        if newRecents.count > 12 {
            newRecents = Array(newRecents.prefix(12))
        }
        recentWorkingDirectories = newRecents

        persistState()
        
        // Restart file watching for new directory
        startFileWatching()
        
        // Clear last snapshot
        lastTasksSnapshot.removeAll()
    }

    public func chooseWorkingDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.prompt = "Choose"

        if panel.runModal() == .OK, let url = panel.url {
            setWorkingDirectory(url)
        }
    }

    public func selectRecentWorkingDirectory(_ url: URL) {
        setWorkingDirectory(url)
    }

    // MARK: - CLI Operations

    public func injectClient(_ client: RalphCLIClient) {
        self.client = client
        Task {
            await loadCLISpec()
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

    public func loadTasks(retryConfiguration: RetryConfiguration = .default) async {
        guard let client else {
            tasksErrorMessage = "CLI client not available."
            return
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
                    // Check for process failures
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, delay in
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

            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let document = try decoder.decode(RalphTaskQueueDocument.self, from: data)
            tasks = document.tasks
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
    
    // MARK: - File Watching

    /// Start watching queue files for external changes.
    private func startFileWatching() {
        // Stop existing watcher
        fileWatcher?.stop()
        
        // Create and configure new watcher
        let watcher = QueueFileWatcher(workingDirectoryURL: workingDirectoryURL)
        watcher.onFileChanged = { [weak self] in
            Task { [weak self] in
                await self?.handleExternalFileChange()
            }
        }
        watcher.start()
        fileWatcher = watcher
    }
    
    /// Stop file watching (call when workspace is being deallocated).
    public func stopFileWatching() {
        fileWatcher?.stop()
        fileWatcher = nil
    }
    
    /// Handle external file changes by reloading tasks
    private func handleExternalFileChange() async {
        // Store current tasks for comparison
        lastTasksSnapshot = tasks
        
        // Reload tasks
        await loadTasks()
        
        // Post notification for UI to animate changes
        NotificationCenter.default.post(
            name: .queueFilesExternallyChanged,
            object: self,
            userInfo: [
                "previousTasks": lastTasksSnapshot,
                "currentTasks": tasks
            ]
        )
    }
    
    /// Compare two task arrays to detect what changed
    public func detectTaskChanges(previous: [RalphTask], current: [RalphTask]) -> TaskChanges {
        let previousIDs = Set(previous.map { $0.id })
        let currentIDs = Set(current.map { $0.id })
        
        let added = current.filter { !previousIDs.contains($0.id) }
        let removed = previous.filter { !currentIDs.contains($0.id) }
        
        // Build dictionary for O(1) lookups instead of O(N) linear search
        let previousByID = Dictionary(uniqueKeysWithValues: previous.map { ($0.id, $0) })
        
        var changed: [RalphTask] = []
        for task in current {
            if let previousTask = previousByID[task.id] {
                if task.status != previousTask.status ||
                   task.title != previousTask.title ||
                   task.priority != previousTask.priority ||
                   task.tags != previousTask.tags {
                    changed.append(task)
                }
            }
        }
        
        return TaskChanges(added: added, removed: removed, changed: changed)
    }
    
    /// Check if a task has been modified externally by comparing the stored updatedAt
    /// with the current updatedAt in the loaded tasks.
    /// Returns the current task if conflict detected, nil if no conflict.
    public func checkForConflict(taskID: String, originalUpdatedAt: Date?) -> RalphTask? {
        guard let currentTask = tasks.first(where: { $0.id == taskID }) else {
            // Task was deleted - treat as conflict
            return nil
        }
        
        // If we have no original timestamp, we can't detect conflicts
        guard let originalUpdatedAt = originalUpdatedAt else {
            return nil
        }
        
        // If current updatedAt differs from original, there's a conflict
        if let currentUpdatedAt = currentTask.updatedAt {
            if currentUpdatedAt != originalUpdatedAt {
                return currentTask
            }
        }
        
        return nil
    }

    /// Represents a conflict between local and external task state
    public struct TaskConflict: Sendable {
        public let localTask: RalphTask
        public let externalTask: RalphTask
        public let conflictedFields: [String]
        
        public init(localTask: RalphTask, externalTask: RalphTask, conflictedFields: [String]) {
            self.localTask = localTask
            self.externalTask = externalTask
            self.conflictedFields = conflictedFields
        }
    }

    /// Detect specific field differences between local and external task
    public func detectConflictedFields(local: RalphTask, external: RalphTask) -> [String] {
        var fields: [String] = []
        
        if local.title != external.title { fields.append("title") }
        if local.description != external.description { fields.append("description") }
        if local.status != external.status { fields.append("status") }
        if local.priority != external.priority { fields.append("priority") }
        if local.tags != external.tags { fields.append("tags") }
        if local.scope != external.scope { fields.append("scope") }
        if local.evidence != external.evidence { fields.append("evidence") }
        if local.plan != external.plan { fields.append("plan") }
        if local.notes != external.notes { fields.append("notes") }
        if local.dependsOn != external.dependsOn { fields.append("dependsOn") }
        if local.blocks != external.blocks { fields.append("blocks") }
        if local.relatesTo != external.relatesTo { fields.append("relatesTo") }
        
        return fields
    }
    
    /// Represents detected changes between two task snapshots
    public struct TaskChanges: Sendable {
        public let added: [RalphTask]
        public let removed: [RalphTask]
        public let changed: [RalphTask]
        
        public var hasChanges: Bool {
            !added.isEmpty || !removed.isEmpty || !changed.isEmpty
        }
    }

    // MARK: - Graph Data Loading

    /// Load graph data from CLI for dependency visualization
    public func loadGraphData(retryConfiguration: RetryConfiguration = .default) async {
        guard let client else {
            graphDataErrorMessage = "CLI client not available."
            return
        }

        graphDataLoading = true
        graphDataErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "graph", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.graphDataErrorMessage = "Retrying load graph (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            guard collected.status.code == 0 else {
                graphDataErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load graph data (exit \(collected.status.code))."
                    : collected.stderr
                graphDataLoading = false
                return
            }

            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            let document = try decoder.decode(RalphGraphDocument.self, from: data)
            graphData = document
        } catch {
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadGraphData",
                workspaceURL: workingDirectoryURL
            )
            graphDataErrorMessage = recoveryError.message
            lastRecoveryError = recoveryError
            showErrorRecovery = true
        }

        graphDataLoading = false
    }

    /// Returns filtered and sorted tasks based on current filter/sort state
    public func filteredAndSortedTasks() -> [RalphTask] {
        var result = tasks

        // Apply text filter (search in title, description, tags)
        let filterText = taskFilterText.trimmingCharacters(in: .whitespacesAndNewlines)
        if !filterText.isEmpty {
            result = result.filter { task in
                let matchesTitle = task.title.localizedCaseInsensitiveContains(filterText)
                let matchesDescription = task.description?.localizedCaseInsensitiveContains(filterText) ?? false
                let matchesTags = task.tags.contains { $0.localizedCaseInsensitiveContains(filterText) }
                return matchesTitle || matchesDescription || matchesTags
            }
        }

        // Apply status filter
        if let statusFilter = taskStatusFilter {
            result = result.filter { $0.status == statusFilter }
        }

        // Apply priority filter
        if let priorityFilter = taskPriorityFilter {
            result = result.filter { $0.priority == priorityFilter }
        }

        // Apply tag filter
        if let tagFilter = taskTagFilter, !tagFilter.isEmpty {
            result = result.filter { $0.tags.contains(tagFilter) }
        }

        // Apply sorting
        result.sort { a, b in
            let comparison: Bool
            switch taskSortBy {
            case .priority:
                comparison = a.priority.sortOrder < b.priority.sortOrder
            case .created:
                comparison = (a.createdAt ?? .distantPast) < (b.createdAt ?? .distantPast)
            case .updated:
                comparison = (a.updatedAt ?? .distantPast) < (b.updatedAt ?? .distantPast)
            case .status:
                comparison = a.status.rawValue < b.status.rawValue
            case .title:
                comparison = a.title.localizedCompare(b.title) == .orderedAscending
            }
            return taskSortAscending ? comparison : !comparison
        }

        return result
    }

    /// Returns the next task that should be worked on (first todo)
    public func nextTask() -> RalphTask? {
        tasks.first { $0.status == .todo }
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

    /// Update task status via CLI, optionally setting startedAt when moving to "doing"
    public func updateTaskStatus(taskID: String, to newStatus: RalphTaskStatus) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        
        // Build arguments for status change
        let arguments = ["--no-color", "task", "edit", "status", newStatus.rawValue, taskID]

        let collected = try await helper.execute(
            operation: { [self] in
                let result = try await client.runAndCollect(
                    arguments: arguments,
                    currentDirectoryURL: workingDirectoryURL
                )
                if result.status.code != 0 {
                    throw result.toError()
                }
                return result
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    self?.errorMessage = "Retrying status update (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(
                "Failed to update status: \(collected.stderr.isEmpty ? "Exit \(collected.status.code)" : collected.stderr)"
            )
        }

        // If moving to "doing", also set startedAt timestamp
        if newStatus == .doing {
            let dateFormatter = ISO8601DateFormatter()
            let startedAt = dateFormatter.string(from: Date())
            _ = try? await helper.execute(
                operation: { [self] in
                    try await client.runAndCollect(
                        arguments: ["--no-color", "task", "edit", "started_at", startedAt, taskID],
                        currentDirectoryURL: workingDirectoryURL
                    )
                }
            )
        }

        // Reload tasks to get updated state
        await loadTasks()
    }

    // MARK: - Task Updates

    /// Update a task by applying changes via the CLI and reloading the task list.
    /// This compares the original task with the updated task and generates appropriate CLI commands.
    /// - Parameters:
    ///   - original: The original task before any edits
    ///   - updated: The updated task with edits applied
    ///   - originalUpdatedAt: The updatedAt timestamp at the time editing began (for optimistic locking)
    /// - Throws: WorkspaceError.taskConflict if the task has been modified externally
    public func updateTask(from original: RalphTask, to updated: RalphTask, originalUpdatedAt: Date? = nil) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }
        
        // Check for external changes if originalUpdatedAt is provided (optimistic locking)
        if let originalUpdatedAt = originalUpdatedAt {
            if let currentTask = tasks.first(where: { $0.id == updated.id }),
               let currentUpdatedAt = currentTask.updatedAt,
               currentUpdatedAt != originalUpdatedAt {
                throw WorkspaceError.taskConflict(currentTask)
            }
        }

        // Build list of field changes needed
        var editCommands: [(field: String, value: String)] = []

        // Title
        if original.title != updated.title {
            editCommands.append(("title", updated.title))
        }

        // Description (empty string becomes nil)
        let originalDesc = original.description ?? ""
        let updatedDesc = updated.description ?? ""
        if originalDesc != updatedDesc {
            editCommands.append(("description", updatedDesc))
        }

        // Status
        if original.status != updated.status {
            editCommands.append(("status", updated.status.rawValue))
        }

        // Priority
        if original.priority != updated.priority {
            editCommands.append(("priority", updated.priority.rawValue))
        }

        // Tags (comma-separated)
        if original.tags != updated.tags {
            let value = updated.tags.joined(separator: ", ")
            editCommands.append(("tags", value))
        }

        // Scope
        let originalScope = original.scope ?? []
        let updatedScope = updated.scope ?? []
        if originalScope != updatedScope {
            let value = updatedScope.joined(separator: "\n")
            editCommands.append(("scope", value))
        }

        // Evidence
        let originalEvidence = original.evidence ?? []
        let updatedEvidence = updated.evidence ?? []
        if originalEvidence != updatedEvidence {
            let value = updatedEvidence.joined(separator: "\n")
            editCommands.append(("evidence", value))
        }

        // Plan
        let originalPlan = original.plan ?? []
        let updatedPlan = updated.plan ?? []
        if originalPlan != updatedPlan {
            let value = updatedPlan.joined(separator: "\n")
            editCommands.append(("plan", value))
        }

        // Notes
        let originalNotes = original.notes ?? []
        let updatedNotes = updated.notes ?? []
        if originalNotes != updatedNotes {
            let value = updatedNotes.joined(separator: "\n")
            editCommands.append(("notes", value))
        }

        // Relationships
        let originalDepends = original.dependsOn ?? []
        let updatedDepends = updated.dependsOn ?? []
        if originalDepends != updatedDepends {
            let value = updatedDepends.joined(separator: ", ")
            editCommands.append(("depends_on", value))
        }

        let originalBlocks = original.blocks ?? []
        let updatedBlocks = updated.blocks ?? []
        if originalBlocks != updatedBlocks {
            let value = updatedBlocks.joined(separator: ", ")
            editCommands.append(("blocks", value))
        }

        let originalRelates = original.relatesTo ?? []
        let updatedRelates = updated.relatesTo ?? []
        if originalRelates != updatedRelates {
            let value = updatedRelates.joined(separator: ", ")
            editCommands.append(("relates_to", value))
        }

        // Execute each edit command with retry
        let helper = RetryHelper(configuration: .default)
        
        for (field, value) in editCommands {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "task", "edit", field, value, updated.id],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.errorMessage = "Retrying edit \(field) (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            guard collected.status.code == 0 else {
                throw WorkspaceError.cliError(
                    "Failed to edit \(field): \(collected.stderr.isEmpty ? "Exit \(collected.status.code)" : collected.stderr)"
                )
            }
        }

        // Reload tasks to get updated state
        await loadTasks()
    }

    // MARK: - Task Creation

    /// Create a new task using the CLI task build command with optional template.
    /// This runs `ralph task build` or `ralph task template build` depending on parameters.
    public func createTask(
        title: String,
        description: String? = nil,
        priority: RalphTaskPriority,
        tags: [String] = [],
        scope: [String]? = nil,
        template: String? = nil,
        target: String? = nil
    ) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        // Build the request string from title and description
        var request = title
        if let description = description, !description.isEmpty {
            request += "\n\n\(description)"
        }

        // Build arguments based on whether we're using a template
        var arguments: [String] = ["--no-color"]

        if let template = template {
            // Use template build command: ralph task template build <template> [target] <request>
            arguments.append(contentsOf: ["task", "template", "build", template])

            // Add target if provided (for templates with variables)
            if let target = target, !target.isEmpty {
                arguments.append(target)
            }

            // Add request as positional argument
            arguments.append(request)
        } else {
            // Use regular task build command: ralph task build <request>
            arguments.append(contentsOf: ["task", "build"])

            // Add request
            arguments.append(request)
        }

        let helper = RetryHelper(configuration: .default)
        
        let collected = try await helper.execute(
            operation: { [self, arguments] in
                let result = try await client.runAndCollect(
                    arguments: arguments,
                    currentDirectoryURL: workingDirectoryURL
                )
                if result.status.code != 0 {
                    throw result.toError()
                }
                return result
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    self?.errorMessage = "Retrying create task (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(
                collected.stderr.isEmpty ? "Failed to create task (exit \(collected.status.code))" : collected.stderr
            )
        }

        // Reload tasks to get the newly created task
        await loadTasks()
    }

    // MARK: - Errors

    public enum WorkspaceError: Error, LocalizedError {
        case cliClientUnavailable
        case cliError(String)
        case taskConflict(RalphTask)  // NEW: Contains the current external task
        
        public var errorDescription: String? {
            switch self {
            case .cliClientUnavailable:
                return "CLI client is not available."
            case .cliError(let message):
                return message
            case .taskConflict:  // NEW
                return "Task has been modified externally. Please resolve the conflict before saving."
            }
        }
    }

    public func run(arguments: [String]) {
        guard let client else {
            errorMessage = "CLI client not available."
            return
        }
        guard !isRunning else { return }

        output = ""
        outputBuffer.clear()
        attributedOutput = []
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true
        executionStartTime = Date()

        Task {
            do {
                let collected = try await client.runAndCollect(
                    arguments: arguments,
                    currentDirectoryURL: workingDirectoryURL
                )

                // Update output with size limiting via outputBuffer
                outputBuffer.setContent(collected.stdout)
                if !collected.stderr.isEmpty {
                    outputBuffer.append("\n[stderr] " + collected.stderr)
                }

                // Keep legacy output property in sync for backwards compatibility
                output = outputBuffer.content

                // Parse phase information from output
                detectPhase(from: output)

                // Parse ANSI codes for rich display (with segment limiting)
                parseANSICodes(from: output)
                enforceANSISegmentLimit()

                lastExitStatus = collected.status
                isRunning = false

                // Record execution history
                if let startTime = executionStartTime {
                    let record = ExecutionRecord(
                        id: UUID(),
                        taskID: currentTaskID,
                        startTime: startTime,
                        endTime: Date(),
                        exitCode: Int(collected.status.code),
                        wasCancelled: false
                    )
                    addToHistory(record)
                }

                // Handle loop mode - run next task if enabled and not stopping
                if isLoopMode && !stopAfterCurrent && collected.status.code == 0 {
                    // Small delay before next task
                    try? await Task.sleep(nanoseconds: 1_000_000_000) // 1 second
                    if isLoopMode && !stopAfterCurrent {
                        runNextTask()
                        return  // Don't reset state, we're continuing
                    }
                }

                resetExecutionState()
            } catch {
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "run",
                    workspaceURL: workingDirectoryURL
                )
                errorMessage = recoveryError.message
                lastRecoveryError = recoveryError
                showErrorRecovery = true
                isRunning = false
                resetExecutionState()
            }
        }
    }

    public func cancel() {
        // Note: With runAndCollect, we can't easily cancel mid-execution.
        // The task will complete but loop mode can be stopped.

        // Record cancelled execution
        if let startTime = executionStartTime {
            let record = ExecutionRecord(
                id: UUID(),
                taskID: currentTaskID,
                startTime: startTime,
                endTime: Date(),
                exitCode: nil,
                wasCancelled: true
            )
            addToHistory(record)
        }

        isLoopMode = false
        stopAfterCurrent = true
        isRunning = false
        resetExecutionState()
    }

    // MARK: - Execution Control

    /// Run the next task in the queue (ralph run one)
    public func runNextTask() {
        // Reset execution state
        resetExecutionState()

        // Get the next task before starting
        if let next = nextTask() {
            currentTaskID = next.id
        }

        executionStartTime = Date()
        run(arguments: ["--no-color", "run", "one"])
    }

    /// Start loop mode (continuously run tasks)
    public func startLoop() {
        isLoopMode = true
        stopAfterCurrent = false
        runNextTask()
    }

    /// Stop loop mode (finish current task then stop)
    public func stopLoop() {
        isLoopMode = false
        stopAfterCurrent = true
    }

    /// Reset execution state after completion or cancellation
    private func resetExecutionState() {
        currentPhase = nil
        executionStartTime = nil
        currentTaskID = nil
        attributedOutput = []
        // Note: outputBuffer is intentionally preserved for inspection after completion
    }

    /// Add execution record to history (keeps last 50)
    private func addToHistory(_ record: ExecutionRecord) {
        executionHistory.insert(record, at: 0)
        if executionHistory.count > 50 {
            executionHistory = Array(executionHistory.prefix(50))
        }
    }

    /// Parse phase information from CLI output
    public func detectPhase(from output: String) {
        // Look for phase indicators in output
        if output.contains("PHASE 1") || output.contains("Phase 1") || 
           output.contains("PLANNING") || output.contains("Planning") ||
           output.contains("# Phase 1") || output.contains("## Phase 1") {
            currentPhase = .plan
        } else if output.contains("PHASE 2") || output.contains("Phase 2") || 
                  output.contains("IMPLEMENTING") || output.contains("Implementing") ||
                  output.contains("IMPLEMENTATION") || output.contains("# Phase 2") || 
                  output.contains("## Phase 2") {
            currentPhase = .implement
        } else if output.contains("PHASE 3") || output.contains("Phase 3") || 
                  output.contains("REVIEWING") || output.contains("Reviewing") ||
                  output.contains("REVIEW") || output.contains("# Phase 3") || 
                  output.contains("## Phase 3") {
            currentPhase = .review
        }
    }

    // MARK: - ANSI Parsing

    /// Tracks current ANSI styling state during parsing
    private struct ANSIStyleState {
        var foregroundColor: ANSIColor = .default
        var backgroundColor: ANSIColor = .default
        var isBold: Bool = false
        var isItalic: Bool = false
        var isDim: Bool = false
        var isUnderline: Bool = false

        mutating func reset() {
            foregroundColor = .default
            backgroundColor = .default
            isBold = false
            isItalic = false
            isDim = false
            isUnderline = false
        }

        mutating func applySGR(_ code: Int) {
            switch code {
            case 0: // Reset
                reset()
            case 1: // Bold
                isBold = true
            case 2: // Dim/Faint
                isDim = true
            case 3: // Italic
                isItalic = true
            case 4: // Underline
                isUnderline = true
            case 22: // Normal intensity (not bold, not dim)
                isBold = false
                isDim = false
            case 23: // Not italic
                isItalic = false
            case 24: // Not underlined
                isUnderline = false
            case 30...37: // Foreground colors (standard)
                foregroundColor = colorFromCode(code)
            case 38: // Extended foreground color (handled separately)
                break
            case 39: // Default foreground
                foregroundColor = .default
            case 40...47: // Background colors
                backgroundColor = colorFromCode(code - 10)  // Map to foreground equivalent
            case 48: // Extended background color
                break
            case 49: // Default background
                backgroundColor = .default
            case 90...97: // Bright foreground colors
                foregroundColor = colorFromCode(code)
            case 100...107: // Bright background colors
                backgroundColor = colorFromCode(code - 10)
            default:
                break
            }
        }

        private func colorFromCode(_ code: Int) -> ANSIColor {
            switch code {
            case 30, 40: return .black
            case 31, 41: return .red
            case 32, 42: return .green
            case 33, 43: return .yellow
            case 34, 44: return .blue
            case 35, 45: return .magenta
            case 36, 46: return .cyan
            case 37, 47: return .white
            case 90, 100: return .brightBlack
            case 91, 101: return .brightRed
            case 92, 102: return .brightGreen
            case 93, 103: return .brightYellow
            case 94, 104: return .brightBlue
            case 95, 105: return .brightMagenta
            case 96, 106: return .brightCyan
            case 97, 107: return .brightWhite
            default: return .default
            }
        }
    }

    /// Parse ANSI codes from raw output and update attributedOutput
    ///
    /// Supports SGR codes for colors (16-color, 256-color, true color),
    /// text attributes (bold, italic), and strips cursor movement codes.
    public func parseANSICodes(from rawOutput: String) {
        var segments: [ANSISegment] = []
        var currentState = ANSIStyleState()
        var currentText = ""
        var index = rawOutput.startIndex

        while index < rawOutput.endIndex {
            // Look for escape character
            if rawOutput[index] == "\u{001B}",
               index < rawOutput.index(before: rawOutput.endIndex),
               rawOutput[rawOutput.index(after: index)] == "[" {
                // Found potential CSI sequence
                let afterBracket = rawOutput.index(index, offsetBy: 2)

                // Parse the command string
                var commandEnd = afterBracket
                var commandChars = ""

                while commandEnd < rawOutput.endIndex {
                    let char = rawOutput[commandEnd]
                    // SGR command ends with 'm'
                    // Cursor commands end with A-Z, a-z (except [)
                    if (char >= "A" && char <= "Z") || (char >= "a" && char <= "z" && char != "[") {
                        // Check if it's an SGR sequence (ends with 'm')
                        if char == "m" {
                            // Process SGR sequence
                            if !currentText.isEmpty {
                                segments.append(ANSISegment(
                                    text: currentText,
                                    color: currentState.foregroundColor,
                                    isBold: currentState.isBold,
                                    isItalic: currentState.isItalic
                                ))
                                currentText = ""
                            }

                            // Parse SGR parameters
                            if commandChars.isEmpty {
                                // Empty sequence is reset
                                currentState.reset()
                            } else {
                                var params = commandChars.split(separator: ";").compactMap { Int($0) }
                                if params.isEmpty {
                                    params = [0]  // Reset if no valid params
                                }

                                var i = 0
                                while i < params.count {
                                    let code = params[i]

                                    if code == 38 && i + 1 < params.count {
                                        // Extended foreground color
                                        let subCode = params[i + 1]
                                        if subCode == 5 && i + 2 < params.count {
                                            // 256-color
                                            currentState.foregroundColor = .indexed(UInt8(params[i + 2]))
                                            i += 3
                                        } else if subCode == 2 && i + 4 < params.count {
                                            // True color RGB
                                            currentState.foregroundColor = .rgb(
                                                UInt8(max(0, min(255, params[i + 2]))),
                                                UInt8(max(0, min(255, params[i + 3]))),
                                                UInt8(max(0, min(255, params[i + 4])))
                                            )
                                            i += 5
                                        } else {
                                            i += 1
                                        }
                                    } else if code == 48 && i + 1 < params.count {
                                        // Extended background color - parse but don't apply yet
                                        let subCode = params[i + 1]
                                        if subCode == 5 && i + 2 < params.count {
                                            currentState.backgroundColor = .indexed(UInt8(params[i + 2]))
                                            i += 3
                                        } else if subCode == 2 && i + 4 < params.count {
                                            currentState.backgroundColor = .rgb(
                                                UInt8(max(0, min(255, params[i + 2]))),
                                                UInt8(max(0, min(255, params[i + 3]))),
                                                UInt8(max(0, min(255, params[i + 4])))
                                            )
                                            i += 5
                                        } else {
                                            i += 1
                                        }
                                    } else {
                                        currentState.applySGR(code)
                                        i += 1
                                    }
                                }
                            }
                        }
                        // Non-SGR CSI sequence (cursor movement, etc.) - just skip it

                        // Move past this sequence
                        index = rawOutput.index(after: commandEnd)
                        break
                    } else if char == "[" {
                        // Nested CSI - shouldn't happen, treat as end
                        index = afterBracket
                        break
                    } else {
                        commandChars.append(char)
                        commandEnd = rawOutput.index(after: commandEnd)
                    }
                }

                // If we didn't find a terminator, treat escape as literal
                if commandEnd >= rawOutput.endIndex {
                    currentText.append(rawOutput[index])
                    index = rawOutput.index(after: index)
                }
            } else {
                currentText.append(rawOutput[index])
                index = rawOutput.index(after: index)
            }
        }

        // Don't forget the final segment
        if !currentText.isEmpty {
            segments.append(ANSISegment(
                text: currentText,
                color: currentState.foregroundColor,
                isBold: currentState.isBold,
                isItalic: currentState.isItalic
            ))
        }

        // Merge with existing segments or replace
        if attributedOutput.isEmpty {
            attributedOutput = segments
        } else {
            // Append new segments, merging with last if styles match
            for segment in segments {
                if let last = attributedOutput.last,
                   last.color == segment.color,
                   last.isBold == segment.isBold,
                   last.isItalic == segment.isItalic {
                    // Merge with previous segment
                    let mergedText = last.text + segment.text
                    attributedOutput[attributedOutput.count - 1] = ANSISegment(
                        text: mergedText,
                        color: segment.color,
                        isBold: segment.isBold,
                        isItalic: segment.isItalic
                    )
                } else {
                    attributedOutput.append(segment)
                }
            }
        }

        // Optimization: Merge adjacent segments with identical styling
        attributedOutput = mergeAdjacentSegments(attributedOutput)
    }

    /// Merge adjacent segments with identical styling to reduce segment count
    private func mergeAdjacentSegments(_ segments: [ANSISegment]) -> [ANSISegment] {
        guard segments.count > 1 else { return segments }

        var merged: [ANSISegment] = []

        for segment in segments {
            if let last = merged.last,
               last.color == segment.color,
               last.isBold == segment.isBold,
               last.isItalic == segment.isItalic {
                // Merge with last
                merged[merged.count - 1] = ANSISegment(
                    text: last.text + segment.text,
                    color: segment.color,
                    isBold: segment.isBold,
                    isItalic: segment.isItalic
                )
            } else {
                merged.append(segment)
            }
        }

        return merged
    }

    /// Enforce maximum number of ANSI segments to limit memory usage.
    /// Keeps the most recent segments and prepends an indicator when truncated.
    public func enforceANSISegmentLimit() {
        guard attributedOutput.count > maxANSISegments else { return }

        // Keep the most recent segments (trailing end of output)
        attributedOutput = Array(attributedOutput.suffix(maxANSISegments))

        // Prepend a default-colored indicator segment if not already present
        let indicatorText = "\n... [console output truncated due to length] ...\n"
        if !attributedOutput.isEmpty && attributedOutput[0].text != indicatorText {
            let indicator = ANSISegment(
                text: indicatorText,
                color: .yellow,
                isBold: false,
                isItalic: true
            )
            attributedOutput.insert(indicator, at: 0)
        }
    }

    // MARK: - CLI Spec Loading

    public func loadCLISpec(retryConfiguration: RetryConfiguration = .minimal) async {
        guard let client else {
            cliSpecErrorMessage = "CLI client not available."
            return
        }

        cliSpecIsLoading = true
        cliSpecErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "__cli-spec", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            guard collected.status.code == 0 else {
                cliSpec = nil
                cliSpecErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load CLI spec (exit \(collected.status.code))."
                    : collected.stderr
                cliSpecIsLoading = false
                return
            }

            let data = Data(collected.stdout.utf8)
            let decoded = try JSONDecoder().decode(RalphCLISpecDocument.self, from: data)
            cliSpec = decoded
        } catch {
            cliSpec = nil
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadCLISpec",
                workspaceURL: workingDirectoryURL
            )
            cliSpecErrorMessage = recoveryError.message
            lastRecoveryError = recoveryError
            showErrorRecovery = true
        }

        cliSpecIsLoading = false
    }

    // MARK: - Advanced Command Helpers

    public func advancedCommands() -> [RalphCLICommandSpec] {
        guard let cliSpec else { return [] }
        var out: [RalphCLICommandSpec] = []
        for sub in cliSpec.root.subcommands {
            collectCommands(sub, includeHidden: advancedShowHiddenCommands, into: &out)
        }
        return out
    }

    public func selectedAdvancedCommand() -> RalphCLICommandSpec? {
        guard let id = advancedSelectedCommandID else { return nil }
        return advancedCommands().first(where: { $0.id == id })
    }

    public func resetAdvancedInputs() {
        advancedBoolValues.removeAll(keepingCapacity: false)
        advancedCountValues.removeAll(keepingCapacity: false)
        advancedSingleValues.removeAll(keepingCapacity: false)
        advancedMultiValues.removeAll(keepingCapacity: false)
    }

    public func buildAdvancedArguments() -> [String] {
        guard let cmd = selectedAdvancedCommand() else { return [] }
        var selections: [String: RalphCLIArgValue] = [:]

        for arg in cmd.args {
            if arg.positional {
                let raw = advancedMultiValues[arg.id] ?? ""
                let values = raw.split(whereSeparator: \.isNewline)
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
                continue
            }

            if arg.isCountFlag {
                let n = advancedCountValues[arg.id] ?? 0
                if n > 0 {
                    selections[arg.id] = .count(n)
                }
                continue
            }

            if arg.isBooleanFlag {
                let present = advancedBoolValues[arg.id] ?? false
                selections[arg.id] = .flag(present)
                continue
            }

            if arg.takesValue {
                let raw = advancedMultiValues[arg.id] ?? ""
                let values = raw.split(whereSeparator: \.isNewline)
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
            }
        }

        var globals: [String] = []
        if advancedIncludeNoColor {
            globals.append("--no-color")
        }
        return RalphCLIArgumentBuilder.buildArguments(
            command: cmd,
            selections: selections,
            globalArguments: globals
        )
    }

    private func collectCommands(
        _ command: RalphCLICommandSpec,
        includeHidden: Bool,
        into out: inout [RalphCLICommandSpec]
    ) {
        if includeHidden || !command.hidden {
            out.append(command)
        }
        for sub in command.subcommands {
            collectCommands(sub, includeHidden: includeHidden, into: &out)
        }
    }

    // MARK: - Analytics Data Loading

    /// Load all analytics data for the dashboard
    public func loadAnalytics(timeRange: TimeRange = .sevenDays) async {
        guard let client else {
            analyticsErrorMessage = "CLI client not available."
            return
        }
        
        analyticsLoading = true
        analyticsErrorMessage = nil
        
        // Capture days value to avoid data race warnings with async let
        let days = timeRange.days ?? 30
        
        // Load all data in parallel
        async let summaryTask = loadProductivitySummary(client: client)
        async let velocityTask = loadVelocity(client: client, days: days)
        async let burndownTask = loadBurndown(client: client, days: days)
        async let statsTask = loadQueueStats(client: client)
        async let historyTask = loadHistory(client: client, days: days)
        
        let (summary, velocity, burndown, stats, history) = await (
            summaryTask, velocityTask, burndownTask, statsTask, historyTask
        )
        
        var newData = AnalyticsData()
        newData.productivitySummary = summary
        newData.velocity = velocity
        newData.burndown = burndown
        newData.queueStats = stats
        newData.history = history
        
        analyticsData = newData
        analyticsLoading = false
    }

    private func loadProductivitySummary(client: RalphCLIClient) async -> ProductivitySummaryReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "productivity", "summary", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            return try decoder.decode(ProductivitySummaryReport.self, from: data)
        } catch {
            return nil
        }
    }

    private func loadVelocity(client: RalphCLIClient, days: Int) async -> ProductivityVelocityReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "productivity", "velocity", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            return try decoder.decode(ProductivityVelocityReport.self, from: data)
        } catch {
            return nil
        }
    }

    private func loadBurndown(client: RalphCLIClient, days: Int) async -> BurndownReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "burndown", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            return try decoder.decode(BurndownReport.self, from: data)
        } catch {
            return nil
        }
    }

    private func loadQueueStats(client: RalphCLIClient) async -> QueueStatsReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "stats", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            return try decoder.decode(QueueStatsReport.self, from: data)
        } catch {
            return nil
        }
    }

    private func loadHistory(client: RalphCLIClient, days: Int) async -> HistoryReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "history", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            let data = Data(collected.stdout.utf8)
            let decoder = JSONDecoder()
            return try decoder.decode(HistoryReport.self, from: data)
        } catch {
            return nil
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

// MARK: - Notification Names

extension Notification.Name {
    /// Posted when queue files are changed externally (via CLI or another process)
    public static let queueFilesExternallyChanged = Notification.Name("queueFilesExternallyChanged")
}

// MARK: - Error Recovery Support

extension Workspace {
    /// Report an error with recovery context
    @MainActor
    public func reportError(_ error: any Error, operation: String) {
        let recoveryError = RecoveryError.classify(
            error: error,
            operation: operation,
            workspaceURL: workingDirectoryURL
        )
        lastRecoveryError = recoveryError
        showErrorRecovery = true

        RalphLogger.shared.error(
            "Operation '\(operation)' failed: \(recoveryError.message)",
            category: .workspace
        )
    }

    /// Clear error recovery state
    @MainActor
    public func clearErrorRecovery() {
        lastRecoveryError = nil
        showErrorRecovery = false
        retryState = nil
    }
}
