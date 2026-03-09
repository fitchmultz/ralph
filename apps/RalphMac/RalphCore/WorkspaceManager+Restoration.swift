/**
 WorkspaceManager+Restoration

 Responsibilities:
 - Persist and claim window restoration state across launches.
 - Rebuild window/tab state from persisted workspace snapshots.

 Does not handle:
 - Workspace creation policy outside restoration flows.
 - Scene routing registrations.

 Invariants/assumptions callers must respect:
 - Claimed window states are unique per live scene until the pool is reset.
 - Empty or invalid restoration payloads fall back to a fresh workspace window.
 */

public import Foundation

public extension WorkspaceManager {
    func saveWindowState(_ state: WindowState) {
        var allStates = loadAllWindowStates()
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

    func loadAllWindowStates() -> [WindowState] {
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

    func removeWindowState(_ windowID: UUID) {
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

    func claimWindowState(preferredID: UUID?) -> WindowState {
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

    func restoreWindows() -> [WindowState] {
        let states = loadAllWindowStates()

        if states.isEmpty {
            let restorableExisting = workspaces.filter {
                workspaceIsRestorable($0.identityState.workingDirectoryURL)
            }
            if !restorableExisting.isEmpty {
                return [
                    WindowState(
                        workspaceIDs: restorableExisting.map { $0.id },
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
                    return workspaceIsRestorable(existing.identityState.workingDirectoryURL)
                }
                guard let restored = restoreWorkspace(id: workspaceID) else { return false }
                return workspaceIsRestorable(restored.identityState.workingDirectoryURL)
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

    func ensureRestorationPool() {
        guard !restorationPoolInitialized else { return }
        unclaimedWindowStates = restoreWindows()
        restorationPoolInitialized = true
    }

    func resetWindowStateClaimPool() {
        unclaimedWindowStates.removeAll()
        restorationPoolInitialized = false
    }
}
