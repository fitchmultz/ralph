/**
 Workspace+Persistence

 Responsibilities:
 - Persist workspace identity state through a single snapshot store.
 - Preserve user-selected workspace folder access with security-scoped bookmarks.
 - Track machine-resolved workspace paths from the active working directory.
 - Handle working-directory changes and associated watcher/config refreshes.
 - Surface persistence failures as workspace-scoped operational state.

 Does not handle:
 - Queue mutation flows.
 - CLI subprocess execution.
 - Error recovery presentation.

 Invariants/assumptions callers must respect:
 - Persistence keys remain namespaced by `Workspace.id`.
 - Working-directory changes must flow through this extension so recents and watchers stay in sync.
 - Machine-resolved paths override fallback `.ralph/...` defaults once available.
 */

public import Foundation
import AppKit
public import Combine

@MainActor
public final class WorkspaceIdentityState: ObservableObject {
    @Published public var name: String
    @Published public var workingDirectoryURL: URL
    @Published public var recentWorkingDirectories: [URL]
    @Published public var workingDirectoryBookmarkData: Data?
    @Published public var recentWorkingDirectoryBookmarks: [String: Data]
    @Published public var resolvedPaths: MachineQueuePaths?
    @Published public internal(set) var repositoryGeneration: UInt64
    @Published public internal(set) var retargetRevision: UInt64
    private var securityScopedWorkingDirectoryURL: URL?

    public init(
        name: String,
        workingDirectoryURL: URL,
        recentWorkingDirectories: [URL],
        workingDirectoryBookmarkData: Data? = nil,
        recentWorkingDirectoryBookmarks: [String: Data] = [:],
        resolvedPaths: MachineQueuePaths? = nil,
        repositoryGeneration: UInt64 = 0,
        retargetRevision: UInt64 = 0
    ) {
        self.name = name
        self.workingDirectoryURL = workingDirectoryURL
        self.recentWorkingDirectories = recentWorkingDirectories
        self.workingDirectoryBookmarkData = workingDirectoryBookmarkData
        self.recentWorkingDirectoryBookmarks = recentWorkingDirectoryBookmarks
        self.resolvedPaths = resolvedPaths
        self.repositoryGeneration = repositoryGeneration
        self.retargetRevision = retargetRevision
    }

    deinit {
        securityScopedWorkingDirectoryURL?.stopAccessingSecurityScopedResource()
    }

    func replaceSecurityScopedWorkingDirectoryAccess(with url: URL?, bookmarkData: Data?) {
        securityScopedWorkingDirectoryURL?.stopAccessingSecurityScopedResource()
        securityScopedWorkingDirectoryURL = nil

        guard bookmarkData != nil, let url else { return }
        if url.startAccessingSecurityScopedResource() {
            securityScopedWorkingDirectoryURL = url
        }
    }
}

public struct PersistenceIssue: Identifiable, Equatable, Sendable {
    public enum Domain: String, Sendable {
        case workspaceState
        case cachedTasks
        case navigationState
        case temporaryFiles
        case windowRestoration
        case versionCache
        case appDefaultsPreparation
        case crashReporting
    }

    public enum Operation: String, Sendable {
        case load
        case save
        case delete
        case prune
        case install
        case export
    }

    public let id: UUID
    public let domain: Domain
    public let operation: Operation
    public let context: String
    public let message: String
    public let timestamp: Date

    public init(
        id: UUID = UUID(),
        domain: Domain,
        operation: Operation,
        context: String,
        message: String,
        timestamp: Date = Date()
    ) {
        self.id = id
        self.domain = domain
        self.operation = operation
        self.context = context
        self.message = message
        self.timestamp = timestamp
    }

    init(domain: Domain, operation: Operation, context: String, error: any Error) {
        self.init(
            domain: domain,
            operation: operation,
            context: context,
            message: String(describing: error)
        )
    }
}

struct RalphWorkspaceDefaultsSnapshot: Codable, Sendable {
    let name: String
    let workingDirectoryURL: URL
    let recentWorkingDirectories: [URL]
    let workingDirectoryBookmarkData: Data?
    let recentWorkingDirectoryBookmarks: [String: Data]?

    init(
        name: String,
        workingDirectoryURL: URL,
        recentWorkingDirectories: [URL],
        workingDirectoryBookmarkData: Data? = nil,
        recentWorkingDirectoryBookmarks: [String: Data]? = nil
    ) {
        self.name = name
        self.workingDirectoryURL = workingDirectoryURL
        self.recentWorkingDirectories = recentWorkingDirectories
        self.workingDirectoryBookmarkData = workingDirectoryBookmarkData
        self.recentWorkingDirectoryBookmarks = recentWorkingDirectoryBookmarks
    }
}

struct WorkspaceStateStore {
    private let defaults: UserDefaults

    init(defaults: UserDefaults = RalphAppDefaults.userDefaults) {
        self.defaults = defaults
    }

    func load(id: UUID, keyPrefix: String) throws -> RalphWorkspaceDefaultsSnapshot? {
        guard let data = defaults.data(forKey: snapshotKey(id: id, keyPrefix: keyPrefix)) else {
            return nil
        }
        return try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: data)
    }

    func save(_ snapshot: RalphWorkspaceDefaultsSnapshot, id: UUID, keyPrefix: String) throws {
        let data = try JSONEncoder().encode(snapshot)
        defaults.set(data, forKey: snapshotKey(id: id, keyPrefix: keyPrefix))
    }

    func remove(id: UUID, keyPrefix: String) {
        defaults.removeObject(forKey: snapshotKey(id: id, keyPrefix: keyPrefix))
    }

    private func snapshotKey(id: UUID, keyPrefix: String) -> String {
        "\(keyPrefix)\(id.uuidString).snapshot"
    }
}

public extension Workspace {
    func defaultsKey(_ suffix: String) -> String {
        "com.mitchfultz.ralph.workspace.\(id.uuidString).\(suffix)"
    }

    var hasRalphQueueFile: Bool {
        if let resolvedQueueFileURL {
            return FileManager.default.fileExists(atPath: resolvedQueueFileURL.path)
        }
        return Self.existingQueueFileURL(in: identityState.workingDirectoryURL) != nil
    }

    var projectDisplayName: String {
        let pathName = identityState.workingDirectoryURL.standardizedFileURL.lastPathComponent
            .trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
        if !pathName.isEmpty, pathName != "/" {
            return pathName
        }

        let storedName = identityState.name.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
        if !storedName.isEmpty {
            return storedName
        }

        return "workspace"
    }

    var queueFileURL: URL {
        resolvedQueueFileURL ?? Self.preferredQueueFileURL(in: identityState.workingDirectoryURL)
    }

    var doneFileURL: URL {
        resolvedDoneFileURL
            ?? identityState.workingDirectoryURL.appendingPathComponent(".ralph/done.jsonc", isDirectory: false)
    }

    var projectConfigFileURL: URL? {
        if let path = identityState.resolvedPaths?.projectConfigPath {
            return URL(fileURLWithPath: path, isDirectory: false)
        }
        return identityState.workingDirectoryURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
    }

    var queueWatcherTargets: QueueFileWatcher.WatchTargets {
        QueueFileWatcher.WatchTargets(
            workingDirectoryURL: identityState.workingDirectoryURL,
            queueFileURL: queueFileURL,
            doneFileURL: doneFileURL,
            projectConfigFileURL: projectConfigFileURL
        )
    }

    var resolvedQueueFileURL: URL? {
        guard let path = identityState.resolvedPaths?.queuePath else { return nil }
        return URL(fileURLWithPath: path, isDirectory: false)
    }

    var resolvedDoneFileURL: URL? {
        guard let path = identityState.resolvedPaths?.donePath else { return nil }
        return URL(fileURLWithPath: path, isDirectory: false)
    }

    static func existingQueueFileURL(in workingDirectoryURL: URL) -> URL? {
        let candidate = workingDirectoryURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
        if FileManager.default.fileExists(atPath: candidate.path) {
            return candidate
        }
        return nil
    }

    static func preferredQueueFileURL(in workingDirectoryURL: URL) -> URL {
        existingQueueFileURL(in: workingDirectoryURL)
            ?? workingDirectoryURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
    }

    func loadState() {
        let store = WorkspaceStateStore()

        do {
            guard let snapshot = try store.load(
                id: id,
                keyPrefix: RalphAppDefaults.productionDomainIdentifier + ".workspace."
            ) else {
                clearPersistenceIssue(domain: .workspaceState)
                return
            }

            let bookmarkResolution = Self.resolveSecurityScopedBookmark(
                snapshot.workingDirectoryBookmarkData,
                fallbackURL: snapshot.workingDirectoryURL
            )
            identityState.replaceSecurityScopedWorkingDirectoryAccess(
                with: bookmarkResolution.url,
                bookmarkData: bookmarkResolution.bookmarkData
            )

            let restoredRecents = Self.restoreRecentWorkingDirectories(from: snapshot)
            let restoredWorkingDirectory = Self.directoryExists(bookmarkResolution.url)
                ? bookmarkResolution.url
                : identityState.workingDirectoryURL
            let restoredBookmarkData = restoredWorkingDirectory == bookmarkResolution.url
                ? bookmarkResolution.bookmarkData
                : identityState.workingDirectoryBookmarkData
            if restoredWorkingDirectory != bookmarkResolution.url {
                identityState.replaceSecurityScopedWorkingDirectoryAccess(
                    with: identityState.workingDirectoryURL,
                    bookmarkData: identityState.workingDirectoryBookmarkData
                )
            }

            identityState.recentWorkingDirectories = restoredRecents
            identityState.workingDirectoryURL = restoredWorkingDirectory
            identityState.workingDirectoryBookmarkData = restoredBookmarkData
            identityState.recentWorkingDirectoryBookmarks = snapshot.recentWorkingDirectoryBookmarks ?? [:]
            identityState.name = snapshot.name
            clearPersistenceIssue(domain: .workspaceState)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .workspaceState,
                    operation: .load,
                    context: identityState.workingDirectoryURL.path,
                    error: error
                )
            )
        }
    }

    func persistState() {
        let store = WorkspaceStateStore()
        let snapshot = RalphWorkspaceDefaultsSnapshot(
            name: identityState.name,
            workingDirectoryURL: identityState.workingDirectoryURL,
            recentWorkingDirectories: identityState.recentWorkingDirectories,
            workingDirectoryBookmarkData: identityState.workingDirectoryBookmarkData,
            recentWorkingDirectoryBookmarks: identityState.recentWorkingDirectoryBookmarks
        )

        do {
            try store.save(
                snapshot,
                id: id,
                keyPrefix: RalphAppDefaults.productionDomainIdentifier + ".workspace."
            )
            clearPersistenceIssue(domain: .workspaceState)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .workspaceState,
                    operation: .save,
                    context: identityState.workingDirectoryURL.path,
                    error: error
                )
            )
        }
    }

    func shutdown() {
        guard !isShutDown else { return }

        isShutDown = true
        cancelRepositoryActivity()
        cancelOperationalHealthRefresh()
        cancelHealthCheck()
        identityState.repositoryGeneration &+= 1
        identityState.retargetRevision &+= 1
        runnerController.prepareForRepositoryRetarget()
        queueRuntime.stopWatching()
        identityState.replaceSecurityScopedWorkingDirectoryAccess(with: nil, bookmarkData: nil)
        refreshOperationalHealth()
    }

    func setWorkingDirectory(_ url: URL, bookmarkData: Data? = nil) {
        guard !isShutDown else { return }

        markStartupPlaceholderConsumed()
        let standardizedURL = Self.normalizedWorkingDirectoryURL(url)
        let currentURL = normalizedWorkingDirectoryURL
        let resolvedBookmarkData = bookmarkData
            ?? identityState.recentWorkingDirectoryBookmarks[standardizedURL.path]
            ?? (standardizedURL == currentURL ? identityState.workingDirectoryBookmarkData : nil)
        guard standardizedURL != currentURL else {
            identityState.workingDirectoryBookmarkData = resolvedBookmarkData
            identityState.replaceSecurityScopedWorkingDirectoryAccess(
                with: standardizedURL,
                bookmarkData: resolvedBookmarkData
            )
            updateRecentWorkingDirectories(with: standardizedURL, bookmarkData: resolvedBookmarkData)
            persistState()
            return
        }

        cancelRepositoryActivity()
        cancelHealthCheck()
        runnerController.prepareForRepositoryRetarget()
        queueRuntime.prepareForRepositoryRetarget()
        resetRepositoryDerivedStateForRetarget()

        let repositoryContext = beginRepositoryRetarget(to: standardizedURL)
        identityState.workingDirectoryBookmarkData = resolvedBookmarkData
        identityState.replaceSecurityScopedWorkingDirectoryAccess(
            with: standardizedURL,
            bookmarkData: resolvedBookmarkData
        )
        updateRecentWorkingDirectories(with: standardizedURL, bookmarkData: resolvedBookmarkData)

        persistState()
        queueRuntime.restartWatching()
        scheduleHealthCheck()
        refreshOperationalHealth()

        scheduleRepositoryActivity {
            await $0.reloadRepositoryContext(repositoryContext)
        }
    }

    func chooseWorkingDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.prompt = "Choose"

        if panel.runModal() == .OK, let url = panel.url {
            setWorkingDirectory(url, bookmarkData: Self.securityScopedBookmarkData(for: url))
        }
    }

    func selectRecentWorkingDirectory(_ url: URL) {
        let standardizedURL = Self.normalizedWorkingDirectoryURL(url)
        let bookmarkData = identityState.recentWorkingDirectoryBookmarks[standardizedURL.path]
        let resolution = Self.resolveSecurityScopedBookmark(bookmarkData, fallbackURL: standardizedURL)
        setWorkingDirectory(resolution.url, bookmarkData: resolution.bookmarkData)
    }
}

extension Workspace {
    static func securityScopedBookmarkData(for url: URL) -> Data? {
        do {
            return try url.bookmarkData(
                options: .withSecurityScope,
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
        } catch {
            RalphLogger.shared.debug(
                "Failed to create security-scoped bookmark for \(url.path): \(error)",
                category: .workspace
            )
            return nil
        }
    }

    static func resolveSecurityScopedBookmark(
        _ bookmarkData: Data?,
        fallbackURL: URL
    ) -> (url: URL, bookmarkData: Data?) {
        guard let bookmarkData else {
            return (Self.normalizedWorkingDirectoryURL(fallbackURL), nil)
        }

        do {
            var isStale = false
            let resolvedURL = try URL(
                resolvingBookmarkData: bookmarkData,
                options: [.withSecurityScope, .withoutUI],
                relativeTo: nil,
                bookmarkDataIsStale: &isStale
            )
            let normalizedURL = Self.normalizedWorkingDirectoryURL(resolvedURL)
            let refreshedBookmarkData = isStale
                ? Self.securityScopedBookmarkData(for: normalizedURL)
                : bookmarkData
            return (normalizedURL, refreshedBookmarkData)
        } catch {
            RalphLogger.shared.debug(
                "Failed to resolve security-scoped workspace bookmark for \(fallbackURL.path): \(error)",
                category: .workspace
            )
            return (Self.normalizedWorkingDirectoryURL(fallbackURL), nil)
        }
    }

    static func restoreRecentWorkingDirectories(from snapshot: RalphWorkspaceDefaultsSnapshot) -> [URL] {
        let bookmarks = snapshot.recentWorkingDirectoryBookmarks ?? [:]
        var restored: [URL] = []
        for recentURL in snapshot.recentWorkingDirectories {
            let normalizedURL = Self.normalizedWorkingDirectoryURL(recentURL)
            if let bookmarkData = bookmarks[normalizedURL.path] {
                restored.append(
                    Self.resolveSecurityScopedBookmark(bookmarkData, fallbackURL: normalizedURL).url
                )
            } else {
                restored.append(normalizedURL)
            }
        }
        return restored
    }

    private func updateRecentWorkingDirectories(with url: URL, bookmarkData: Data?) {
        var newRecents = identityState.recentWorkingDirectories.filter { $0.path != url.path }
        newRecents.insert(url, at: 0)
        if newRecents.count > 12 {
            newRecents = Array(newRecents.prefix(12))
        }
        identityState.recentWorkingDirectories = newRecents

        if let bookmarkData {
            identityState.recentWorkingDirectoryBookmarks[url.path] = bookmarkData
        }
        let activePaths = Set(newRecents.map(\.path))
        identityState.recentWorkingDirectoryBookmarks = identityState.recentWorkingDirectoryBookmarks
            .filter { activePaths.contains($0.key) }
    }

    func resetRepositoryDerivedStateForRetarget() {
        clearErrorRecovery()
        stopLoop()
        resetExecutionState()
        runState.isRunning = false
        runState.stopAfterCurrent = false
        runState.currentTaskID = nil
        runState.errorMessage = nil
        runState.lastExitStatus = nil
        runState.executionHistory.removeAll(keepingCapacity: false)
        runState.currentRunnerConfig = nil
        runState.runnerConfigErrorMessage = nil
        runState.runnerConfigLoading = false
        runState.runControlSelectedTaskID = nil
        identityState.resolvedPaths = nil
        runState.output = ""
        runState.outputBuffer.clear()
        runState.attributedOutput = []
        runState.streamProcessor.reset()

        taskState.tasks.removeAll(keepingCapacity: false)
        taskState.tasksErrorMessage = nil
        taskState.tasksLoading = false
        taskState.lastQueueRefreshEvent = nil

        commandState.cliSpec = nil
        commandState.cliSpecErrorMessage = nil
        commandState.cliSpecIsLoading = false
        commandState.advancedSelectedCommandID = nil
        resetAdvancedInputs()

        let previousAnalyticsTimeRange = insightsState.analytics.timeRange
        insightsState.graphData = nil
        insightsState.graphDataErrorMessage = nil
        insightsState.graphDataLoading = false
        insightsState.analytics = AnalyticsDashboardState(timeRange: previousAnalyticsTimeRange)

        clearCachedTasks()
        diagnosticsState.cliHealthStatus = nil
    }

    func reloadRepositoryContext(_ repositoryContext: RepositoryContext) async {
        guard !isShutDown, !Task.isCancelled, isCurrentRepositoryContext(repositoryContext) else { return }
        await refreshRepositoryState(retryConfiguration: .minimal)
    }

    func removePersistedState() {
        WorkspaceStateStore().remove(
            id: id,
            keyPrefix: RalphAppDefaults.productionDomainIdentifier + ".workspace."
        )
    }

    func recordPersistenceIssue(_ issue: PersistenceIssue) {
        diagnosticsState.persistenceIssue = issue
        refreshOperationalHealth()
        RalphLogger.shared.error(
            "Persistence \(issue.domain.rawValue) \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
            category: .workspace
        )
    }

    func clearPersistenceIssue(domain: PersistenceIssue.Domain, matchingContext: String? = nil) {
        guard diagnosticsState.persistenceIssue?.domain == domain else { return }
        if let matchingContext, diagnosticsState.persistenceIssue?.context != matchingContext {
            return
        }
        diagnosticsState.persistenceIssue = nil
        refreshOperationalHealth()
    }

    public func updateNavigationPersistenceIssue(_ issue: PersistenceIssue?) {
        diagnosticsState.navigationPersistenceIssue = issue
        refreshOperationalHealth()
        if let issue {
            RalphLogger.shared.error(
                "Navigation persistence \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
                category: .workspace
            )
        }
    }

    static func directoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}
