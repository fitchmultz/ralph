/**
 WindowStateRoutingTests

 Purpose:
 - Validate scene routing, focused/effective workspace selection, and launch-time app-default cleanup.

 Responsibilities:
 - Validate scene routing, focused/effective workspace selection, and launch-time app-default cleanup.

 Does not handle:
 - WindowState model encoding or raw persistence helpers.
 - Restoration claim selection and startup-placeholder identity helpers.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Window routing actions are registered against the shared WorkspaceManager test instance.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class WindowStateRoutingTests: WindowStateTestCase {
    func test_prepareForLaunch_prunesUITestingWorkspaceStateFromProductionDefaults() throws {
        let defaults = UserDefaults.standard
        let workspaceID = UUID()
        let navigationKey = "com.mitchfultz.ralph.navigationState.\(workspaceID.uuidString)"
        let snapshotKey = workspaceSnapshotKey(for: workspaceID)
        let cachedTasksKey = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString).cachedTasks"
        let tempUITestPath = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-ui-tests", isDirectory: true)
            .appendingPathComponent("ui-test-state-prune", isDirectory: true)
        let snapshot = RalphWorkspaceDefaultsSnapshot(
            name: "UI Test Workspace",
            workingDirectoryURL: tempUITestPath,
            recentWorkingDirectories: [tempUITestPath]
        )

        defaults.set(try JSONEncoder().encode(snapshot), forKey: snapshotKey)
        defaults.set(Data("cached".utf8), forKey: cachedTasksKey)
        defaults.set(Data("navigation".utf8), forKey: navigationKey)
        defaults.set(
            try JSONEncoder().encode([WindowState(workspaceIDs: [workspaceID])]),
            forKey: testRestorationKey
        )

        _ = RalphAppDefaults.prepareForLaunch()

        XCTAssertNil(defaults.object(forKey: snapshotKey))
        XCTAssertNil(defaults.object(forKey: cachedTasksKey))
        XCTAssertNil(defaults.object(forKey: navigationKey))
        XCTAssertTrue(manager.loadAllWindowStates().isEmpty)
    }

    func test_prepareForLaunch_unitTestingClearsStrayCurrentProcessDefaults() throws {
        let defaults = UserDefaults.standard
        let workspaceID = UUID()
        let snapshotKey = workspaceSnapshotKey(for: workspaceID)
        let cachedTasksKey = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString).cachedTasks"
        let navigationKey = "com.mitchfultz.ralph.navigationState.\(workspaceID.uuidString)"
        let versionCacheKey = "com.mitchfultz.ralph.versionCheckCache"
        let workspacePath = try makeWorkspaceDirectory(prefix: "unit-test-defaults-prune")
        defer { RalphCoreTestSupport.assertRemoved(workspacePath) }

        let snapshot = RalphWorkspaceDefaultsSnapshot(
            name: "Unit Test Workspace",
            workingDirectoryURL: workspacePath,
            recentWorkingDirectories: [workspacePath]
        )

        defaults.set(try JSONEncoder().encode(snapshot), forKey: snapshotKey)
        defaults.set(Data("cached".utf8), forKey: cachedTasksKey)
        defaults.set(Data("navigation".utf8), forKey: navigationKey)
        defaults.set(Data("version".utf8), forKey: versionCacheKey)
        defaults.set(
            try JSONEncoder().encode([WindowState(workspaceIDs: [workspaceID])]),
            forKey: testRestorationKey
        )

        _ = RalphAppDefaults.prepareForLaunch()

        XCTAssertNil(defaults.object(forKey: snapshotKey))
        XCTAssertNil(defaults.object(forKey: cachedTasksKey))
        XCTAssertNil(defaults.object(forKey: navigationKey))
        XCTAssertNil(defaults.object(forKey: versionCacheKey))
        XCTAssertNil(defaults.object(forKey: testRestorationKey))
    }

    func test_route_toWorkspace_focusesContainingWindowAndExecutesSceneAction() {
        let workspace = manager.createWorkspace()
        let windowID = UUID()
        var focusedWorkspaceID: UUID?
        var revealedWindow = false
        var persistedWindowState = false
        var receivedRoute: WorkspaceSceneRoute?

        manager.registerWindowRouteActions(
            for: windowID,
            actions: WindowRouteActions(
                containsWorkspace: { $0 == workspace.id },
                focusWorkspace: { focusedWorkspaceID = $0 },
                appendWorkspace: { _ in XCTFail("existing workspace should not append into a new window") },
                revealWindow: { revealedWindow = true },
                persistState: { persistedWindowState = true }
            )
        )
        manager.registerWorkspaceRouteActions(for: workspace.id) { route in
            receivedRoute = route
        }

        manager.route(.showTaskDetail(taskID: "RQ-123"), to: workspace.id)

        XCTAssertEqual(focusedWorkspaceID, workspace.id)
        XCTAssertEqual(receivedRoute, .showTaskDetail(taskID: "RQ-123"))
        XCTAssertTrue(revealedWindow)
        XCTAssertTrue(persistedWindowState)
        XCTAssertEqual(manager.focusedWorkspace?.id, workspace.id)
    }

    func test_route_toWorkspace_replaysPendingRouteWhenWorkspaceSceneRegisters() {
        let workspace = manager.createWorkspace()
        let windowID = UUID()
        var appendedWorkspaceID: UUID?
        var focusedWorkspaceID: UUID?
        var revealedWindow = false
        var receivedRoutes: [WorkspaceSceneRoute] = []

        manager.registerWindowRouteActions(
            for: windowID,
            actions: WindowRouteActions(
                containsWorkspace: { _ in false },
                focusWorkspace: { focusedWorkspaceID = $0 },
                appendWorkspace: { appendedWorkspaceID = $0 },
                revealWindow: { revealedWindow = true },
                persistState: {}
            )
        )

        manager.route(.showTaskCreation, to: workspace.id)
        XCTAssertEqual(appendedWorkspaceID, workspace.id)
        XCTAssertEqual(focusedWorkspaceID, workspace.id)
        XCTAssertTrue(revealedWindow)

        manager.registerWorkspaceRouteActions(for: workspace.id) { route in
            receivedRoutes.append(route)
        }

        XCTAssertEqual(receivedRoutes, [.showTaskCreation])
    }

    func test_effectiveWorkspace_prefersFocusedAndLastActiveWorkspace() throws {
        let firstDirectory = try makeWorkspaceDirectory(prefix: "effective-workspace-first")
        let secondDirectory = try makeWorkspaceDirectory(prefix: "effective-workspace-second")
        defer {
            RalphCoreTestSupport.assertRemoved(firstDirectory)
            RalphCoreTestSupport.assertRemoved(secondDirectory)
        }

        let firstWorkspace = manager.createWorkspace(workingDirectory: firstDirectory)
        let secondWorkspace = manager.createWorkspace(workingDirectory: secondDirectory)

        XCTAssertEqual(manager.effectiveWorkspace?.id, firstWorkspace.id)

        manager.revealWorkspace(secondWorkspace.id)
        XCTAssertEqual(manager.effectiveWorkspace?.id, secondWorkspace.id)

        manager.markWorkspaceActive(firstWorkspace)
        XCTAssertEqual(manager.effectiveWorkspace?.id, firstWorkspace.id)

        manager.markWorkspaceActive(nil)
        XCTAssertEqual(manager.effectiveWorkspace?.id, firstWorkspace.id)
    }

    func test_closeWorkspace_reassignsEffectiveWorkspaceWhenActiveWorkspaceCloses() throws {
        let firstDirectory = try makeWorkspaceDirectory(prefix: "effective-workspace-close-first")
        let secondDirectory = try makeWorkspaceDirectory(prefix: "effective-workspace-close-second")
        let thirdDirectory = try makeWorkspaceDirectory(prefix: "effective-workspace-close-third")
        defer {
            RalphCoreTestSupport.assertRemoved(firstDirectory)
            RalphCoreTestSupport.assertRemoved(secondDirectory)
            RalphCoreTestSupport.assertRemoved(thirdDirectory)
        }

        _ = manager.createWorkspace(workingDirectory: firstDirectory)
        let secondWorkspace = manager.createWorkspace(workingDirectory: secondDirectory)
        let thirdWorkspace = manager.createWorkspace(workingDirectory: thirdDirectory)

        manager.markWorkspaceActive(secondWorkspace)
        XCTAssertEqual(manager.effectiveWorkspace?.id, secondWorkspace.id)

        manager.closeWorkspace(secondWorkspace)

        XCTAssertEqual(manager.focusedWorkspace?.id, thirdWorkspace.id)
        XCTAssertEqual(manager.effectiveWorkspace?.id, thirdWorkspace.id)
        XCTAssertNotEqual(manager.lastActiveWorkspaceID, secondWorkspace.id)
    }

    func test_prepareForLaunch_clearsPersistedAppWindowFrameState() {
        let defaults = UserDefaults.standard
        let offscreenKey = "NSWindow Frame main-AppWindow-1"
        let onscreenKey = "NSWindow Frame main-AppWindow-2"

        defaults.set("490 -1280 1400 900 -314 1600 3008 1661 ", forKey: offscreenKey)
        defaults.set("100 100 1200 800 0 0 2560 1600 ", forKey: onscreenKey)

        _ = RalphAppDefaults.prepareForLaunch()

        XCTAssertNil(defaults.object(forKey: offscreenKey))
        XCTAssertNil(defaults.object(forKey: onscreenKey))
    }
}
