/**
 WorkspaceManager+Routing

 Responsibilities:
 - Bridge scene-scoped route registration to the shared WorkspaceSceneRouter.
 - Reveal workspaces and persist registered window states for unfocused surfaces.

 Does not handle:
 - Window restoration storage.
 - Workspace creation and migration.

 Invariants/assumptions callers must respect:
 - Route actions must be registered before unfocused surfaces attempt to target them.
 - Revealing a workspace prefers the focused scene router when available.
 */

public import Foundation

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
