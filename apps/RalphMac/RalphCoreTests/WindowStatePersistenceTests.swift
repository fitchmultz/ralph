/**
 WindowStatePersistenceTests

 Purpose:
 - Validate WindowState model encoding, selection validation, and persistence through WorkspaceManager.

 Responsibilities:
 - Validate WindowState model encoding, selection validation, and persistence through WorkspaceManager.
 - Cover workspace snapshot persistence and corrupt-snapshot recovery surfaces.

 Does not handle:
 - Window restoration selection flows.
 - Scene routing and effective-workspace behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Persistence behavior is exercised through the shared WindowStateTestCase environment.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class WindowStatePersistenceTests: WindowStateTestCase {
    func test_saveAndLoadWindowState_roundTrip() {
        let workspace1 = manager.createWorkspace()
        let workspace2 = manager.createWorkspace()
        let windowState = WindowState(workspaceIDs: [workspace1.id, workspace2.id], selectedTabIndex: 1)

        manager.saveWindowState(windowState)

        let loadedStates = manager.loadAllWindowStates()
        XCTAssertEqual(loadedStates.count, 1)
        XCTAssertEqual(loadedStates.first?.id, windowState.id)
        XCTAssertEqual(loadedStates.first?.workspaceIDs, windowState.workspaceIDs)
        XCTAssertEqual(loadedStates.first?.selectedTabIndex, 1)
        XCTAssertEqual(loadedStates.first?.version, 1)
    }

    func test_loadAllWindowStates_withNoSavedState_returnsEmpty() {
        XCTAssertTrue(manager.loadAllWindowStates().isEmpty)
    }

    func test_removeWindowState_removesCorrectState() {
        let workspace1 = manager.createWorkspace()
        let workspace2 = manager.createWorkspace()
        let state1 = WindowState(workspaceIDs: [workspace1.id])
        let state2 = WindowState(workspaceIDs: [workspace2.id])

        manager.saveWindowState(state1)
        manager.saveWindowState(state2)
        manager.removeWindowState(state1.id)

        let loadedStates = manager.loadAllWindowStates()
        XCTAssertEqual(loadedStates.count, 1)
        XCTAssertEqual(loadedStates.first?.id, state2.id)
    }

    func test_saveWindowState_overwritesExistingState() {
        let workspace = manager.createWorkspace()
        let originalState = WindowState(workspaceIDs: [workspace.id], selectedTabIndex: 0)
        manager.saveWindowState(originalState)

        let secondWorkspace = manager.createWorkspace()
        var modifiedState = originalState
        modifiedState.workspaceIDs.append(secondWorkspace.id)
        modifiedState.selectedTabIndex = 1
        manager.saveWindowState(modifiedState)

        let loadedStates = manager.loadAllWindowStates()
        XCTAssertEqual(loadedStates.count, 1)
        XCTAssertEqual(loadedStates.first?.workspaceIDs.count, 2)
        XCTAssertEqual(loadedStates.first?.selectedTabIndex, 1)
    }

    func test_windowState_versionField_defaultsToOne() {
        let workspace = manager.createWorkspace()
        XCTAssertEqual(WindowState(workspaceIDs: [workspace.id]).version, 1)
    }

    func test_windowState_validateSelection_withEmptyWorkspaces() {
        var state = WindowState(workspaceIDs: [], selectedTabIndex: 5)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 0)
    }

    func test_windowState_validateSelection_withOutOfBoundsIndex() {
        let workspace1 = manager.createWorkspace()
        let workspace2 = manager.createWorkspace()
        var state = WindowState(workspaceIDs: [workspace1.id, workspace2.id], selectedTabIndex: 10)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 1)
    }

    func test_windowState_validateSelection_withNegativeIndex() {
        let workspace = manager.createWorkspace()
        var state = WindowState(workspaceIDs: [workspace.id], selectedTabIndex: -5)
        state.validateSelection()
        XCTAssertEqual(state.selectedTabIndex, 0)
    }

    func test_createWorkspace_persistsInitialWorkingDirectory() throws {
        let temp = try makeWorkspaceDirectory(prefix: "persist-working-directory")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let snapshotData = try XCTUnwrap(defaults.data(forKey: workspaceSnapshotKey(for: workspace.id)))
        let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: snapshotData)

        XCTAssertEqual(snapshot.workingDirectoryURL, temp)
        XCTAssertEqual(snapshot.name, temp.lastPathComponent)
    }

    func test_createWorkspace_backfillsWorkingDirectoryBookmarkDataWhenAvailable() throws {
        let temp = try makeWorkspaceDirectory(prefix: "persist-working-directory-bookmark")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        guard Workspace.securityScopedBookmarkData(for: temp) != nil else {
            throw XCTSkip("Security-scoped bookmarks unavailable in this test environment")
        }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let snapshotData = try XCTUnwrap(defaults.data(forKey: workspaceSnapshotKey(for: workspace.id)))
        let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: snapshotData)

        XCTAssertNotNil(workspace.identityState.workingDirectoryBookmarkData)
        XCTAssertEqual(
            workspace.identityState.recentWorkingDirectoryBookmarks[temp.path],
            workspace.identityState.workingDirectoryBookmarkData
        )
        XCTAssertEqual(snapshot.workingDirectoryBookmarkData, workspace.identityState.workingDirectoryBookmarkData)
        XCTAssertEqual(
            snapshot.recentWorkingDirectoryBookmarks?[temp.path],
            workspace.identityState.workingDirectoryBookmarkData
        )
    }

    func test_workspaceSnapshotDecode_acceptsLegacySnapshotsWithoutBookmarks() throws {
        let data = """
            {
              "name": "Legacy",
              "workingDirectoryURL": "file:///tmp/ralph-legacy/",
              "recentWorkingDirectories": []
            }
            """.data(using: .utf8)!

        let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: data)

        XCTAssertEqual(snapshot.name, "Legacy")
        XCTAssertEqual(snapshot.workingDirectoryURL.path, "/tmp/ralph-legacy")
        XCTAssertNil(snapshot.workingDirectoryBookmarkData)
        XCTAssertNil(snapshot.recentWorkingDirectoryBookmarks)
    }

    func test_persistState_preservesWorkspaceDirectoryBookmarkData() throws {
        let temp = try makeWorkspaceDirectory(prefix: "persist-bookmark-data")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let bookmarkData = Data("bookmark".utf8)
        workspace.identityState.workingDirectoryBookmarkData = bookmarkData
        workspace.identityState.recentWorkingDirectoryBookmarks = [temp.path: bookmarkData]
        workspace.persistState()

        let snapshotData = try XCTUnwrap(defaults.data(forKey: workspaceSnapshotKey(for: workspace.id)))
        let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: snapshotData)

        XCTAssertEqual(snapshot.workingDirectoryBookmarkData, bookmarkData)
        XCTAssertEqual(snapshot.recentWorkingDirectoryBookmarks?[temp.path], bookmarkData)
    }

    func test_loadState_backfillsLegacyWorkspaceBookmarkDataWhenAvailable() throws {
        let temp = try makeWorkspaceDirectory(prefix: "legacy-bookmark-backfill")
        defer { RalphCoreTestSupport.assertRemoved(temp) }

        guard Workspace.securityScopedBookmarkData(for: temp) != nil else {
            throw XCTSkip("Security-scoped bookmarks unavailable in this test environment")
        }

        let workspaceID = UUID()
        let snapshot = RalphWorkspaceDefaultsSnapshot(
            name: temp.lastPathComponent,
            workingDirectoryURL: temp,
            recentWorkingDirectories: []
        )
        let snapshotData = try JSONEncoder().encode(snapshot)
        defaults.set(snapshotData, forKey: workspaceSnapshotKey(for: workspaceID))

        let workspace = Workspace(id: workspaceID, workingDirectoryURL: temp)

        XCTAssertNotNil(workspace.identityState.workingDirectoryBookmarkData)
        XCTAssertEqual(
            workspace.identityState.recentWorkingDirectoryBookmarks[temp.path],
            workspace.identityState.workingDirectoryBookmarkData
        )

        let persistedData = try XCTUnwrap(defaults.data(forKey: workspaceSnapshotKey(for: workspaceID)))
        let persistedSnapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: persistedData)
        XCTAssertEqual(
            persistedSnapshot.workingDirectoryBookmarkData,
            workspace.identityState.workingDirectoryBookmarkData
        )
        XCTAssertEqual(
            persistedSnapshot.recentWorkingDirectoryBookmarks?[temp.path],
            workspace.identityState.workingDirectoryBookmarkData
        )

        workspace.shutdown()
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

    func test_workspaceWorkingDirectory_withCorruptSnapshot_surfacesPersistenceIssue() {
        let workspaceID = UUID()
        defaults.set(Data("not-json".utf8), forKey: workspaceSnapshotKey(for: workspaceID))

        let resolved = manager.workspaceWorkingDirectory(workspaceID)

        XCTAssertNil(resolved)
        XCTAssertEqual(manager.persistenceIssue?.domain, .workspaceState)
        XCTAssertEqual(manager.persistenceIssue?.operation, .load)
    }

    func test_windowState_encodeDecode_preservesAllFields() throws {
        let workspace1 = UUID()
        let workspace2 = UUID()
        let originalState = WindowState(
            id: UUID(),
            workspaceIDs: [workspace1, workspace2],
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
        let workspace1 = UUID()
        let workspace2 = UUID()

        let state1 = WindowState(id: id, workspaceIDs: [workspace1, workspace2], selectedTabIndex: 1)
        let state2 = WindowState(id: id, workspaceIDs: [workspace1, workspace2], selectedTabIndex: 1)
        let state3 = WindowState(id: UUID(), workspaceIDs: [workspace1, workspace2], selectedTabIndex: 1)
        let state4 = WindowState(id: id, workspaceIDs: [workspace1, workspace2], selectedTabIndex: 0)

        XCTAssertEqual(state1, state2)
        XCTAssertNotEqual(state1, state3)
        XCTAssertNotEqual(state1, state4)
    }
}
