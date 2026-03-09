/**
 WorkspaceManager

 Responsibilities:
 - Manage the lifecycle of all workspaces across the application.
 - Provide shared CLI client to all workspaces.
 - Handle window/tab restoration on app relaunch.
 - Coordinate workspace creation, duplication, and closure.
 - Perform CLI version compatibility check on initialization.
 - Resolve CLI executable from runtime override (`RALPH_BIN_PATH`) or bundled binary.

 Does not handle:
 - Per-workspace UI rendering (see WorkspaceView).
 - Direct UserDefaults access for workspace state (handled by Workspace).
 - Detailed version parsing logic (see VersionValidator).

 Invariants/assumptions callers must respect:
 - Single instance per app (ObservableObject singleton).
 - Window restoration state is stored under a dedicated UserDefaults key.
 - CLI client initialization failures are surfaced via errorMessage.
 - Version check results are cached for 5 minutes to avoid repeated subprocess calls.
 - Scene-scoped routing must register per-window and per-workspace actions before unfocused
   surfaces (menu bar, URL handlers) can target them directly.
 */

public import Foundation
public import Combine
import SwiftUI
import OSLog

public struct RalphAppLaunchPreparationResult {
    public let persistenceIssue: PersistenceIssue?
}

private struct WindowStateStore {
    private let defaults: UserDefaults
    private let restorationKey: String

    init(
        defaults: UserDefaults = RalphAppDefaults.userDefaults,
        restorationKey: String = RalphAppDefaults.productionDomainIdentifier + ".windowRestorationState"
    ) {
        self.defaults = defaults
        self.restorationKey = restorationKey
    }

    func loadAll() throws -> [WindowState] {
        guard let data = defaults.data(forKey: restorationKey) else {
            return []
        }
        return try JSONDecoder().decode([WindowState].self, from: data)
    }

    func saveAll(_ states: [WindowState]) throws {
        let data = try JSONEncoder().encode(states)
        defaults.set(data, forKey: restorationKey)
    }

    func clear() {
        defaults.removeObject(forKey: restorationKey)
    }
}

public enum RalphAppDefaults {
    public static let productionDomainIdentifier = "com.mitchfultz.ralph"
    public static let uiTestingDomainIdentifier = productionDomainIdentifier + ".uitesting"

    private static let uiTestingPathMarker = "/ralph-ui-tests/"
    private static let workspaceKeyPrefix = productionDomainIdentifier + ".workspace."
    private static let navigationKeyPrefix = productionDomainIdentifier + ".navigationState."
    private static let restorationKey = productionDomainIdentifier + ".windowRestorationState"

    public static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains("--uitesting")
    }

    public static var userDefaults: UserDefaults {
        if isUITesting, let suiteDefaults = UserDefaults(suiteName: uiTestingDomainIdentifier) {
            return suiteDefaults
        }
        return .standard
    }

    @MainActor
    public static func prepareForLaunch() -> RalphAppLaunchPreparationResult {
        if isUITesting {
            resetUITestingDefaults()
            return RalphAppLaunchPreparationResult(persistenceIssue: nil)
        }

        return RalphAppLaunchPreparationResult(
            persistenceIssue: pruneUITestingStateFromProductionDefaults()
        )
    }

    private static func resetUITestingDefaults() {
        guard let suiteDefaults = UserDefaults(suiteName: uiTestingDomainIdentifier) else { return }
        suiteDefaults.removePersistentDomain(forName: uiTestingDomainIdentifier)
    }

    private static func pruneUITestingStateFromProductionDefaults() -> PersistenceIssue? {
        let defaults = UserDefaults.standard
        let dictionary = defaults.dictionaryRepresentation()
        var contaminatedWorkspaceIDs = Set<UUID>()
        for key in dictionary.keys where key.hasPrefix(workspaceKeyPrefix) && key.hasSuffix(".snapshot") {
            guard let data = dictionary[key] as? Data else { continue }
            do {
                let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: data)
                guard snapshot.workingDirectoryURL.path.contains(uiTestingPathMarker),
                      let workspaceID = workspaceID(fromWorkspaceKey: key) else {
                    continue
                }
                contaminatedWorkspaceIDs.insert(workspaceID)
            } catch {
                return PersistenceIssue(
                    domain: .appDefaultsPreparation,
                    operation: .load,
                    context: key,
                    error: error
                )
            }
        }

        guard !contaminatedWorkspaceIDs.isEmpty else { return nil }

        for workspaceID in contaminatedWorkspaceIDs {
            removeWorkspaceState(workspaceID, from: defaults)
        }

        do {
            let store = WindowStateStore(defaults: defaults, restorationKey: restorationKey)
            let states = try store.loadAll()
            let filteredStates = states.compactMap { state -> WindowState? in
                var updated = state
                updated.workspaceIDs.removeAll { contaminatedWorkspaceIDs.contains($0) }
                updated.validateSelection()
                return updated.workspaceIDs.isEmpty ? nil : updated
            }
            try store.saveAll(filteredStates)
            return nil
        } catch {
            return PersistenceIssue(
                domain: .appDefaultsPreparation,
                operation: .load,
                context: restorationKey,
                error: error
            )
        }
    }

    private static func removeWorkspaceState(_ workspaceID: UUID, from defaults: UserDefaults) {
        let workspacePrefix = workspaceKeyPrefix + workspaceID.uuidString + "."
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(workspacePrefix) {
            defaults.removeObject(forKey: key)
        }

        defaults.removeObject(forKey: navigationKeyPrefix + workspaceID.uuidString)
    }

    private static func workspaceID(fromWorkspaceKey key: String) -> UUID? {
        let suffix = key.dropFirst(workspaceKeyPrefix.count)
        guard let separatorIndex = suffix.firstIndex(of: ".") else { return nil }
        return UUID(uuidString: String(suffix[..<separatorIndex]))
    }
}

@MainActor
public final class WorkspaceManager: ObservableObject {
    public static let shared = WorkspaceManager()
    public static let cliBinaryOverrideEnvKey = "RALPH_BIN_PATH"

    @Published public private(set) var workspaces: [Workspace] = []
    @Published public var errorMessage: String?
    @Published public private(set) var versionCheckResult: VersionValidator.VersionCheckResult?
    @Published public var focusedWorkspace: Workspace?
    @Published public private(set) var persistenceIssue: PersistenceIssue?

    public private(set) var client: RalphCLIClient?

    private let restorationKey = "com.mitchfultz.ralph.windowRestorationState"
    private let versionCheckCacheKey = "com.mitchfultz.ralph.versionCheckCache"
    private var unclaimedWindowStates: [WindowState] = []
    private var restorationPoolInitialized = false
    private let sceneRouter = WorkspaceSceneRouter()

    private init() {
        let preparation = RalphAppDefaults.prepareForLaunch()
        persistenceIssue = preparation.persistenceIssue

        if !configureInitialClient() {
            return
        }

        // Perform version compatibility check asynchronously
        Task { @MainActor in
            await performVersionCheck()
        }

        migrateLegacyStateIfNeeded()
    }

    @discardableResult
    private func configureInitialClient() -> Bool {
        let environment = ProcessInfo.processInfo.environment
        if let overridePath = environment[Self.cliBinaryOverrideEnvKey],
           !overridePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let overrideURL = URL(fileURLWithPath: overridePath, isDirectory: false)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            do {
                client = try RalphCLIClient(executableURL: overrideURL)
                errorMessage = nil
                RalphLogger.shared.info(
                    "Using CLI override from environment: \(overrideURL.path)",
                    category: .cli
                )
                return true
            } catch {
                RalphLogger.shared.error(
                    "Ignoring invalid CLI override '\(overridePath)': \(error)",
                    category: .cli
                )
            }
        }

        do {
            client = try RalphCLIClient.bundled()
            errorMessage = nil
            return true
        } catch {
            errorMessage = "Failed to locate bundled ralph executable: \(error)"
            return false
        }
    }

    /// Reject CLI executable paths provided by URL/launcher context.
    public func adoptCLIExecutable(path: String) {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        RalphLogger.shared.error(
            "Rejected insecure URL-driven CLI override: \(trimmed)",
            category: .cli
        )
    }

    // MARK: - Workspace Lifecycle

    @discardableResult
    public func createWorkspace(workingDirectory: URL? = nil) -> Workspace {
        createWorkspace(id: UUID(), workingDirectory: workingDirectory)
    }

    @discardableResult
    public func createWorkspace(id: UUID, workingDirectory: URL? = nil) -> Workspace {
        if let existing = workspaces.first(where: { $0.id == id }) {
            return existing
        }

        let defaultDirectory = workspaces.last?.workingDirectoryURL
            ?? FileManager.default.homeDirectoryForCurrentUser
        let directory = workingDirectory ?? defaultDirectory

        let workspace = Workspace(id: id, workingDirectoryURL: directory, client: client)
        workspaces.append(workspace)

        // Load CLI spec for the new workspace
        Task { @MainActor in
            await workspace.loadCLISpec()
        }

        return workspace
    }

    public func closeWorkspace(_ workspace: Workspace) {
        // Cancel any running operations
        workspace.cancel()
        workspace.removePersistedState()

        // Remove from tracking
        workspaces.removeAll { $0.id == workspace.id }

        // Clean up UserDefaults for this workspace
        cleanWorkspaceDefaults(workspace.id)
    }

    public func duplicateWorkspace(_ workspace: Workspace) -> Workspace {
        let newWorkspace = createWorkspace(workingDirectory: workspace.workingDirectoryURL)
        newWorkspace.name = "\(workspace.name) Copy"
        return newWorkspace
    }

    // MARK: - Window Restoration

    public func saveWindowState(_ state: WindowState) {
        var allStates = loadAllWindowStates()

        // Remove existing state for this window
        allStates.removeAll { $0.id == state.id }
        allStates.append(state)

        do {
            try WindowStateStore().saveAll(allStates)
            clearPersistenceIssue(domain: .windowRestoration)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .windowRestoration,
                    operation: .save,
                    context: restorationKey,
                    error: error
                )
            )
        }

        if restorationPoolInitialized {
            unclaimedWindowStates.removeAll { $0.id == state.id }
        }
    }

    public func loadAllWindowStates() -> [WindowState] {
        do {
            let states = try WindowStateStore().loadAll()
            clearPersistenceIssue(domain: .windowRestoration)
            return states
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .windowRestoration,
                    operation: .load,
                    context: restorationKey,
                    error: error
                )
            )
            return []
        }
    }

    public func removeWindowState(_ windowID: UUID) {
        var allStates = loadAllWindowStates()
        allStates.removeAll { $0.id == windowID }
        unclaimedWindowStates.removeAll { $0.id == windowID }

        do {
            try WindowStateStore().saveAll(allStates)
            clearPersistenceIssue(domain: .windowRestoration)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .windowRestoration,
                    operation: .save,
                    context: restorationKey,
                    error: error
                )
            )
        }
    }

    /// Claim a unique window state for a scene.
    ///
    /// If `preferredID` exists in restored state, that state is returned and removed from the
    /// unclaimed pool. Otherwise the next unclaimed restored state is returned. If none remain,
    /// a new default window state is created.
    public func claimWindowState(preferredID: UUID?) -> WindowState {
        ensureRestorationPool()

        if let preferredID,
           let preferredIndex = unclaimedWindowStates.firstIndex(where: { $0.id == preferredID }) {
            return unclaimedWindowStates.remove(at: preferredIndex)
        }

        if !unclaimedWindowStates.isEmpty {
            return unclaimedWindowStates.removeFirst()
        }

        let workspace = createWorkspace()
        return WindowState(workspaceIDs: [workspace.id])
    }

    public func restoreWindows() -> [WindowState] {
        let states = loadAllWindowStates()

        // No persisted window states: prefer already-created workspaces (e.g. URL-open launch path).
        if states.isEmpty {
            let restorableExisting = workspaces.filter { workspaceIsRestorable($0.workingDirectoryURL) }
            if !restorableExisting.isEmpty {
                return [
                    WindowState(
                        workspaceIDs: restorableExisting.map(\.id),
                        selectedTabIndex: 0
                    )
                ]
            }

            let workspace = createWorkspace()
            return [WindowState(workspaceIDs: [workspace.id])]
        }

        var restoredStates: [WindowState] = []
        for state in states {
            var rebuiltState = state
            rebuiltState.workspaceIDs = state.workspaceIDs.filter { workspaceID in
                if let existing = workspaces.first(where: { $0.id == workspaceID }) {
                    return workspaceIsRestorable(existing.workingDirectoryURL)
                }
                guard let restored = restoreWorkspace(id: workspaceID) else { return false }
                return workspaceIsRestorable(restored.workingDirectoryURL)
            }
            rebuiltState.validateSelection()
            if !rebuiltState.workspaceIDs.isEmpty {
                restoredStates.append(rebuiltState)
            }
        }

        if restoredStates.isEmpty {
            let workspace = createWorkspace()
            return [WindowState(workspaceIDs: [workspace.id])]
        }

        return restoredStates
    }

    private func ensureRestorationPool() {
        guard !restorationPoolInitialized else { return }
        unclaimedWindowStates = restoreWindows()
        restorationPoolInitialized = true
    }

    /// Reset in-memory window-state claim tracking.
    /// Used by tests to isolate singleton state between test cases.
    func resetWindowStateClaimPool() {
        unclaimedWindowStates.removeAll()
        restorationPoolInitialized = false
    }

    // MARK: - Legacy Migration

    private func migrateLegacyStateIfNeeded() {
        let defaults = RalphAppDefaults.userDefaults
        let migratedKey = "com.mitchfultz.ralph.legacyMigrated"

        guard !defaults.bool(forKey: migratedKey) else { return }

        // Check for legacy working directory
        if let legacyPath = defaults.string(forKey: "com.mitchfultz.ralph.workingDirectoryPath") {
            let url = URL(fileURLWithPath: legacyPath, isDirectory: true)
            if FileManager.default.fileExists(atPath: url.path) {
                let workspace = createWorkspace(workingDirectory: url)

                // Migrate recent directories
                if let legacyRecents = defaults.array(forKey: "com.mitchfultz.ralph.recentWorkingDirectoryPaths") as? [String] {
                    let recents = legacyRecents
                        .map { URL(fileURLWithPath: $0, isDirectory: true) }
                        .filter { url in
                            var isDir: ObjCBool = false
                            return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
                        }
                    workspace.recentWorkingDirectories = recents
                }
            }

            // Mark as migrated
            defaults.set(true, forKey: migratedKey)
        }
    }

    private func cleanWorkspaceDefaults(_ workspaceID: UUID) {
        let prefix = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString)."
        let defaults = RalphAppDefaults.userDefaults

        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(prefix) {
            defaults.removeObject(forKey: key)
        }
    }

    private func workspaceWorkingDirectory(_ workspaceID: UUID) -> URL? {
        let snapshotKeyPrefix = RalphAppDefaults.productionDomainIdentifier + ".workspace."
        let snapshot: RalphWorkspaceDefaultsSnapshot
        do {
            guard let loaded = try WorkspaceStateStore().load(id: workspaceID, keyPrefix: snapshotKeyPrefix) else {
                return nil
            }
            snapshot = loaded
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .workspaceState,
                    operation: .load,
                    context: "\(snapshotKeyPrefix)\(workspaceID.uuidString).snapshot",
                    error: error
                )
            )
            return nil
        }
        let url = snapshot.workingDirectoryURL
        return workspaceIsRestorable(url) ? url : nil
    }

    @discardableResult
    private func restoreWorkspace(id: UUID) -> Workspace? {
        if let existing = workspaces.first(where: { $0.id == id }) {
            return workspaceIsRestorable(existing.workingDirectoryURL) ? existing : nil
        }
        guard let directory = workspaceWorkingDirectory(id) else { return nil }
        return createWorkspace(id: id, workingDirectory: directory)
    }

    private func workspaceDirectoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }

    private func workspaceIsRestorable(_ url: URL) -> Bool {
        guard workspaceDirectoryExists(url) else { return false }
        return Workspace.existingQueueFileURL(in: url) != nil
    }

    // MARK: - Version Compatibility

    /// Cached version check result structure
    private struct CachedVersionResult: Codable {
        let timestamp: Date
        let isCompatible: Bool
        let versionString: String
    }

    /// Performs async version check of the CLI.
    /// Caches successful results to avoid repeated subprocess calls.
    /// Tries `--version` first, falls back to `version` subcommand for compatibility.
    @MainActor
    private func performVersionCheck() async {
        // Check cache first
        if let cached = checkCachedVersionResult(), cached.isCompatible {
            RalphLogger.shared.debug("Using cached CLI version check result", category: .cli)
            self.versionCheckResult = cached
            return
        }

        let result = await executeVersionCheck()
        if let result = result {
            self.versionCheckResult = result
            
            if result.isCompatible {
                cacheVersionResult(result)
                RalphLogger.shared.info("CLI version compatible: \(result.rawVersion)", category: .cli)
            } else {
                var message = result.errorMessage ?? "Unknown version error"
                if let guidance = result.guidanceMessage {
                    message += "\n\n" + guidance
                }
                errorMessage = message
                RalphLogger.shared.error("CLI version incompatible: \(message)", category: .cli)
            }
        }
    }
    
    /// Executes the CLI version check subprocess and validates the result.
    /// - Returns: The validation result, or nil if the check failed to execute
    @MainActor
    private func executeVersionCheck() async -> VersionValidator.VersionCheckResult? {
        guard let client = self.client else {
            errorMessage = "Cannot check CLI version: client not initialized"
            return nil
        }

        do {
            // Try `--version` first, fall back to `version` subcommand
            var output = try await client.runAndCollect(arguments: ["--version"])
            if output.status.code != 0 {
                output = try await client.runAndCollect(arguments: ["version"])
            }

            guard output.status.code == 0 else {
                let message = "CLI version check failed with exit code \(output.status.code)"
                errorMessage = message
                RalphLogger.shared.error("CLI version check failed: \(message)", category: .cli)
                return nil
            }

            let versionString = output.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
            let validator = VersionValidator()
            return validator.validate(versionString)

        } catch {
            let message = "Failed to check CLI version: \(error.localizedDescription)"
            errorMessage = message
            RalphLogger.shared.error("Failed to check CLI version: \(message)", category: .cli)
            return nil
        }
    }

    /// Check if we have a recent cached version result
    private func checkCachedVersionResult() -> VersionValidator.VersionCheckResult? {
        guard let data = RalphAppDefaults.userDefaults.data(forKey: versionCheckCacheKey) else {
            return nil
        }
        let cached: CachedVersionResult
        do {
            cached = try JSONDecoder().decode(CachedVersionResult.self, from: data)
            clearPersistenceIssue(domain: .versionCache)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .versionCache,
                    operation: .load,
                    context: versionCheckCacheKey,
                    error: error
                )
            )
            RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)
            return nil
        }

        // Check if cache is still valid
        let age = Date().timeIntervalSince(cached.timestamp)
        guard age < VersionCompatibility.cacheDuration else {
            RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)
            return nil
        }

        // Return a compatible result (we only cache successful checks)
        if cached.isCompatible {
            return VersionValidator.VersionCheckResult(status: .compatible, rawVersion: cached.versionString)
        }

        return nil
    }

    /// Cache a successful version check result
    private func cacheVersionResult(_ result: VersionValidator.VersionCheckResult) {
        guard result.isCompatible else { return }

        let cached = CachedVersionResult(
            timestamp: Date(),
            isCompatible: true,
            versionString: result.rawVersion
        )

        do {
            let data = try JSONEncoder().encode(cached)
            RalphAppDefaults.userDefaults.set(data, forKey: versionCheckCacheKey)
            clearPersistenceIssue(domain: .versionCache)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .versionCache,
                    operation: .save,
                    context: versionCheckCacheKey,
                    error: error
                )
            )
        }
    }

    /// Public method to manually trigger a version check (for "Check for Updates" menu)
    @MainActor
    public func checkForCLIUpdates() async -> VersionValidator.VersionCheckResult? {
        // Clear cache to force fresh check
        RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)

        guard let result = await executeVersionCheck() else {
            return nil
        }

        self.versionCheckResult = result

        if result.isCompatible {
            cacheVersionResult(result)
        } else {
            var message = result.errorMessage ?? "Unknown version error"
            if let guidance = result.guidanceMessage {
                message += "\n\n" + guidance
            }
            errorMessage = message
        }

        return result
    }

    private func recordPersistenceIssue(_ issue: PersistenceIssue) {
        persistenceIssue = issue
        RalphLogger.shared.error(
            "WorkspaceManager persistence \(issue.domain.rawValue) \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
            category: .workspace
        )
    }

    private func clearPersistenceIssue(domain: PersistenceIssue.Domain) {
        guard persistenceIssue?.domain == domain else { return }
        persistenceIssue = nil
    }
}

public extension WorkspaceManager {
    func registerWindowRouteActions(for windowID: UUID, actions: WindowRouteActions) {
        sceneRouter.registerWindowRouteActions(for: windowID, actions: actions)
    }

    func unregisterWindowRouteActions(for windowID: UUID) {
        sceneRouter.unregisterWindowRouteActions(for: windowID)
    }

    func registerWorkspaceRouteActions(
        for workspaceID: UUID,
        perform: @escaping (WorkspaceSceneRoute) -> Void
    ) {
        sceneRouter.registerWorkspaceRouteActions(for: workspaceID, perform: perform)
    }

    func unregisterWorkspaceRouteActions(for workspaceID: UUID) {
        sceneRouter.unregisterWorkspaceRouteActions(for: workspaceID)
    }

    func route(_ route: WorkspaceSceneRoute, to workspaceID: UUID) {
        revealWorkspace(workspaceID)
        sceneRouter.route(route, to: workspaceID)
    }

    func revealWorkspace(_ workspaceID: UUID) {
        if sceneRouter.focusWorkspace(
            workspaceID,
            focusedWorkspaceID: focusedWorkspace?.id
        ) {
            focusedWorkspace = workspaces.first(where: { $0.id == workspaceID })
            return
        }
    }

    func persistRegisteredWindowStates() {
        sceneRouter.persistRegisteredWindowStates()
    }

    func resetSceneRoutingForTests() {
        sceneRouter.resetForTests()
    }
}
