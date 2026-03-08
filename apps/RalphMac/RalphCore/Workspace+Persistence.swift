/**
 Workspace+Persistence

 Responsibilities:
 - Persist workspace identity state through a single snapshot store.
 - Resolve queue-file paths from the active working directory.
 - Handle working-directory changes and associated watcher/config refreshes.
 - Surface persistence failures as workspace-scoped operational state.

 Does not handle:
 - Queue mutation flows.
 - CLI subprocess execution.
 - Error recovery presentation.

 Invariants/assumptions callers must respect:
 - Persistence keys remain namespaced by `Workspace.id`.
 - Working-directory changes must flow through this extension so recents and watchers stay in sync.
 - Queue-file resolution prefers `.ralph/queue.jsonc` when both formats are absent.
 */

public import Foundation
import AppKit
public import Combine

@MainActor
public final class WorkspaceIdentityState: ObservableObject {
    @Published public var name: String
    @Published public var workingDirectoryURL: URL
    @Published public var recentWorkingDirectories: [URL]

    public init(name: String, workingDirectoryURL: URL, recentWorkingDirectories: [URL]) {
        self.name = name
        self.workingDirectoryURL = workingDirectoryURL
        self.recentWorkingDirectories = recentWorkingDirectories
    }
}

public struct PersistenceIssue: Identifiable, Equatable, Sendable {
    public enum Domain: String, Sendable {
        case workspaceState
        case cachedTasks
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
        Self.existingQueueFileURL(in: workingDirectoryURL) != nil
    }

    var projectDisplayName: String {
        let pathName = workingDirectoryURL.standardizedFileURL.lastPathComponent
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if !pathName.isEmpty, pathName != "/" {
            return pathName
        }

        let storedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !storedName.isEmpty {
            return storedName
        }

        return "workspace"
    }

    var queueFileURL: URL {
        Self.preferredQueueFileURL(in: workingDirectoryURL)
    }

    static func existingQueueFileURL(in workingDirectoryURL: URL) -> URL? {
        for fileName in ["queue.jsonc", "queue.json"] {
            let candidate = workingDirectoryURL.appendingPathComponent(".ralph/\(fileName)", isDirectory: false)
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
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

            let restoredRecents = snapshot.recentWorkingDirectories.filter(Self.directoryExists)
            let restoredWorkingDirectory = Self.directoryExists(snapshot.workingDirectoryURL)
                ? snapshot.workingDirectoryURL
                : identityState.workingDirectoryURL

            identityState.recentWorkingDirectories = restoredRecents
            identityState.workingDirectoryURL = restoredWorkingDirectory
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
            recentWorkingDirectories: identityState.recentWorkingDirectories
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

    func setWorkingDirectory(_ url: URL) {
        identityState.workingDirectoryURL = url
        identityState.name = url.lastPathComponent

        var newRecents = identityState.recentWorkingDirectories.filter { $0.path != url.path }
        newRecents.insert(url, at: 0)
        if newRecents.count > 12 {
            newRecents = Array(newRecents.prefix(12))
        }
        identityState.recentWorkingDirectories = newRecents

        persistState()
        startFileWatching()
        lastTasksSnapshot.removeAll()

        if client != nil {
            Task { @MainActor [weak self] in
                await self?.loadRunnerConfiguration(retryConfiguration: .minimal)
            }
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
            setWorkingDirectory(url)
        }
    }

    func selectRecentWorkingDirectory(_ url: URL) {
        setWorkingDirectory(url)
    }
}

extension Workspace {
    func removePersistedState() {
        WorkspaceStateStore().remove(
            id: id,
            keyPrefix: RalphAppDefaults.productionDomainIdentifier + ".workspace."
        )
    }

    func recordPersistenceIssue(_ issue: PersistenceIssue) {
        diagnosticsState.persistenceIssue = issue
        RalphLogger.shared.error(
            "Persistence \(issue.domain.rawValue) \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
            category: .workspace
        )
    }

    func clearPersistenceIssue(domain: PersistenceIssue.Domain) {
        guard diagnosticsState.persistenceIssue?.domain == domain else { return }
        diagnosticsState.persistenceIssue = nil
    }

    static func directoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}
