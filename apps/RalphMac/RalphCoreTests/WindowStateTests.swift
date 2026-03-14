/**
 WindowStateTests

 Responsibilities:
 - Validate WindowState persistence via WorkspaceManager
 - Ensure state is correctly saved and restored across app launches
 - Test navigation state persistence in NavigationViewModel

 Does not handle:
 - UI-level window management
 - Cross-window synchronization
 - Actual macOS window lifecycle
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class WindowStateTests: RalphCoreTestCase {
    private var manager: WorkspaceManager!
    private let testRestorationKey = "com.mitchfultz.ralph.windowRestorationState"
    private let testNavigationKey = "com.mitchfultz.ralph.navigationState"
    private let workspaceSnapshotPrefix = "com.mitchfultz.ralph.workspace."

    private var defaults: UserDefaults { RalphAppDefaults.userDefaults }

    override func setUp() {
        super.setUp()
        manager = WorkspaceManager.shared
        manager.resetWindowStateClaimPool()
        manager.resetSceneRoutingForTests()
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
        defaults.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
        cleanupWorkspaceSnapshots()
    }

    override func tearDown() {
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
        manager.resetWindowStateClaimPool()
        manager.resetSceneRoutingForTests()
        defaults.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
        cleanupWorkspaceSnapshots()
        super.tearDown()
    }

    private func cleanupNavigationState() {
        for key in defaults.dictionaryRepresentation().keys {
            if key.hasPrefix(testNavigationKey) {
                defaults.removeObject(forKey: key)
            }
        }
    }

    private func cleanupWorkspaceSnapshots() {
        for key in defaults.dictionaryRepresentation().keys {
            if key.hasPrefix(workspaceSnapshotPrefix) {
                defaults.removeObject(forKey: key)
            }
        }
    }

    private func workspaceSnapshotKey(for workspaceID: UUID) -> String {
        workspaceSnapshotPrefix + workspaceID.uuidString + ".snapshot"
    }

    private func makeWorkspaceDirectory(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }

    private func makeSeededWorkspaceDirectory(prefix: String) throws -> URL {
        let directory = try makeWorkspaceDirectory(prefix: prefix)
        let ralphDirectory = directory.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDirectory, withIntermediateDirectories: true)
        try #"{"version":1,"tasks":[]}"#.write(
            to: ralphDirectory.appendingPathComponent("queue.jsonc", isDirectory: false),
            atomically: true,
            encoding: .utf8
        )
        return directory
    }

    // MARK: - WindowState Persistence Tests

    func test_saveAndLoadWindowState_roundTrip() throws {
        // Create a window state with test workspaces
        let workspace1 = manager.createWorkspace()
        let workspace2 = manager.createWorkspace()
        let windowState = WindowState(
            workspaceIDs: [workspace1.id, workspace2.id],
            selectedTabIndex: 1
        )

        // Save the state
        manager.saveWindowState(windowState)

        // Load all states
        let loadedStates = manager.loadAllWindowStates()

        // Verify
        XCTAssertEqual(loadedStates.count, 1)
        XCTAssertEqual(loadedStates.first?.id, windowState.id)
        XCTAssertEqual(loadedStates.first?.workspaceIDs, windowState.workspaceIDs)
        XCTAssertEqual(loadedStates.first?.selectedTabIndex, 1)
        XCTAssertEqual(loadedStates.first?.version, 1)
    }

    func test_loadAllWindowStates_withNoSavedState_returnsEmpty() {
        let states = manager.loadAllWindowStates()
        XCTAssertTrue(states.isEmpty)
    }

    func test_removeWindowState_removesCorrectState() {
        // Create and save two window states
        let ws1 = manager.createWorkspace()
        let ws2 = manager.createWorkspace()
        let state1 = WindowState(workspaceIDs: [ws1.id])
        let state2 = WindowState(workspaceIDs: [ws2.id])

        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        // Remove first state
        manager.removeWindowState(state1.id)

        // Verify only second remains
        let loaded = manager.loadAllWindowStates()
        XCTAssertEqual(loaded.count, 1)
        XCTAssertEqual(loaded.first?.id, state2.id)
    }

    func test_saveWindowState_overwritesExistingState() {
        // Create and save initial state
        let ws = manager.createWorkspace()
        let state = WindowState(workspaceIDs: [ws.id], selectedTabIndex: 0)
        manager.saveWindowState(state)

        // Modify and save again
        let ws2 = manager.createWorkspace()
        var modifiedState = state
        modifiedState.workspaceIDs.append(ws2.id)
        modifiedState.selectedTabIndex = 1
        manager.saveWindowState(modifiedState)

        // Verify only the modified state exists
        let loaded = manager.loadAllWindowStates()
        XCTAssertEqual(loaded.count, 1)
        XCTAssertEqual(loaded.first?.workspaceIDs.count, 2)
        XCTAssertEqual(loaded.first?.selectedTabIndex, 1)
    }

    func test_windowState_versionField_defaultsToOne() {
        let ws = manager.createWorkspace()
        let state = WindowState(workspaceIDs: [ws.id])
        XCTAssertEqual(state.version, 1)
    }

    func test_windowState_validateSelection_withEmptyWorkspaces() {
        var state = WindowState(workspaceIDs: [], selectedTabIndex: 5)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 0)
    }

    func test_windowState_validateSelection_withOutOfBoundsIndex() {
        let ws1 = manager.createWorkspace()
        let ws2 = manager.createWorkspace()
        var state = WindowState(workspaceIDs: [ws1.id, ws2.id], selectedTabIndex: 10)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 1) // Should clamp to max valid index
    }

    func test_windowState_validateSelection_withNegativeIndex() {
        let ws = manager.createWorkspace()
        var state = WindowState(workspaceIDs: [ws.id], selectedTabIndex: -5)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 0) // Should clamp to min valid index
    }

    // MARK: - Restore Windows Tests

    func test_restoreWindows_withNoSavedState_createsDefault() {
        // Ensure no saved state
        defaults.removeObject(forKey: testRestorationKey)

        let restored = manager.restoreWindows()

        // Should create a default window with new workspace
        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs.count, 1)
    }

    func test_restoreWindows_withNoSavedState_usesExistingWorkspace() throws {
        defaults.removeObject(forKey: testRestorationKey)
        let temp = try makeWorkspaceDirectory(prefix: "restore-windows-existing")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs, [workspace.id])
    }

    func test_createWorkspace_persistsInitialWorkingDirectory() throws {
        let temp = try makeWorkspaceDirectory(prefix: "persist-working-directory")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let key = workspaceSnapshotKey(for: workspace.id)
        let snapshotData = try XCTUnwrap(defaults.data(forKey: key))
        let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: snapshotData)

        XCTAssertEqual(snapshot.workingDirectoryURL, temp)
        XCTAssertEqual(snapshot.name, temp.lastPathComponent)
    }

    func test_unitTestDefaults_areIsolatedFromStandardDefaults() {
        let key = "com.mitchfultz.ralph.unit-test-isolation"
        defer {
            defaults.removeObject(forKey: key)
            UserDefaults.standard.removeObject(forKey: key)
        }

        defaults.set("isolated", forKey: key)

        XCTAssertEqual(defaults.string(forKey: key), "isolated")
        XCTAssertNil(UserDefaults.standard.object(forKey: key))
    }

    func test_workspaceProjectDisplayName_prefersWorkingDirectoryLeafName() throws {
        let temp = try makeWorkspaceDirectory(prefix: "workspace-project-display-name")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(workingDirectoryURL: temp)
        workspace.name = "RalphMac"

        XCTAssertEqual(workspace.projectDisplayName, temp.lastPathComponent)
    }

    func test_workspaceMatchesWorkingDirectory_normalizesInputURLs() throws {
        let temp = try makeWorkspaceDirectory(prefix: "workspace-match-working-directory")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let nestedPath = temp.appendingPathComponent("..", isDirectory: true)
            .appendingPathComponent(temp.lastPathComponent, isDirectory: true)
        let workspace = Workspace(workingDirectoryURL: nestedPath)

        XCTAssertTrue(workspace.matchesWorkingDirectory(temp))
        XCTAssertEqual(workspace.normalizedWorkingDirectoryURL.path, temp.path)
    }

    func test_isURLRoutingPlaceholderWorkspace_trueForExplicitStartupPlaceholderDuringInitialLoad() throws {
        let temp = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-empty")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(
            workingDirectoryURL: temp,
            launchDisposition: .startupPlaceholder
        )

        workspace.taskState.tasksLoading = true

        XCTAssertTrue(workspace.isURLRoutingPlaceholderWorkspace)
    }

    func test_isURLRoutingPlaceholderWorkspace_falseForRegularWorkspaceEvenWhenQueueIsEmpty() throws {
        let temp = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-populated")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = Workspace(workingDirectoryURL: temp)
        workspace.taskState.tasks = []
        workspace.taskState.tasksLoading = false
        workspace.taskState.tasksErrorMessage = nil

        XCTAssertFalse(workspace.isURLRoutingPlaceholderWorkspace)
    }

    func test_setWorkingDirectory_consumesURLRoutingPlaceholderDisposition() throws {
        let initialDirectory = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-initial")
        let targetDirectory = try makeSeededWorkspaceDirectory(prefix: "url-routing-placeholder-target")
        defer {
            RalphCoreTestSupport.assertRemoved(initialDirectory)
            RalphCoreTestSupport.assertRemoved(targetDirectory)
        }

        let workspace = Workspace(
            workingDirectoryURL: initialDirectory,
            launchDisposition: .startupPlaceholder
        )

        XCTAssertTrue(workspace.isURLRoutingPlaceholderWorkspace)

        workspace.setWorkingDirectory(targetDirectory)

        XCTAssertFalse(workspace.isURLRoutingPlaceholderWorkspace)
        XCTAssertEqual(workspace.identityState.workingDirectoryURL.path, targetDirectory.path)
    }

    func test_restoreWindows_withValidSavedState_restoresCorrectly() {
        // Create and save a window state
        let ws1 = manager.createWorkspace()
        let ws2 = manager.createWorkspace()
        let state = WindowState(workspaceIDs: [ws1.id, ws2.id], selectedTabIndex: 1)
        manager.saveWindowState(state)

        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.id, state.id)
        XCTAssertEqual(restored.first?.workspaceIDs.count, 2)
    }

    func test_claimWindowState_returnsDistinctStates_forMultipleClaims() throws {
        let dir1 = try makeSeededWorkspaceDirectory(prefix: "claim-window-state-a")
        let dir2 = try makeSeededWorkspaceDirectory(prefix: "claim-window-state-b")
        defer {
            RalphCoreTestSupport.assertRemoved(dir1)
            RalphCoreTestSupport.assertRemoved(dir2)
        }

        let ws1 = manager.createWorkspace(workingDirectory: dir1)
        let ws2 = manager.createWorkspace(workingDirectory: dir2)
        let state1 = WindowState(workspaceIDs: [ws1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [ws2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let firstClaim = manager.claimWindowState(preferredID: nil)
        let secondClaim = manager.claimWindowState(preferredID: nil)

        XCTAssertNotEqual(firstClaim.id, secondClaim.id)
        XCTAssertTrue([state1.id, state2.id].contains(firstClaim.id))
        XCTAssertTrue([state1.id, state2.id].contains(secondClaim.id))
    }

    func test_claimWindowState_prefersProvidedID_whenAvailable() throws {
        let dir1 = try makeSeededWorkspaceDirectory(prefix: "claim-preferred-a")
        let dir2 = try makeSeededWorkspaceDirectory(prefix: "claim-preferred-b")
        defer {
            RalphCoreTestSupport.assertRemoved(dir1)
            RalphCoreTestSupport.assertRemoved(dir2)
        }

        let ws1 = manager.createWorkspace(workingDirectory: dir1)
        let ws2 = manager.createWorkspace(workingDirectory: dir2)
        let state1 = WindowState(workspaceIDs: [ws1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [ws2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let claimed = manager.claimWindowState(preferredID: state2.id)

        XCTAssertEqual(claimed.id, state2.id)
    }

    func test_claimWindowState_withSamePreferredIDTwice_returnsUniqueStates() throws {
        let dir1 = try makeSeededWorkspaceDirectory(prefix: "claim-same-preferred-a")
        let dir2 = try makeSeededWorkspaceDirectory(prefix: "claim-same-preferred-b")
        defer {
            RalphCoreTestSupport.assertRemoved(dir1)
            RalphCoreTestSupport.assertRemoved(dir2)
        }

        let ws1 = manager.createWorkspace(workingDirectory: dir1)
        let ws2 = manager.createWorkspace(workingDirectory: dir2)
        let state1 = WindowState(workspaceIDs: [ws1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [ws2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let firstClaim = manager.claimWindowState(preferredID: state1.id)
        let secondClaim = manager.claimWindowState(preferredID: state1.id)

        XCTAssertEqual(firstClaim.id, state1.id)
        XCTAssertNotEqual(secondClaim.id, state1.id)
    }

    func test_prepareForLaunch_prunesUITestingWorkspaceStateFromProductionDefaults() throws {
        let defaults = UserDefaults.standard
        let workspaceID = UUID()
        let navigationKey = "com.mitchfultz.ralph.navigationState.\(workspaceID.uuidString)"
        let snapshotKey = workspaceSnapshotKey(for: workspaceID)
        let cachedTasksKey = "com.mitchfultz.ralph.workspace.\(workspaceID.uuidString).cachedTasks"
        let tempUITestPath = RalphCoreTestSupport.workspaceURL(label: "ui-test-state-prune")
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

        RalphAppDefaults.prepareForLaunch()

        XCTAssertNil(defaults.object(forKey: snapshotKey))
        XCTAssertNil(defaults.object(forKey: cachedTasksKey))
        XCTAssertNil(defaults.object(forKey: navigationKey))
        XCTAssertTrue(manager.loadAllWindowStates().isEmpty)
    }

    func test_workspaceWorkingDirectory_withCorruptSnapshot_surfacesPersistenceIssue() {
        let workspaceID = UUID()
        defaults.set(Data("not-json".utf8), forKey: workspaceSnapshotKey(for: workspaceID))

        let resolved = manager.workspaceWorkingDirectory(workspaceID)

        XCTAssertEqual(resolved, FileManager.default.homeDirectoryForCurrentUser)
        XCTAssertEqual(manager.persistenceIssue?.domain, .workspaceState)
        XCTAssertEqual(manager.persistenceIssue?.operation, .load)
    }

    // MARK: - Codable Tests

    func test_windowState_encodeDecode_preservesAllFields() throws {
        let ws1 = UUID()
        let ws2 = UUID()
        let originalState = WindowState(
            id: UUID(),
            workspaceIDs: [ws1, ws2],
            selectedTabIndex: 1,
            version: 1
        )

        let data = try JSONEncoder().encode(originalState)
        let decodedState = try JSONDecoder().decode(WindowState.self, from: data)

        XCTAssertEqual(decodedState.id, originalState.id)
        XCTAssertEqual(decodedState.workspaceIDs, originalState.workspaceIDs)
        XCTAssertEqual(decodedState.selectedTabIndex, originalState.selectedTabIndex)
        XCTAssertEqual(decodedState.version, originalState.version)
    }

    func test_windowState_equatable() {
        let id = UUID()
        let ws1 = UUID()
        let ws2 = UUID()

        let state1 = WindowState(id: id, workspaceIDs: [ws1, ws2], selectedTabIndex: 1)
        let state2 = WindowState(id: id, workspaceIDs: [ws1, ws2], selectedTabIndex: 1)
        let state3 = WindowState(id: UUID(), workspaceIDs: [ws1, ws2], selectedTabIndex: 1)
        let state4 = WindowState(id: id, workspaceIDs: [ws1, ws2], selectedTabIndex: 0)

        XCTAssertEqual(state1, state2)
        XCTAssertNotEqual(state1, state3)
        XCTAssertNotEqual(state1, state4)
    }

    // MARK: - Scene Routing Tests

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

        let firstWorkspace = manager.createWorkspace(workingDirectory: firstDirectory)
        let secondWorkspace = manager.createWorkspace(workingDirectory: secondDirectory)
        let thirdWorkspace = manager.createWorkspace(workingDirectory: thirdDirectory)
        _ = firstWorkspace

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
