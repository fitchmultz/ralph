/**
 WorkspaceManager+Routing

 Purpose:
 - Bridge scene-scoped route registration to the shared WorkspaceSceneRouter.

 Responsibilities:
 - Bridge scene-scoped route registration to the shared WorkspaceSceneRouter.
 - Reveal workspaces and persist registered window states for unfocused surfaces.

 Does not handle:
 - Window restoration storage.
 - Workspace creation and migration.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Route actions must be registered before unfocused surfaces attempt to target them.
 - Revealing a workspace prefers the focused scene router when available.
 */

public import Foundation

public extension WorkspaceManager {
    var effectiveWorkspace: Workspace? {
        if let focusedWorkspaceID = focusedWorkspace?.id,
           let focusedWorkspace = workspaces.first(where: { $0.id == focusedWorkspaceID }) {
            return focusedWorkspace
        }

        if let lastActiveWorkspaceID,
           let activeWorkspace = workspaces.first(where: { $0.id == lastActiveWorkspaceID }) {
            return activeWorkspace
        }

        if let lastWorkspace = workspaces.last {
            return lastWorkspace
        }
        return workspaces.first
    }

    func registerWindowRouteActions(for windowID: UUID, actions: WindowRouteActions) {
        sceneRouter.registerWindowRouteActions(for: windowID, actions: actions)
        attemptPendingWorkspaceRevealIfNeeded(trigger: "window-route-registered")
    }

    func unregisterWindowRouteActions(for windowID: UUID) {
        sceneRouter.unregisterWindowRouteActions(for: windowID)
    }

    func registerWorkspaceRouteActions(
        for workspaceID: UUID,
        perform: @escaping (WorkspaceSceneRoute) -> Void
    ) {
        sceneRouter.registerWorkspaceRouteActions(for: workspaceID, perform: perform)
        attemptPendingWorkspaceRevealIfNeeded(trigger: "workspace-route-registered")
    }

    func unregisterWorkspaceRouteActions(for workspaceID: UUID) {
        sceneRouter.unregisterWorkspaceRouteActions(for: workspaceID)
    }

    func route(_ route: WorkspaceSceneRoute, to workspaceID: UUID) {
        revealWorkspace(workspaceID)
        sceneRouter.route(route, to: workspaceID)
    }

    func markWorkspaceActive(_ workspace: Workspace?) {
        let newWorkspaceID = workspace?.id

        if focusedWorkspace?.id == newWorkspaceID, lastActiveWorkspaceID == newWorkspaceID {
            return
        }

        guard let workspace,
              workspaces.contains(where: { $0.id == workspace.id }) else {
            if focusedWorkspace == nil, lastActiveWorkspaceID == nil {
                return
            }
            focusedWorkspace = nil
            if let lastActiveWorkspaceID,
               !workspaces.contains(where: { $0.id == lastActiveWorkspaceID }) {
                self.lastActiveWorkspaceID = effectiveWorkspace?.id
            }
            return
        }

        focusedWorkspace = workspace
        lastActiveWorkspaceID = workspace.id
    }

    @discardableResult
    func revealWorkspace(_ workspaceID: UUID) -> Bool {
        if sceneRouter.focusWorkspace(
            workspaceID,
            focusedWorkspaceID: focusedWorkspace?.id
        ) {
            markWorkspaceActive(workspaces.first(where: { $0.id == workspaceID }))
            clearRevealHealth(for: workspaceID)
            return true
        }

        if let workspace = workspaces.first(where: { $0.id == workspaceID }) {
            lastActiveWorkspaceID = workspace.id
        }
        return false
    }

    func scheduleWorkspaceReveal(_ workspaceID: UUID) {
        workspaceRevealTask?.cancel()
        workspaceRevealTask = nil
        workspaceRevealRevision &+= 1
        let revision = workspaceRevealRevision

        pendingWorkspaceReveal = PendingWorkspaceReveal(
            workspaceID: workspaceID,
            revision: revision,
            startedAt: Date(),
            attempts: 0
        )
        clearRevealHealth(for: workspaceID)
        attemptPendingWorkspaceRevealIfNeeded(
            trigger: "scheduled",
            expectedRevision: revision
        )

        guard pendingWorkspaceReveal?.revision == revision else {
            return
        }

        let configuration = workspaceRevealConfiguration
        let maxAttempts = max(configuration.maxAttempts, 0)
        let yieldAttempts = max(configuration.yieldAttempts, 0)

        workspaceRevealTask = Task { @MainActor [weak self] in
            guard let self else { return }

            for attempt in 0..<maxAttempts {
                guard !Task.isCancelled else { return }
                guard self.pendingWorkspaceReveal?.revision == revision else { return }

                self.pendingWorkspaceReveal?.attempts = attempt + 1
                self.attemptPendingWorkspaceRevealIfNeeded(
                    trigger: "poll-\(attempt + 1)",
                    expectedRevision: revision
                )

                guard self.pendingWorkspaceReveal?.revision == revision else { return }
                guard attempt + 1 < maxAttempts else { continue }

                if attempt < yieldAttempts {
                    await configuration.yield()
                } else {
                    await configuration.sleep(configuration.retryDelayNanoseconds)
                }
            }

            self.recordWorkspaceRevealTimeout(for: workspaceID, revision: revision)
        }
    }

    func persistRegisteredWindowStates() {
        sceneRouter.persistRegisteredWindowStates()
    }

    func resetSceneRoutingForTests() {
        sceneRouter.resetForTests()
        workspaceRevealTask?.cancel()
        workspaceRevealTask = nil
        pendingWorkspaceReveal = nil
        workspaceRevealConfiguration = WorkspaceRevealRetryConfiguration()
        for workspace in workspaces where workspace.diagnosticsState.revealHealth != nil {
            workspace.diagnosticsState.revealHealth = nil
            workspace.refreshOperationalHealth()
        }
    }
}

private extension WorkspaceManager {
    func attemptPendingWorkspaceRevealIfNeeded(
        trigger: String,
        expectedRevision: UInt64? = nil
    ) {
        guard let pending = pendingWorkspaceReveal else { return }
        if let expectedRevision, pending.revision != expectedRevision {
            return
        }

        updateRevealHealthIfNeeded(
            for: pending.workspaceID,
            state: .pending(attempts: pending.attempts)
        )

        if revealWorkspace(pending.workspaceID) {
            let attempts = max(1, pending.attempts)
            RalphLogger.shared.info(
                "Resolved workspace reveal for \(pending.workspaceID.uuidString) via \(trigger) after \(attempts) attempt\(attempts == 1 ? "" : "s")",
                category: .workspace
            )
            pendingWorkspaceReveal = nil
            workspaceRevealTask?.cancel()
            workspaceRevealTask = nil
        }
    }

    func recordWorkspaceRevealTimeout(for workspaceID: UUID, revision: UInt64) {
        guard let pending = pendingWorkspaceReveal, pending.revision == revision else {
            return
        }

        let attempts = max(pending.attempts, workspaceRevealConfiguration.maxAttempts)
        updateRevealHealthIfNeeded(
            for: workspaceID,
            state: .timedOut(attempts: attempts)
        )
        RalphLogger.shared.error(
            "Workspace reveal timed out for \(workspaceID.uuidString) after \(attempts) attempts",
            category: .workspace
        )
        pendingWorkspaceReveal = nil
        workspaceRevealTask = nil
    }

    func updateRevealHealthIfNeeded(
        for workspaceID: UUID,
        state: WorkspaceRevealHealth.State
    ) {
        guard let workspace = workspaces.first(where: { $0.id == workspaceID }) else {
            return
        }

        if workspace.diagnosticsState.revealHealth?.state == state {
            return
        }

        workspace.diagnosticsState.revealHealth = WorkspaceRevealHealth(
            workspaceID: workspaceID,
            state: state
        )
        workspace.refreshOperationalHealth()
    }

    func clearRevealHealth(for workspaceID: UUID) {
        guard let workspace = workspaces.first(where: { $0.id == workspaceID }) else {
            return
        }
        guard workspace.diagnosticsState.revealHealth != nil else { return }

        workspace.diagnosticsState.revealHealth = nil
        workspace.refreshOperationalHealth()
    }
}
