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
import SwiftUI

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

    public enum TaskSortOption: String, CaseIterable {
        case priority = "Priority"
        case created = "Created"
        case updated = "Updated"
        case status = "Status"
        case title = "Title"
    }

    private var client: RalphCLIClient?
    private var currentRun: RalphCLIRun?
    private var cancellables = Set<AnyCancellable>()

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

    public func run(arguments: [String]) {
        guard let client else {
            errorMessage = "CLI client not available."
            return
        }
        guard !isRunning else { return }

        output = ""
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true

        do {
            let run = try client.start(
                arguments: arguments,
                currentDirectoryURL: workingDirectoryURL
            )
            currentRun = run

            Task { @MainActor in
                for await event in run.events {
                    let prefix: String = (event.stream == .stdout) ? "" : "[stderr] "
                    output.append(prefix)
                    output.append(event.text)
                }

                let status = await run.waitUntilExit()
                lastExitStatus = status
                isRunning = false
                currentRun = nil
            }
        } catch {
            errorMessage = "Failed to start ralph: \(error)"
            isRunning = false
            currentRun = nil
        }
    }

    public func cancel() {
        currentRun?.cancel()
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
