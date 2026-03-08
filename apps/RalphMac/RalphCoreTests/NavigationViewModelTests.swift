/**
 NavigationViewModelTests

 Responsibilities:
 - Validate NavigationViewModel persistence via RalphAppDefaults.
 - Ensure navigation state is correctly saved and restored across app launches.
 - Test that different workspaces keep separate navigation state.

 Does not handle:
 - UI-level navigation interactions.
 - Window routing or focused scene action behavior.
 - Cross-workspace synchronization outside persisted state.

 Invariants/assumptions callers must respect:
 - Tests run on the main actor.
 - Navigation state persistence uses Ralph's isolated app defaults suite.
 - State round-trips should remain versioned and backward-safe.
 */

#if canImport(SwiftUI)

import Foundation
import XCTest
@testable import RalphCore

final class NavigationViewModelTests: XCTestCase {
    private let testNavigationKey = "com.mitchfultz.ralph.navigationState"

    override func setUp() {
        super.setUp()
        cleanupNavigationState()
    }

    override func tearDown() {
        cleanupNavigationState()
        super.tearDown()
    }

    private func cleanupNavigationState() {
        let defaults = RalphAppDefaults.userDefaults
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(testNavigationKey) {
            defaults.removeObject(forKey: key)
        }
    }

    @MainActor
    func test_navigationViewModel_saveAndLoad_roundTrip() throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectedSection = .quickActions
        viewModel.taskViewMode = .kanban
        viewModel.selectedTaskID = "RQ-0001"

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedViewModel = NavigationViewModel(workspaceID: workspaceID)
        XCTAssertEqual(loadedViewModel.selectedSection, .quickActions)
        XCTAssertEqual(loadedViewModel.taskViewMode, .kanban)
        XCTAssertEqual(loadedViewModel.selectedTaskID, "RQ-0001")
    }

    @MainActor
    func test_navigationViewModel_differentWorkspaces_haveSeparateState() throws {
        let workspace1ID = UUID()
        let workspace2ID = UUID()

        let vm1 = NavigationViewModel(workspaceID: workspace1ID)
        vm1.selectedSection = .queue
        vm1.taskViewMode = .list
        vm1.selectedTaskID = "RQ-0001"

        let vm2 = NavigationViewModel(workspaceID: workspace2ID)
        vm2.selectedSection = .quickActions
        vm2.taskViewMode = .kanban
        vm2.selectedTaskID = "RQ-0002"

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedVM1 = NavigationViewModel(workspaceID: workspace1ID)
        let loadedVM2 = NavigationViewModel(workspaceID: workspace2ID)

        XCTAssertEqual(loadedVM1.selectedSection, .queue)
        XCTAssertEqual(loadedVM1.taskViewMode, .list)
        XCTAssertEqual(loadedVM1.selectedTaskID, "RQ-0001")

        XCTAssertEqual(loadedVM2.selectedSection, .quickActions)
        XCTAssertEqual(loadedVM2.taskViewMode, .kanban)
        XCTAssertEqual(loadedVM2.selectedTaskID, "RQ-0002")
    }

    @MainActor
    func test_navigationViewModel_noWorkspaceID_usesGenericState() throws {
        let viewModel = NavigationViewModel(workspaceID: nil)
        viewModel.selectedSection = .advancedRunner
        viewModel.taskViewMode = .graph

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedViewModel = NavigationViewModel(workspaceID: nil)
        XCTAssertEqual(loadedViewModel.selectedSection, .advancedRunner)
        XCTAssertEqual(loadedViewModel.taskViewMode, .graph)
    }

    @MainActor
    func test_navigationViewModel_versionMismatch_usesDefaults() throws {
        struct InvalidState: Codable {
            let version: Int
            let selectedSection: String
        }

        let workspaceID = UUID()
        let invalidState = InvalidState(version: 999, selectedSection: "queue")
        let data = try JSONEncoder().encode(invalidState)
        RalphAppDefaults.userDefaults.set(data, forKey: "\(testNavigationKey).\(workspaceID.uuidString)")

        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        XCTAssertEqual(viewModel.selectedSection, .queue)
        XCTAssertEqual(viewModel.taskViewMode, .list)
        XCTAssertNil(viewModel.selectedTaskID)
    }

    @MainActor
    func test_navigationViewModel_noSavedState_usesDefaults() {
        let viewModel = NavigationViewModel(workspaceID: UUID())
        XCTAssertEqual(viewModel.selectedSection, .queue)
        XCTAssertEqual(viewModel.taskViewMode, .list)
        XCTAssertNil(viewModel.selectedTaskID)
    }

    @MainActor
    func test_navigationViewModel_stateChange_triggersSave() throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectedSection = .analytics

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedViewModel = NavigationViewModel(workspaceID: workspaceID)
        XCTAssertEqual(loadedViewModel.selectedSection, .analytics)
    }

    @MainActor
    func test_navigationViewModel_taskSelectionChange_triggersSave() throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectTask("RQ-5678")

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedViewModel = NavigationViewModel(workspaceID: workspaceID)
        XCTAssertEqual(loadedViewModel.selectedTaskID, "RQ-5678")
    }

    @MainActor
    func test_navigationViewModel_taskViewModeChange_triggersSave() throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.setTaskViewMode(.graph)

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        let loadedViewModel = NavigationViewModel(workspaceID: workspaceID)
        XCTAssertEqual(loadedViewModel.taskViewMode, .graph)
    }
}

#endif
