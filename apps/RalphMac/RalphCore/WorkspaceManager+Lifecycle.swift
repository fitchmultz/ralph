/**
 WorkspaceManager+Lifecycle

 Responsibilities:
 - Manage workspace creation, duplication, closure, and legacy-state migration.
 - Restore workspace-level persisted directories when rebuilding sessions.

 Does not handle:
 - Window-state claiming/persistence.
 - CLI version checks.

 Invariants/assumptions callers must respect:
 - Restorable workspaces must still exist on disk and contain a Ralph queue file.
 - Closing a workspace removes its persisted app-default state.
 */

public import Foundation

public extension WorkspaceManager {
    @discardableResult
    func createWorkspace(
        workingDirectory: URL? = nil,
        launchDisposition: Workspace.LaunchDisposition = .regular
    ) -> Workspace {
        createWorkspace(id: UUID(), workingDirectory: workingDirectory, launchDisposition: launchDisposition)
    }

    @discardableResult
    func createWorkspace(
        id: UUID,
        workingDirectory: URL? = nil,
        launchDisposition: Workspace.LaunchDisposition = .regular
    ) -> Workspace {
        if let existing = workspaces.first(where: { $0.id == id }) {
            return existing
        }

        let defaultDirectory = workspaces.last?.identityState.workingDirectoryURL
            ?? FileManager.default.homeDirectoryForCurrentUser
        let directory = workingDirectory ?? defaultDirectory

        let workspace = Workspace(
            id: id,
            workingDirectoryURL: directory,
            launchDisposition: launchDisposition,
            client: client
        )
        workspaces.append(workspace)
        if focusedWorkspace == nil && lastActiveWorkspaceID == nil {
            lastActiveWorkspaceID = workspace.id
        }

        scheduleWorkspaceBootstrap(for: workspace)
        return workspace
    }

    func closeWorkspace(_ workspace: Workspace) {
        cancelWorkspaceBootstrap(for: workspace.id)
        workspace.shutdown()
        workspace.cancel()
        workspace.removePersistedState()
        workspaces.removeAll { $0.id == workspace.id }
        if focusedWorkspace?.id == workspace.id {
            focusedWorkspace = nil
        }
        if lastActiveWorkspaceID == workspace.id {
            lastActiveWorkspaceID = nil
        }
        let fallbackWorkspace = effectiveWorkspace
        if focusedWorkspace == nil {
            focusedWorkspace = fallbackWorkspace
        }
        if lastActiveWorkspaceID == nil {
            lastActiveWorkspaceID = fallbackWorkspace?.id
        }
        cleanWorkspaceDefaults(workspace.id)
    }

    func duplicateWorkspace(_ workspace: Workspace) -> Workspace {
        let newWorkspace = createWorkspace(workingDirectory: workspace.identityState.workingDirectoryURL)
        newWorkspace.identityState.name = "\(workspace.identityState.name) Copy"
        return newWorkspace
    }

    func scheduleWorkspaceBootstrap(for workspace: Workspace) {
        cancelWorkspaceBootstrap(for: workspace.id)
        let revision = (workspaceBootstrapRevisions[workspace.id] ?? 0) &+ 1
        workspaceBootstrapRevisions[workspace.id] = revision

        workspaceBootstrapTasks[workspace.id] = Task { @MainActor [weak self, weak workspace] in
            guard let self, let workspace, !workspace.isShutDown else { return }
            await workspace.loadCLISpec()
            guard self.workspaceBootstrapRevisions[workspace.id] == revision else { return }
            self.workspaceBootstrapTasks[workspace.id] = nil
        }
    }

    func cancelWorkspaceBootstrap(for workspaceID: UUID) {
        workspaceBootstrapTasks[workspaceID]?.cancel()
        workspaceBootstrapTasks[workspaceID] = nil
        workspaceBootstrapRevisions[workspaceID] = (workspaceBootstrapRevisions[workspaceID] ?? 0) &+ 1
    }

    func migrateLegacyStateIfNeeded() {
        let defaults = RalphAppDefaults.userDefaults
        let migratedKey = "com.mitchfultz.ralph.legacyMigrated"

        guard !defaults.bool(forKey: migratedKey) else { return }

        if let legacyPath = defaults.string(forKey: "com.mitchfultz.ralph.workingDirectoryPath") {
            let url = URL(fileURLWithPath: legacyPath, isDirectory: true)
            if FileManager.default.fileExists(atPath: url.path) {
                let workspace = createWorkspace(workingDirectory: url)

                if let legacyRecents = defaults.array(forKey: "com.mitchfultz.ralph.recentWorkingDirectoryPaths") as? [String] {
                    let recents = legacyRecents
                        .map { URL(fileURLWithPath: $0, isDirectory: true) }
                        .filter { url in
                            var isDir: ObjCBool = false
                            return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
                        }
                    workspace.identityState.recentWorkingDirectories = recents
                }
            }

            defaults.set(true, forKey: migratedKey)
        }
    }

    func cleanWorkspaceDefaults(_ workspaceID: UUID) {
        let prefix = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString)."
        let defaults = RalphAppDefaults.userDefaults

        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(prefix) {
            defaults.removeObject(forKey: key)
        }
    }

    func workspaceWorkingDirectory(_ workspaceID: UUID) -> URL? {
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
    func restoreWorkspace(id: UUID) -> Workspace? {
        if let existing = workspaces.first(where: { $0.id == id }) {
            return workspaceIsRestorable(existing.identityState.workingDirectoryURL) ? existing : nil
        }
        guard let directory = workspaceWorkingDirectory(id) else { return nil }
        return createWorkspace(id: id, workingDirectory: directory)
    }

    func workspaceDirectoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }

    func workspaceIsRestorable(_ url: URL) -> Bool {
        guard workspaceDirectoryExists(url) else { return false }
        return Workspace.existingQueueFileURL(in: url) != nil
    }
}
