/**
 WorkspaceTaskPresentationTests

 Responsibilities:
 - Validate deterministic task filtering and ordering snapshots.
 - Cover strict ascending/descending sorting semantics with tied primary keys.
 - Ensure grouped kanban data preserves the same ordered task projection.

 Does not handle:
 - Queue file decoding or file-watcher behavior.
 - UI rendering or SwiftUI selection.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceTaskPresentationTests: RalphCoreTestCase {
    func testTaskPresentation_prioritySortIsDeterministicAscendingAndDescending() {
        let workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "task-presentation-priority"))
        let timestamp = Date(timeIntervalSince1970: 1_700_000_000)

        workspace.taskState.tasks = [
            RalphTask(id: "RQ-003", status: .todo, title: "Same", priority: .high, tags: [], createdAt: timestamp, updatedAt: timestamp),
            RalphTask(id: "RQ-001", status: .todo, title: "Same", priority: .high, tags: [], createdAt: timestamp, updatedAt: timestamp),
            RalphTask(id: "RQ-002", status: .todo, title: "Same", priority: .high, tags: [], createdAt: timestamp, updatedAt: timestamp),
        ]
        workspace.taskState.taskSortBy = .priority

        workspace.taskState.taskSortAscending = true
        XCTAssertEqual(
            workspace.taskPresentation().orderedTaskIDs,
            ["RQ-001", "RQ-002", "RQ-003"]
        )

        workspace.taskState.taskSortAscending = false
        XCTAssertEqual(
            workspace.taskPresentation().orderedTaskIDs,
            ["RQ-003", "RQ-002", "RQ-001"]
        )
    }

    func testTaskPresentation_groupsTasksByStatusWithoutReorderingColumns() {
        let workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "task-presentation-grouping"))
        workspace.taskState.tasks = [
            RalphTask(id: "RQ-010", status: .doing, title: "Doing B", priority: .medium, tags: [], createdAt: Date(timeIntervalSince1970: 20), updatedAt: Date(timeIntervalSince1970: 20)),
            RalphTask(id: "RQ-002", status: .todo, title: "Todo A", priority: .medium, tags: [], createdAt: Date(timeIntervalSince1970: 10), updatedAt: Date(timeIntervalSince1970: 10)),
            RalphTask(id: "RQ-011", status: .doing, title: "Doing A", priority: .medium, tags: [], createdAt: Date(timeIntervalSince1970: 15), updatedAt: Date(timeIntervalSince1970: 15)),
        ]
        workspace.taskState.taskSortBy = .title
        workspace.taskState.taskSortAscending = true

        let presentation = workspace.taskPresentation()

        XCTAssertEqual(presentation.orderedTaskIDs, ["RQ-011", "RQ-010", "RQ-002"])
        XCTAssertEqual(presentation.tasksByStatus[.doing]?.map(\.id), ["RQ-011", "RQ-010"])
        XCTAssertEqual(presentation.tasksByStatus[.todo]?.map(\.id), ["RQ-002"])
    }

    func testGraphViewportState_magnificationUsesGestureBaselineInsteadOfCompounding() {
        var viewport = GraphViewportState()

        viewport.beginMagnificationGesture()
        viewport.updateMagnification(1.1)
        XCTAssertEqual(viewport.scale, 1.1, accuracy: 0.0001)

        viewport.updateMagnification(1.2)
        XCTAssertEqual(viewport.scale, 1.2, accuracy: 0.0001)

        viewport.endMagnificationGesture()
        XCTAssertEqual(viewport.committedScale, 1.2, accuracy: 0.0001)
    }

    func testGraphViewportState_sequentialGesturesAccumulateFromCommittedScale() {
        var viewport = GraphViewportState()

        viewport.beginMagnificationGesture()
        viewport.updateMagnification(1.5)
        viewport.endMagnificationGesture()

        viewport.beginMagnificationGesture()
        viewport.updateMagnification(1.2)
        viewport.endMagnificationGesture()

        XCTAssertEqual(viewport.scale, 1.8, accuracy: 0.0001)
        XCTAssertEqual(viewport.committedScale, 1.8, accuracy: 0.0001)
    }

    func testGraphViewportState_clampsZoomAndResets() {
        var viewport = GraphViewportState(scale: 2.9, committedScale: 2.9)

        viewport.zoomIn()
        XCTAssertEqual(viewport.scale, GraphViewportState.maximumScale, accuracy: 0.0001)

        viewport.zoomOut()
        XCTAssertEqual(viewport.scale, 2.5, accuracy: 0.05)

        viewport.beginMagnificationGesture()
        viewport.updateMagnification(0.01)
        viewport.endMagnificationGesture()
        XCTAssertEqual(viewport.scale, GraphViewportState.minimumScale, accuracy: 0.0001)

        viewport.offset = CGSize(width: 24, height: -12)
        viewport.reset()
        XCTAssertEqual(viewport.scale, 1.0, accuracy: 0.0001)
        XCTAssertEqual(viewport.committedScale, 1.0, accuracy: 0.0001)
        XCTAssertEqual(viewport.offset, .zero)
    }
}
