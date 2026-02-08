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
 */

public import Foundation
public import Combine
public import SwiftUI

public final class Workspace: ObservableObject, Identifiable, Codable, @unchecked Sendable {
    public let id: UUID

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

    public struct ExecutionRecord: Identifiable, Codable {
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

    public struct RunnerConfig {
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
    public struct ANSISegment: Identifiable {
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

    public enum ANSIColor {
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
            }
        }
    }

    private var client: RalphCLIClient?
    private var currentRun: RalphCLIRun?
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
        Task { @MainActor in
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

    public func loadTasks() async {
        guard let client else {
            tasksErrorMessage = "CLI client not available."
            return
        }

        tasksLoading = true
        tasksErrorMessage = nil

        do {
            let collected = try await client.runAndCollect(
                arguments: ["--no-color", "queue", "list", "--format", "json"],
                currentDirectoryURL: workingDirectoryURL
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
        } catch {
            tasksErrorMessage = "Failed to load tasks: \(error.localizedDescription)"
        }

        tasksLoading = false
    }
    
    // MARK: - File Watching

    /// Start watching queue files for external changes
    private func startFileWatching() {
        // Stop existing watcher
        fileWatcher?.stop()
        
        // Create and configure new watcher
        let watcher = QueueFileWatcher(workingDirectoryURL: workingDirectoryURL)
        watcher.onFileChanged = { [weak self] in
            Task { @MainActor [weak self] in
                await self?.handleExternalFileChange()
            }
        }
        watcher.start()
        fileWatcher = watcher
    }
    
    /// Stop file watching (call when workspace is being deallocated)
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
        
        var changed: [RalphTask] = []
        for task in current {
            if let previousTask = previous.first(where: { $0.id == task.id }) {
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
    
    /// Represents detected changes between two task snapshots
    public struct TaskChanges {
        public let added: [RalphTask]
        public let removed: [RalphTask]
        public let changed: [RalphTask]
        
        public var hasChanges: Bool {
            !added.isEmpty || !removed.isEmpty || !changed.isEmpty
        }
    }

    // MARK: - Graph Data Loading

    /// Load graph data from CLI for dependency visualization
    public func loadGraphData() async {
        guard let client else {
            graphDataErrorMessage = "CLI client not available."
            return
        }

        graphDataLoading = true
        graphDataErrorMessage = nil

        do {
            let collected = try await client.runAndCollect(
                arguments: ["--no-color", "queue", "graph", "--format", "json"],
                currentDirectoryURL: workingDirectoryURL
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
            graphDataErrorMessage = "Failed to load graph data: \(error.localizedDescription)"
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

        // Task is blocked if any dependency is not in "done" status
        for dependencyID in dependsOn {
            if let dependency = tasks.first(where: { $0.id == dependencyID }) {
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

        // Build arguments for status change
        let arguments = ["--no-color", "task", "edit", "status", newStatus.rawValue, taskID]

        let collected = try await client.runAndCollect(
            arguments: arguments,
            currentDirectoryURL: workingDirectoryURL
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
            _ = try? await client.runAndCollect(
                arguments: ["--no-color", "task", "edit", "started_at", startedAt, taskID],
                currentDirectoryURL: workingDirectoryURL
            )
        }

        // Reload tasks to get updated state
        await loadTasks()
    }

    // MARK: - Task Updates

    /// Update a task by applying changes via the CLI and reloading the task list.
    /// This compares the original task with the updated task and generates appropriate CLI commands.
    public func updateTask(from original: RalphTask, to updated: RalphTask) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
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

        // Execute each edit command
        for (field, value) in editCommands {
            let collected = try await client.runAndCollect(
                arguments: ["--no-color", "task", "edit", field, value, updated.id],
                currentDirectoryURL: workingDirectoryURL
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

        let collected = try await client.runAndCollect(
            arguments: arguments,
            currentDirectoryURL: workingDirectoryURL
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

        public var errorDescription: String? {
            switch self {
            case .cliClientUnavailable:
                return "CLI client is not available."
            case .cliError(let message):
                return message
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
        attributedOutput = []
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true
        executionStartTime = Date()

        do {
            let run = try client.start(
                arguments: arguments,
                currentDirectoryURL: workingDirectoryURL
            )
            currentRun = run

            Task { @MainActor in
                for await event in run.events {
                    let prefix: String = (event.stream == .stdout) ? "" : "[stderr] "
                    let text = prefix + event.text
                    output.append(text)

                    // Parse phase information from output
                    detectPhase(from: text)

                    // Parse ANSI codes for rich display
                    parseANSICodes(from: text)
                }

                let status = await run.waitUntilExit()
                lastExitStatus = status
                isRunning = false

                // Record execution history
                if let startTime = executionStartTime {
                    let record = ExecutionRecord(
                        id: UUID(),
                        taskID: currentTaskID,
                        startTime: startTime,
                        endTime: Date(),
                        exitCode: Int(status.code),
                        wasCancelled: false
                    )
                    addToHistory(record)
                }

                // Handle loop mode - run next task if enabled and not stopping
                if isLoopMode && !stopAfterCurrent && status.code == 0 {
                    // Small delay before next task
                    try? await Task.sleep(nanoseconds: 1_000_000_000) // 1 second
                    if isLoopMode && !stopAfterCurrent {
                        runNextTask()
                        return  // Don't reset state, we're continuing
                    }
                }

                resetExecutionState()
                currentRun = nil
            }
        } catch {
            errorMessage = "Failed to start ralph: \(error)"
            isRunning = false
            resetExecutionState()
            currentRun = nil
        }
    }

    public func cancel() {
        currentRun?.cancel()

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
        stopAfterCurrent = false
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

    /// Parse ANSI codes from raw output and update attributedOutput
    public func parseANSICodes(from rawOutput: String) {
        // This is a simplified parser - can be enhanced
        // Parse common ANSI escape sequences and convert to ANSISegments
        var segments: [ANSISegment] = []

        // For now, create a single segment with default styling
        // Full ANSI parsing can be implemented as an enhancement
        segments.append(ANSISegment(
            text: rawOutput,
            color: .default,
            isBold: false,
            isItalic: false
        ))

        // Merge with existing segments if needed, or replace
        if attributedOutput.isEmpty {
            attributedOutput = segments
        } else {
            // Append text to the last segment
            let lastIndex = attributedOutput.count - 1
            let lastSegment = attributedOutput[lastIndex]
            let mergedText = lastSegment.text + rawOutput
            attributedOutput[lastIndex] = ANSISegment(
                text: mergedText,
                color: lastSegment.color,
                isBold: lastSegment.isBold,
                isItalic: lastSegment.isItalic
            )
        }
    }

    // MARK: - CLI Spec Loading

    public func loadCLISpec() async {
        guard let client else {
            cliSpecErrorMessage = "CLI client not available."
            return
        }

        cliSpecIsLoading = true
        cliSpecErrorMessage = nil

        do {
            let collected = try await client.runAndCollect(
                arguments: ["--no-color", "__cli-spec", "--format", "json"],
                currentDirectoryURL: workingDirectoryURL
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
            cliSpecErrorMessage = "Failed to load CLI spec: \(error)"
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

    // MARK: - Codable

    enum CodingKeys: String, CodingKey {
        case id, name, workingDirectoryURL, recentWorkingDirectories
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encode(name, forKey: .name)
        try container.encode(workingDirectoryURL, forKey: .workingDirectoryURL)
        try container.encode(recentWorkingDirectories, forKey: .recentWorkingDirectories)
    }

    public required init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        workingDirectoryURL = try container.decode(URL.self, forKey: .workingDirectoryURL)
        recentWorkingDirectories = try container.decode([URL].self, forKey: .recentWorkingDirectories)

        // Initialize runtime state
        output = ""
        isRunning = false

        loadState()
    }
}

// MARK: - Notification Names

extension Notification.Name {
    /// Posted when queue files are changed externally (via CLI or another process)
    public static let queueFilesExternallyChanged = Notification.Name("queueFilesExternallyChanged")
}
