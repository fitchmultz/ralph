/**
 WorkspaceTaskCreationIntegrationTests

 Purpose:
 - Verify Workspace task creation uses the deterministic direct-create path against a real CLI-backed workspace.

 Responsibilities:
 - Verify Workspace task creation uses the deterministic direct-create path against a real CLI-backed workspace.

 Does not handle:
 - Queue watcher edge cases.
 - Retargeting/refresh coverage across multiple workspaces.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests initialize isolated temp workspaces with `ralph init --non-interactive` before exercising task creation.
 */

import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceTaskCreationIntegrationTests: RalphCoreTestCase {
    func test_createTask_importsStructuredTaskImmediately() async throws {
        var workspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-create-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, workspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        try await workspace.createTask(
            title: "UI-created task",
            description: "Created without invoking task builder",
            priority: .high,
            tags: ["ui", "direct-create"],
            scope: ["apps/RalphMac/RalphMac/TaskCreationView.swift"]
        )

        let loadedCreatedTask = await RalphCoreTestSupport.waitUntil(timeout: .seconds(5)) {
            await MainActor.run {
                workspace.taskState.tasks.count == 1
            }
        }
        XCTAssertTrue(loadedCreatedTask)

        let tasks = workspace.taskState.tasks
        XCTAssertEqual(tasks.count, 1)
        let task = try XCTUnwrap(tasks.first)
        XCTAssertEqual(task.title, "UI-created task")
        XCTAssertEqual(task.description, "Created without invoking task builder")
        XCTAssertEqual(task.priority, .high)
        XCTAssertEqual(task.tags, ["ui", "direct-create"])
        XCTAssertEqual(task.scope, ["apps/RalphMac/RalphMac/TaskCreationView.swift"])
        XCTAssertEqual(task.status, .todo)
    }
}
