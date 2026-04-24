/**
 WorkspacePerformanceTestCase

 Purpose:
 - Provide shared setup and synthetic task generators for split workspace regression suites.

 Responsibilities:
 - Provide shared setup and synthetic task generators for split workspace regression suites.
 - Keep a fresh main-actor workspace available for performance-oriented tests.

 Does not handle:
 - Defining the individual regression assertions themselves.
 - Mock CLI fixture factories beyond the shared workspace shell.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Subclasses may assume `workspace` starts fresh for each test.
 - Teardown always shuts down the owned workspace before returning.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
class WorkspacePerformanceTestCase: RalphCoreTestCase {
    var workspace: Workspace!

    override func setUp() async throws {
        try await super.setUp()
        workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(
                label: "\(type(of: self)).\(name)"
            )
        )
    }

    override func tearDown() async throws {
        workspace?.shutdown()
        workspace = nil
        try await super.tearDown()
    }

    func generateTasks(count: Int) -> [RalphTask] {
        (1...count).map { index in
            RalphTask(
                id: String(format: "RQ-%04d", index),
                status: index % 5 == 0 ? .done : .todo,
                title: "Task \(index)",
                description: "Description for task \(index)",
                priority: [.low, .medium, .high, .critical][index % 4],
                tags: ["tag\(index % 5)", "tag\(index % 3)"],
                createdAt: Date().addingTimeInterval(-Double(index * 3600)),
                updatedAt: Date()
            )
        }
    }

    func generateTasks(count: Int, mutateFrom base: [RalphTask]) -> [RalphTask] {
        base.enumerated().map { index, task in
            if index.isMultiple(of: 10) {
                return RalphTask(
                    id: task.id,
                    status: task.status == .todo ? .doing : .todo,
                    title: task.title + " (modified)",
                    description: task.description,
                    priority: task.priority,
                    tags: task.tags,
                    scope: task.scope,
                    evidence: task.evidence,
                    plan: task.plan,
                    notes: task.notes,
                    request: task.request,
                    createdAt: task.createdAt,
                    updatedAt: Date(),
                    startedAt: task.startedAt,
                    completedAt: task.completedAt,
                    estimatedMinutes: task.estimatedMinutes,
                    actualMinutes: task.actualMinutes,
                    dependsOn: task.dependsOn,
                    blocks: task.blocks,
                    relatesTo: task.relatesTo,
                    customFields: task.customFields
                )
            }
            return task
        }
    }

    func generateTasksWithDependencies(count: Int) -> [RalphTask] {
        (1...count).map { index in
            let dependsOn: [String]?
            if index > 10 {
                dependsOn = (1...min(3, index - 1)).map { "RQ-\(index - $0)" }
            } else {
                dependsOn = nil
            }

            return RalphTask(
                id: String(format: "RQ-%04d", index),
                status: index % 3 == 0 ? .done : .todo,
                title: "Task \(index)",
                priority: .medium,
                dependsOn: dependsOn
            )
        }
    }
}
