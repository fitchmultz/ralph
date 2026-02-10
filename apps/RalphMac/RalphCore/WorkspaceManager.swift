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
 */

public import Foundation
public import Combine
import SwiftUI
import OSLog

@MainActor
public final class WorkspaceManager: ObservableObject {
    public static let shared = WorkspaceManager()
    public static let cliBinaryOverrideEnvKey = "RALPH_BIN_PATH"

    @Published public private(set) var workspaces: [Workspace] = []
    @Published public var errorMessage: String?
    @Published public private(set) var versionCheckResult: VersionValidator.VersionCheckResult?

    public private(set) var client: RalphCLIClient?

    private let restorationKey = "com.mitchfultz.ralph.windowRestorationState"
    private let versionCheckCacheKey = "com.mitchfultz.ralph.versionCheckCache"
    private var unclaimedWindowStates: [WindowState] = []
    private var restorationPoolInitialized = false

    private init() {
        if !configureInitialClient() {
            return
        }

        // Perform version compatibility check asynchronously
        Task { @MainActor in
            await performVersionCheck()
        }

        // Migrate from legacy single-workspace state if needed
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

    /// Adopt a CLI executable path provided by URL/launcher context and refresh all workspaces.
    ///
    /// If the override is invalid and there is already a working client, this method logs and
    /// preserves the existing client. If no client exists, the error is surfaced in `errorMessage`.
    public func adoptCLIExecutable(path: String) {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        let executableURL = URL(fileURLWithPath: trimmed, isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()

        if let existing = client?.executableURL
            .standardizedFileURL
            .resolvingSymlinksInPath(),
           existing.path == executableURL.path {
            return
        }

        do {
            let newClient = try RalphCLIClient(executableURL: executableURL)
            client = newClient
            errorMessage = nil

            RalphLogger.shared.info(
                "Adopted CLI executable from URL context: \(executableURL.path)",
                category: .cli
            )

            for workspace in workspaces {
                workspace.injectClient(newClient)
                Task { @MainActor [weak workspace] in
                    guard let workspace else { return }
                    _ = await workspace.checkHealth()
                    if workspace.hasRalphQueueFile {
                        await workspace.loadTasks(retryConfiguration: .minimal)
                    }
                }
            }

            Task { @MainActor [weak self] in
                await self?.performVersionCheck()
            }
        } catch {
            RalphLogger.shared.error(
                "Failed to adopt CLI executable '\(executableURL.path)': \(error)",
                category: .cli
            )
            if client == nil {
                errorMessage = "Failed to use CLI executable override: \(error)"
            }
        }
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

        if let data = try? JSONEncoder().encode(allStates) {
            UserDefaults.standard.set(data, forKey: restorationKey)
        }

        if restorationPoolInitialized {
            unclaimedWindowStates.removeAll { $0.id == state.id }
        }
    }

    public func loadAllWindowStates() -> [WindowState] {
        guard let data = UserDefaults.standard.data(forKey: restorationKey),
              let states = try? JSONDecoder().decode([WindowState].self, from: data) else {
            return []
        }
        return states
    }

    public func removeWindowState(_ windowID: UUID) {
        var allStates = loadAllWindowStates()
        allStates.removeAll { $0.id == windowID }
        unclaimedWindowStates.removeAll { $0.id == windowID }

        if let data = try? JSONEncoder().encode(allStates) {
            UserDefaults.standard.set(data, forKey: restorationKey)
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
        let defaults = UserDefaults.standard
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
        let defaults = UserDefaults.standard

        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(prefix) {
            defaults.removeObject(forKey: key)
        }
    }

    private func workspaceWorkingDirectory(_ workspaceID: UUID) -> URL? {
        let key = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString).workingPath"
        guard let path = UserDefaults.standard.string(forKey: key) else {
            return nil
        }
        let url = URL(fileURLWithPath: path, isDirectory: true)
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
        let queueFile = url.appendingPathComponent(".ralph/queue.json", isDirectory: false)
        return FileManager.default.fileExists(atPath: queueFile.path)
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
        guard let data = UserDefaults.standard.data(forKey: versionCheckCacheKey),
              let cached = try? JSONDecoder().decode(CachedVersionResult.self, from: data) else {
            return nil
        }

        // Check if cache is still valid
        let age = Date().timeIntervalSince(cached.timestamp)
        guard age < VersionCompatibility.cacheDuration else {
            UserDefaults.standard.removeObject(forKey: versionCheckCacheKey)
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

        if let data = try? JSONEncoder().encode(cached) {
            UserDefaults.standard.set(data, forKey: versionCheckCacheKey)
        }
    }

    /// Public method to manually trigger a version check (for "Check for Updates" menu)
    @MainActor
    public func checkForCLIUpdates() async -> VersionValidator.VersionCheckResult? {
        // Clear cache to force fresh check
        UserDefaults.standard.removeObject(forKey: versionCheckCacheKey)

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
}
