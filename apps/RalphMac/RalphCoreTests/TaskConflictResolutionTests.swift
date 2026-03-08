/**
 TaskConflictResolutionTests

 Responsibilities:
 - Verify conflict field grouping and initial selections for merge UI.
 - Protect merge application logic after moving it out of SwiftUI.
 - Ensure agent override conflicts stay part of the shared model.
 */

import Foundation
import XCTest

@testable import RalphCore

final class TaskConflictResolutionTests: XCTestCase {
    func testModelBuildsSectionsAndInitialSelectionsFromConflicts() {
        let local = RalphTask(
            id: "RQ-1",
            status: .doing,
            title: "Local",
            description: "Shared description",
            priority: .high,
            tags: ["swift"],
            updatedAt: Date(),
            dependsOn: ["RQ-0"]
        )
        let external = RalphTask(
            id: "RQ-1",
            status: .todo,
            title: "External",
            description: "Shared description",
            priority: .medium,
            tags: ["rust"],
            updatedAt: Date(),
            dependsOn: ["RQ-2"]
        )

        let model = TaskConflictResolutionModel(localTask: local, externalTask: external)

        XCTAssertEqual(
            Set(model.sections.map { $0.section }),
            Set<TaskConflictFieldSection>([.basicInformation, .tags, .relationships])
        )
        XCTAssertEqual(model.initialSelections[TaskConflictField.title], TaskConflictMergeChoice.external)
        XCTAssertEqual(model.initialSelections[TaskConflictField.status], TaskConflictMergeChoice.external)
        XCTAssertEqual(model.initialSelections[TaskConflictField.tags], TaskConflictMergeChoice.external)
        XCTAssertEqual(model.initialSelections[TaskConflictField.dependsOn], TaskConflictMergeChoice.external)
        XCTAssertNil(model.initialSelections[TaskConflictField.description])
    }

    func testApplySelectionsUsesExternalAsBaseAndOptsIntoLocalFields() {
        let local = RalphTask(
            id: "RQ-2",
            status: .doing,
            title: "Local title",
            priority: .high,
            tags: ["swift"],
            agent: RalphTaskAgent(runner: "codex", model: "gpt-5.4"),
            updatedAt: Date()
        )
        let external = RalphTask(
            id: "RQ-2",
            status: .todo,
            title: "External title",
            priority: .medium,
            tags: ["rust"],
            agent: RalphTaskAgent(runner: "claude", model: "sonnet"),
            updatedAt: Date()
        )

        let merged = TaskConflictResolutionModel.applySelections(
            localTask: local,
            externalTask: external,
            selections: [
                TaskConflictField.title: TaskConflictMergeChoice.local,
                TaskConflictField.agent: TaskConflictMergeChoice.local
            ]
        )

        XCTAssertEqual(merged.title, "Local title")
        XCTAssertEqual(merged.agent?.runner, "codex")
        XCTAssertEqual(merged.status, .todo)
        XCTAssertEqual(merged.priority, .medium)
        XCTAssertEqual(merged.tags, ["rust"])
    }
}
