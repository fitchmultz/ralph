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

public import Foundation
public import XCTest
@testable import RalphCore

@MainActor
final class WindowStateTests: XCTestCase {
    private var manager: WorkspaceManager!
    private let testRestorationKey = "com.mitchfultz.ralph.windowRestorationState"
    private let testNavigationKey = "com.mitchfultz.ralph.navigationState"

    override func setUp() {
        super.setUp()
        manager = WorkspaceManager.shared
        // Clear any existing test state
        UserDefaults.standard.removeObject(forKey: testRestorationKey)
        cleanupNavigationState()
    }

    override func tearDown() {
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
