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
final class WindowStateTests: XCTestCase {
    private var manager: WorkspaceManager!
    private let testRestorationKey = "com.mitchfultz.ralph.windowRestorationState"
    private let testNavigationKey = "com.mitchfultz.ralph.navigationState"

    override func setUp() {
        super.setUp()
        manager = WorkspaceManager.shared
        manager.resetWindowStateClaimPool()
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
        // Clear any existing test state
        UserDefaults.standard.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
    }

    override func tearDown() {
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
        manager.resetWindowStateClaimPool()
        UserDefaults.standard.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
        super.tearDown()
    }

    private func cleanupNavigationState() {
        let defaults = UserDefaults.standard
        for key in defaults.dictionaryRepresentation().keys {
            if key.hasPrefix(testNavigationKey) {
                defaults.removeObject(forKey: key)
            }
        }
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
        UserDefaults.standard.removeObject(forKey: testRestorationKey)

        let restored = manager.restoreWindows()

        // Should create a default window with new workspace
        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs.count, 1)
    }

    func test_restoreWindows_withNoSavedState_usesExistingWorkspace() {
        UserDefaults.standard.removeObject(forKey: testRestorationKey)
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let restored = manager.restoreWindows()

        XCTAssertEqual(restored.count, 1)
        XCTAssertEqual(restored.first?.workspaceIDs, [workspace.id])
    }

    func test_createWorkspace_persistsInitialWorkingDirectory() {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }

        let workspace = manager.createWorkspace(workingDirectory: temp)
        let key = "com.mitchfultz.ralph.workspace.\(workspace.id.uuidString).workingPath"

        XCTAssertEqual(UserDefaults.standard.string(forKey: key), temp.path)
    }

    func test_workspaceProjectDisplayName_prefersWorkingDirectoryLeafName() throws {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }

        let workspace = Workspace(workingDirectoryURL: temp)
        workspace.name = "RalphMac"

        XCTAssertEqual(workspace.projectDisplayName, temp.lastPathComponent)
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

    func test_claimWindowState_returnsDistinctStates_forMultipleClaims() {
        let dir1 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let dir2 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir1, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.removeItem(at: dir1)
            try? FileManager.default.removeItem(at: dir2)
        }
        try? FileManager.default.createDirectory(at: dir1.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir1.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir2.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )

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

    func test_claimWindowState_prefersProvidedID_whenAvailable() {
        let dir1 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let dir2 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir1, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.removeItem(at: dir1)
            try? FileManager.default.removeItem(at: dir2)
        }
        try? FileManager.default.createDirectory(at: dir1.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir1.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir2.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )

        let ws1 = manager.createWorkspace(workingDirectory: dir1)
        let ws2 = manager.createWorkspace(workingDirectory: dir2)
        let state1 = WindowState(workspaceIDs: [ws1.id], selectedTabIndex: 0)
        let state2 = WindowState(workspaceIDs: [ws2.id], selectedTabIndex: 0)
        manager.saveWindowState(state1)
        manager.saveWindowState(state2)

        let claimed = manager.claimWindowState(preferredID: state2.id)

        XCTAssertEqual(claimed.id, state2.id)
    }

    func test_claimWindowState_withSamePreferredIDTwice_returnsUniqueStates() {
        let dir1 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let dir2 = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir1, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.removeItem(at: dir1)
            try? FileManager.default.removeItem(at: dir2)
        }
        try? FileManager.default.createDirectory(at: dir1.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: dir2.appendingPathComponent(".ralph"), withIntermediateDirectories: true)
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir1.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )
        try? #"{"version":1,"tasks":[]}"#.write(
            to: dir2.appendingPathComponent(".ralph/queue.json"),
            atomically: true,
            encoding: .utf8
        )

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
}
