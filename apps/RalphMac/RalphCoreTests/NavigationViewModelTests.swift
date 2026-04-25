/**
 NavigationViewModelTests

 Purpose:
 - Validate NavigationViewModel persistence via RalphAppDefaults.

 Responsibilities:
 - Validate NavigationViewModel persistence via RalphAppDefaults.
 - Ensure navigation state is correctly saved and restored across app launches.
 - Test that different workspaces keep separate navigation state.

 Does not handle:
 - UI-level navigation interactions.
 - Window routing or focused scene action behavior.
 - Cross-workspace synchronization outside persisted state.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests run on the main actor.
 - Navigation state persistence uses Ralph's isolated app defaults suite.
 - State round-trips should remain versioned and backward-safe.
 */

#if canImport(SwiftUI)

import Foundation
import XCTest
@testable import RalphCore

final class NavigationViewModelTests: RalphCoreTestCase {
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
        let workspaceID = UUID()
        let stateKey = "\(testNavigationKey).\(workspaceID.uuidString)"
        var removedKeys: [String] = []
        let store = NavigationStateStore(
            loadData: { _ in
                try JSONEncoder().encode(
                    NavigationState(
                        version: 999,
                        selectedSection: .quickActions,
                        taskViewMode: .kanban,
                        selectedTaskID: "RQ-9999",
                        selectedTaskIDs: ["RQ-9999"]
                    )
                )
            },
            saveData: { _, _ in },
            removeData: { removedKeys.append($0) }
        )

        let viewModel = NavigationViewModel(workspaceID: workspaceID, store: store)
        XCTAssertEqual(viewModel.selectedSection, .queue)
        XCTAssertEqual(viewModel.taskViewMode, .list)
        XCTAssertNil(viewModel.selectedTaskID)
        XCTAssertEqual(removedKeys, [stateKey])
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

    @MainActor
    func test_navigationViewModel_saveFailure_surfacesPersistenceIssue() {
        struct ExpectedFailure: Error {}

        var forwardedIssue: PersistenceIssue?
        let store = NavigationStateStore(
            loadData: { _ in nil },
            saveData: { _, _ in throw ExpectedFailure() },
            removeData: { _ in }
        )

        let viewModel = NavigationViewModel(
            workspaceID: UUID(),
            store: store,
            issueSink: { forwardedIssue = $0 }
        )
        viewModel.selectedSection = .analytics

        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.1))

        XCTAssertEqual(viewModel.persistenceIssue?.domain, .navigationState)
        XCTAssertEqual(viewModel.persistenceIssue?.operation, .save)
        XCTAssertEqual(forwardedIssue?.domain, .navigationState)
        XCTAssertEqual(forwardedIssue?.operation, .save)
    }

    @MainActor
    func test_navigationViewModel_loadFailure_surfacesIssueAndClearsStoredState() {
        struct ExpectedFailure: Error {}

        let workspaceID = UUID()
        let stateKey = "\(testNavigationKey).\(workspaceID.uuidString)"
        var removedKeys: [String] = []
        var forwardedIssue: PersistenceIssue?
        let store = NavigationStateStore(
            loadData: { _ in throw ExpectedFailure() },
            saveData: { _, _ in },
            removeData: { removedKeys.append($0) }
        )

        let viewModel = NavigationViewModel(
            workspaceID: workspaceID,
            store: store,
            issueSink: { forwardedIssue = $0 }
        )

        XCTAssertEqual(viewModel.selectedSection, .queue)
        XCTAssertEqual(viewModel.taskViewMode, .list)
        XCTAssertNil(viewModel.selectedTaskID)
        XCTAssertEqual(viewModel.persistenceIssue?.domain, .navigationState)
        XCTAssertEqual(viewModel.persistenceIssue?.operation, .load)
        XCTAssertEqual(forwardedIssue?.domain, .navigationState)
        XCTAssertEqual(forwardedIssue?.operation, .load)
        XCTAssertEqual(removedKeys, [stateKey])
    }

    @MainActor
    func test_navigationViewModel_delayedIssueSink_replaysLoadIssue() {
        struct ExpectedFailure: Error {}

        let workspaceID = UUID()
        var forwardedIssue: PersistenceIssue?
        let store = NavigationStateStore(
            loadData: { _ in throw ExpectedFailure() },
            saveData: { _, _ in },
            removeData: { _ in }
        )

        let viewModel = NavigationViewModel(workspaceID: workspaceID, store: store)
        viewModel.setPersistenceIssueSink { forwardedIssue = $0 }

        XCTAssertEqual(viewModel.persistenceIssue?.domain, .navigationState)
        XCTAssertEqual(viewModel.persistenceIssue?.operation, .load)
        XCTAssertEqual(forwardedIssue?.domain, .navigationState)
        XCTAssertEqual(forwardedIssue?.operation, .load)
    }
}

#endif
