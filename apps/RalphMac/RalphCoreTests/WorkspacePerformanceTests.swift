/**
 WorkspacePerformanceTests

 Responsibilities:
 - Provide shared setup, synthetic fixtures, and helper utilities for split workspace regression suites.
 - Keep expensive fixture-generation logic centralized across workspace-focused test files.

 Does not handle:
 - Defining the individual performance and regression assertions themselves.

 Invariants/assumptions callers must respect:
 - Callers inherit from `WorkspacePerformanceTestCase` when they need a fresh main-actor workspace.
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

enum WorkspacePerformanceTestSupport {
    static func makeTempDir(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }

    static func makeExecutableScript(in directory: URL, name: String, body: String) throws -> URL {
        try RalphMockCLITestSupport.makeExecutableScript(in: directory, name: name, body: body)
    }

    static func makeVersionAwareMockCLI(in directory: URL, name: String) throws -> URL {
        try RalphMockCLITestSupport.makeVersionAwareMockCLI(in: directory, name: name)
    }

    static func writeEmptyQueueFile(in workspaceDir: URL) throws {
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceDir, tasks: [])
    }

    static func writeQueueFile(in workspaceDir: URL, tasksJSON: String) throws {
        let data = Data(tasksJSON.utf8)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let tasks = try decoder.decode([RalphTask].self, from: data)
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceDir, tasks: tasks)
    }

    static func waitFor(
        timeout: TimeInterval,
        pollInterval: Duration = .milliseconds(50),
        condition: @escaping @MainActor () -> Bool
    ) async -> Bool {
        await RalphCoreTestSupport.waitUntil(
            timeout: .seconds(timeout),
            pollInterval: pollInterval
        ) {
            await MainActor.run { condition() }
        }
    }
}
