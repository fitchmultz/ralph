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
    func test_navigationViewModel_saveAndLoad_roundTrip() async throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectedSection = .quickActions
        viewModel.taskViewMode = .kanban
        viewModel.selectedTaskID = "RQ-0001"

        await assertEventuallyPersistedState(
            workspaceID: workspaceID,
            selectedSection: .quickActions,
            taskViewMode: .kanban,
            selectedTaskID: "RQ-0001"
        )
    }

    @MainActor
    func test_navigationViewModel_differentWorkspaces_haveSeparateState() async throws {
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

        await assertEventuallyPersistedState(
            workspaceID: workspace1ID,
            selectedSection: .queue,
            taskViewMode: .list,
            selectedTaskID: "RQ-0001"
        )

        await assertEventuallyPersistedState(
            workspaceID: workspace2ID,
            selectedSection: .quickActions,
            taskViewMode: .kanban,
            selectedTaskID: "RQ-0002"
        )
    }

    @MainActor
    func test_navigationViewModel_noWorkspaceID_usesGenericState() async throws {
        let viewModel = NavigationViewModel(workspaceID: nil)
        viewModel.selectedSection = .advancedRunner
        viewModel.taskViewMode = .graph

        await assertEventuallyPersistedState(
            workspaceID: nil,
            selectedSection: .advancedRunner,
            taskViewMode: .graph,
            selectedTaskID: nil
        )
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
    func test_navigationViewModel_stateChange_triggersSave() async throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectedSection = .analytics

        await assertEventuallyPersistedState(
            workspaceID: workspaceID,
            selectedSection: .analytics,
            taskViewMode: .list,
            selectedTaskID: nil
        )
    }

    @MainActor
    func test_navigationViewModel_taskSelectionChange_triggersSave() async throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.selectTask("RQ-5678")

        await assertEventuallyPersistedState(
            workspaceID: workspaceID,
            selectedSection: .queue,
            taskViewMode: .list,
            selectedTaskID: "RQ-5678"
        )
    }

    @MainActor
    func test_navigationViewModel_taskViewModeChange_triggersSave() async throws {
        let workspaceID = UUID()
        let viewModel = NavigationViewModel(workspaceID: workspaceID)
        viewModel.setTaskViewMode(.graph)

        await assertEventuallyPersistedState(
            workspaceID: workspaceID,
            selectedSection: .queue,
            taskViewMode: .graph,
            selectedTaskID: nil
        )
    }

    @MainActor
    func test_navigationViewModel_saveFailure_surfacesPersistenceIssue() async {
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

        let surfacedIssue = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            viewModel.persistenceIssue?.domain == .navigationState &&
                viewModel.persistenceIssue?.operation == .save &&
                forwardedIssue?.domain == .navigationState &&
                forwardedIssue?.operation == .save
        }
        XCTAssertTrue(surfacedIssue)

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

    @MainActor
    private func assertEventuallyPersistedState(
        workspaceID: UUID?,
        selectedSection: SidebarSection,
        taskViewMode: TaskViewMode,
        selectedTaskID: String?,
        timeout: TimeInterval = 2.0,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        let persisted = await WorkspacePerformanceTestSupport.waitFor(timeout: timeout) {
            let loadedViewModel = NavigationViewModel(workspaceID: workspaceID)
            return loadedViewModel.selectedSection == selectedSection &&
                loadedViewModel.taskViewMode == taskViewMode &&
                loadedViewModel.selectedTaskID == selectedTaskID
        }

        XCTAssertTrue(
            persisted,
            "Timed out waiting for navigation state persistence (\(workspaceID?.uuidString ?? "global"))",
            file: file,
            line: line
        )
    }
}

#endif
