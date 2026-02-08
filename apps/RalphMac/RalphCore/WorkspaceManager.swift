/**
 WorkspaceManager

 Responsibilities:
 - Manage the lifecycle of all workspaces across the application.
 - Provide shared CLI client to all workspaces.
 - Handle window/tab restoration on app relaunch.
 - Coordinate workspace creation, duplication, and closure.

 Does not handle:
 - Per-workspace UI rendering (see WorkspaceView).
 - Direct UserDefaults access for workspace state (handled by Workspace).

 Invariants/assumptions callers must respect:
 - Single instance per app (ObservableObject singleton).
 - Window restoration state is stored under a dedicated UserDefaults key.
 - CLI client initialization failures are surfaced via errorMessage.
 */

public import Foundation
public import Combine
import SwiftUI

@MainActor
public final class WorkspaceManager: ObservableObject {
    public static let shared = WorkspaceManager()

    @Published public private(set) var workspaces: [Workspace] = []
    @Published public var errorMessage: String?

    public private(set) var client: RalphCLIClient?

    private let restorationKey = "com.mitchfultz.ralph.windowRestorationState"

    private init() {
        do {
            client = try RalphCLIClient.bundled()
        } catch {
            errorMessage = "Failed to locate bundled ralph executable: \(error)"
        }

        // Migrate from legacy single-workspace state if needed
        migrateLegacyStateIfNeeded()
    }

    // MARK: - Workspace Lifecycle

    @discardableResult
    public func createWorkspace(workingDirectory: URL? = nil) -> Workspace {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let directory = workingDirectory ?? home

        let workspace = Workspace(workingDirectoryURL: directory, client: client)
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

        if let data = try? JSONEncoder().encode(allStates) {
            UserDefaults.standard.set(data, forKey: restorationKey)
        }
    }

    public func restoreWindows() -> [WindowState] {
        let states = loadAllWindowStates()

        // Validate and clean up any states with invalid directories
        let validStates = states.filter { state in
            state.workspaceIDs.contains { workspaceID in
                // Check if we can find a valid working directory for this workspace
                if let workspace = workspaces.first(where: { $0.id == workspaceID }) {
                    return FileManager.default.fileExists(atPath: workspace.workingDirectoryURL.path)
                }
                return false
            }
        }

        // If no valid states, create a default window with a new workspace
        if validStates.isEmpty {
            let workspace = createWorkspace()
            return [WindowState(workspaceIDs: [workspace.id])]
        }

        return validStates
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
}
