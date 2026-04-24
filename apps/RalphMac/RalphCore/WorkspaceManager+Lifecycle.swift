/**
 WorkspaceManager+Lifecycle

 Purpose:
 - Manage workspace creation, duplication, closure, and legacy-state migration.

 Responsibilities:
 - Manage workspace creation, duplication, closure, and legacy-state migration.
 - Restore workspace-level persisted directories when rebuilding sessions.
 - Resolve persisted folder bookmarks before probing restorable workspaces.

 Does not handle:
 - Window-state claiming/persistence.
 - CLI version checks.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Restorable workspaces must still exist on disk and contain a Ralph queue file.
 - Closing a workspace removes its persisted app-default state.
 */

public import Foundation

public extension WorkspaceManager {
    @discardableResult
    func createWorkspace(
        workingDirectory: URL? = nil,
        launchDisposition: Workspace.LaunchDisposition = .regular,
        bootstrapRepositoryStateOnInit: Bool = true
    ) -> Workspace {
        createWorkspace(
            id: UUID(),
            workingDirectory: workingDirectory,
            launchDisposition: launchDisposition,
            bootstrapRepositoryStateOnInit: bootstrapRepositoryStateOnInit
        )
    }

    @discardableResult
    func createWorkspace(
        id: UUID,
        workingDirectory: URL? = nil,
        launchDisposition: Workspace.LaunchDisposition = .regular,
        bootstrapRepositoryStateOnInit: Bool = true
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
            client: client,
            bootstrapRepositoryStateOnInit: bootstrapRepositoryStateOnInit
        )
        workspaces.append(workspace)
        if focusedWorkspace == nil && lastActiveWorkspaceID == nil {
            lastActiveWorkspaceID = workspace.id
        }

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
        guard let snapshot = workspaceSnapshot(workspaceID) else {
            return nil
        }
        let workingDirectory = restoredWorkingDirectory(for: snapshot)
        return workspaceIsRestorable(workingDirectory) ? workingDirectory : nil
    }

    @discardableResult
    func restoreWorkspace(id: UUID, restorabilityCache: inout [String: Bool]) -> Workspace? {
        if let existing = workspaces.first(where: { $0.id == id }) {
            return cachedWorkspaceIsRestorable(
                existing.identityState.workingDirectoryURL,
                cache: &restorabilityCache
            ) ? existing : nil
        }
        guard let snapshot = workspaceSnapshot(id) else {
            return nil
        }
        let workingDirectory = restoredWorkingDirectory(for: snapshot)
        guard cachedWorkspaceIsRestorable(workingDirectory, cache: &restorabilityCache) else {
            return nil
        }

        return createWorkspace(
            id: id,
            workingDirectory: workingDirectory,
            bootstrapRepositoryStateOnInit: false
        )
    }

    func workspaceDirectoryExists(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }

    func workspaceIsRestorable(_ url: URL) -> Bool {
        guard workspaceDirectoryExists(url) else { return false }
        return Workspace.existingQueueFileURL(in: url) != nil
    }

    private func workspaceSnapshot(_ workspaceID: UUID) -> RalphWorkspaceDefaultsSnapshot? {
        let snapshotKeyPrefix = RalphAppDefaults.productionDomainIdentifier + ".workspace."
        do {
            return try WorkspaceStateStore().load(id: workspaceID, keyPrefix: snapshotKeyPrefix)
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
    }

    private func restoredWorkingDirectory(for snapshot: RalphWorkspaceDefaultsSnapshot) -> URL {
        Workspace.resolveSecurityScopedBookmark(
            snapshot.workingDirectoryBookmarkData,
            fallbackURL: snapshot.workingDirectoryURL
        ).url
    }

    func cachedWorkspaceIsRestorable(_ url: URL, cache: inout [String: Bool]) -> Bool {
        let normalizedURL = Workspace.normalizedWorkingDirectoryURL(url)
        if let cached = cache[normalizedURL.path] {
            return cached
        }

        let restorable = workspaceIsRestorable(normalizedURL)
        cache[normalizedURL.path] = restorable
        return restorable
    }
}
