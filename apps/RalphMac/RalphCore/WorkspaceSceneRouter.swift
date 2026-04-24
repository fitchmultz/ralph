/**
 WorkspaceSceneRouter

 Purpose:
 - Own scene-routing registrations for windows and workspace views.

 Responsibilities:
 - Own scene-routing registrations for windows and workspace views.
 - Resolve which window should reveal or host a workspace route.
 - Queue workspace routes until a scene registers its handler.

 Does not handle:
 - Workspace lifecycle creation or restoration.
 - SwiftUI view composition.
 - Persisting window state directly.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - All mutations happen on the main actor.
 - Window registration order determines fallback routing preference.
 - Route actions must be unregistered when scenes disappear.
 */

public import Foundation

public enum WorkspaceSceneRoute: Equatable {
    case showTaskCreation
    case showTaskDecompose(taskID: String?)
    case showTaskDetail(taskID: String)
}

public struct WindowRouteActions {
    public let containsWorkspace: (UUID) -> Bool
    public let focusWorkspace: (UUID) -> Void
    public let appendWorkspace: (UUID) -> Void
    public let revealWindow: () -> Void
    public let persistState: () -> Void

    public init(
        containsWorkspace: @escaping (UUID) -> Bool,
        focusWorkspace: @escaping (UUID) -> Void,
        appendWorkspace: @escaping (UUID) -> Void,
        revealWindow: @escaping () -> Void,
        persistState: @escaping () -> Void
    ) {
        self.containsWorkspace = containsWorkspace
        self.focusWorkspace = focusWorkspace
        self.appendWorkspace = appendWorkspace
        self.revealWindow = revealWindow
        self.persistState = persistState
    }
}

@MainActor
final class WorkspaceSceneRouter {
    private var registeredWindowRouteActions: [UUID: WindowRouteActions] = [:]
    private var windowRouteRegistrationOrder: [UUID] = []
    private var registeredWorkspaceRouteActions: [UUID: (WorkspaceSceneRoute) -> Void] = [:]
    private var pendingWorkspaceRoutes: [UUID: [WorkspaceSceneRoute]] = [:]

    func registerWindowRouteActions(for windowID: UUID, actions: WindowRouteActions) {
        registeredWindowRouteActions[windowID] = actions
        if !windowRouteRegistrationOrder.contains(windowID) {
            windowRouteRegistrationOrder.append(windowID)
        }
    }

    func unregisterWindowRouteActions(for windowID: UUID) {
        registeredWindowRouteActions.removeValue(forKey: windowID)
        windowRouteRegistrationOrder.removeAll { $0 == windowID }
    }

    func registerWorkspaceRouteActions(
        for workspaceID: UUID,
        perform: @escaping (WorkspaceSceneRoute) -> Void
    ) {
        registeredWorkspaceRouteActions[workspaceID] = perform
        let queuedRoutes = pendingWorkspaceRoutes.removeValue(forKey: workspaceID) ?? []
        for route in queuedRoutes {
            perform(route)
        }
    }

    func unregisterWorkspaceRouteActions(for workspaceID: UUID) {
        registeredWorkspaceRouteActions.removeValue(forKey: workspaceID)
    }

    func route(_ route: WorkspaceSceneRoute, to workspaceID: UUID) {
        if let perform = registeredWorkspaceRouteActions[workspaceID] {
            perform(route)
        } else {
            pendingWorkspaceRoutes[workspaceID, default: []].append(route)
        }
    }

    func persistRegisteredWindowStates() {
        for windowID in windowRouteRegistrationOrder {
            registeredWindowRouteActions[windowID]?.persistState()
        }
    }

    func resetForTests() {
        registeredWindowRouteActions.removeAll()
        windowRouteRegistrationOrder.removeAll()
        registeredWorkspaceRouteActions.removeAll()
        pendingWorkspaceRoutes.removeAll()
    }

    func focusWorkspace(
        _ workspaceID: UUID,
        focusedWorkspaceID: UUID?
    ) -> Bool {
        if let actions = windowRouteActions(containing: workspaceID) {
            actions.focusWorkspace(workspaceID)
            actions.revealWindow()
            actions.persistState()
            return true
        }

        guard let actions = preferredWindowRouteActions(focusedWorkspaceID: focusedWorkspaceID) else {
            return false
        }

        actions.appendWorkspace(workspaceID)
        actions.focusWorkspace(workspaceID)
        actions.revealWindow()
        actions.persistState()
        return true
    }

    private func windowRouteActions(containing workspaceID: UUID) -> WindowRouteActions? {
        for windowID in windowRouteRegistrationOrder {
            guard let actions = registeredWindowRouteActions[windowID] else { continue }
            if actions.containsWorkspace(workspaceID) {
                return actions
            }
        }
        return nil
    }

    private func preferredWindowRouteActions(focusedWorkspaceID: UUID?) -> WindowRouteActions? {
        if let focusedWorkspaceID,
           let actions = windowRouteActions(containing: focusedWorkspaceID) {
            return actions
        }

        for windowID in windowRouteRegistrationOrder {
            if let actions = registeredWindowRouteActions[windowID] {
                return actions
            }
        }

        return nil
    }
}
