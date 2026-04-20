/**
 WorkspaceQueueRefreshTests

 Responsibilities:
 - Validate workspace queue refresh, analytics refresh, watcher refresh, and retargeting behavior.

 Does not handle:
 - Low-level file-watcher retry behavior.
 - Direct task-creation path coverage.

 Invariants/assumptions callers must respect:
 - Tests initialize isolated temp workspaces and rely on deterministic queue/analytics convergence checks.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceQueueRefreshTests: RalphCoreTestCase {
    func test_workspaceInitialRefresh_populatesQueueWithoutEagerGraphOrAnalytics() async throws {
        var seedingWorkspace: Workspace!
        var workspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-initial-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, seedingWorkspace, workspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        seedingWorkspace = Workspace(workingDirectoryURL: workspaceURL, client: client)
        try await seedingWorkspace.createTask(
            title: "Seed queue state",
            priority: .medium
        )
        try await seedingWorkspace.createTask(
            title: "Render analytics state",
            priority: .medium
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loaded = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.count == 2
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }

        XCTAssertTrue(loaded)
        XCTAssertEqual(workspace.taskState.tasks.count, 2)
        XCTAssertNil(workspace.insightsState.graphData)
        XCTAssertNil(workspace.insightsState.analytics.lastRefreshedAt)
    }

    func test_workspaceWatcherExternalMutation_refreshesQueueGraphAndAnalytics() async throws {
        var workspace: Workspace!
        var writerWorkspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-watch-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, workspace, writerWorkspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)
        writerWorkspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loadedEmptyState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
                    && workspace.diagnosticsState.watcherHealth.isWatching
            }
        }

        XCTAssertTrue(loadedEmptyState)

        try await writerWorkspace.createTask(
            title: "Observe watcher refresh",
            priority: .medium
        )
        try await writerWorkspace.createTask(
            title: "Update analytics after mutation",
            priority: .medium
        )

        let refreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.count == 2
                    && workspace.insightsState.graphData?.summary.totalTasks == 2
                    && workspace.insightsState.analytics.queueStatsValue?.summary.active == 2
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        XCTAssertTrue(refreshed)
        XCTAssertEqual(workspace.taskState.lastQueueRefreshEvent?.highlightedTaskIDs.count, 2)
    }

    func test_workspaceWatcherAtomicQueueReplacement_refreshesQueueGraphAndAnalytics() async throws {
        var workspace: Workspace!
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-refresh-replace-")
        defer { RalphCoreTestSupport.shutdownAndRemove(workspaceURL, workspace) }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        let loadedEmptyState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
                    && workspace.diagnosticsState.watcherHealth.isWatching
            }
        }

        XCTAssertTrue(loadedEmptyState)

        let replacementURL = workspaceURL.appendingPathComponent("queue-replacement.jsonc", isDirectory: false)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: replacementURL,
            tasks: [
                RalphTask(
                    id: "RQ-9001",
                    status: .todo,
                    title: "Atomic replace task",
                    priority: .high,
                    tags: ["watcher", "replace"],
                    createdAt: ISO8601DateFormatter().date(from: "2026-03-12T00:00:00Z"),
                    updatedAt: ISO8601DateFormatter().date(from: "2026-03-12T00:00:00Z")
                )
            ]
        )
        defer { XCTAssertNoThrow(try WorkspaceTaskCreationTestSupport.removeItemIfExists(replacementURL)) }

        _ = try FileManager.default.replaceItemAt(
            workspaceURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false),
            withItemAt: replacementURL
        )

        let queueRefreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.map(\.id) == ["RQ-9001"]
                    && workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange
            }
        }

        let queueTaskIDs = await MainActor.run { workspace.taskState.tasks.map { $0.id } }
        let refreshEvent = await MainActor.run { workspace.taskState.lastQueueRefreshEvent }
        XCTAssertTrue(
            queueRefreshed,
            "queue tasks=\(queueTaskIDs) event=\(String(describing: refreshEvent))"
        )

        let derivedViewsRefreshed = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.insightsState.graphData?.summary.totalTasks == 1
                    && workspace.insightsState.analytics.queueStatsValue?.summary.active == 1
            }
        }

        let graphTotalTasks = await MainActor.run { workspace.insightsState.graphData?.summary.totalTasks }
        let analyticsActiveCount = await MainActor.run { workspace.insightsState.analytics.queueStatsValue?.summary.active }
        XCTAssertTrue(
            derivedViewsRefreshed,
            "graph=\(String(describing: graphTotalTasks)) analytics=\(String(describing: analyticsActiveCount))"
        )
    }

    func test_workspaceRetarget_refreshesQueueWithoutEagerGraphOrAnalyticsForNewDirectory() async throws {
        var populatedWorkspace: Workspace!
        var workspace: Workspace!
        let emptyWorkspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-empty-")
        let populatedWorkspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-retarget-populated-")
        defer {
            RalphCoreTestSupport.shutdownAndRemove(emptyWorkspaceURL, workspace)
            RalphCoreTestSupport.shutdownAndRemove(populatedWorkspaceURL, populatedWorkspace)
        }

        let client = try RalphCLIClient(executableURL: try WorkspaceTaskCreationTestSupport.resolveRalphBinaryURL())
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: emptyWorkspaceURL
        )
        try await WorkspaceTaskCreationTestSupport.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: populatedWorkspaceURL
        )

        populatedWorkspace = Workspace(workingDirectoryURL: populatedWorkspaceURL, client: client)
        try await populatedWorkspace.createTask(
            title: "Switch workspace truth",
            priority: .medium
        )

        workspace = Workspace(workingDirectoryURL: emptyWorkspaceURL, client: client)

        let loadedInitialState = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.taskState.tasks.isEmpty
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }
        XCTAssertTrue(loadedInitialState)

        workspace.setWorkingDirectory(populatedWorkspaceURL)

        let retargeted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(10)) {
            await MainActor.run {
                workspace.workingDirectoryURL == Workspace.normalizedWorkingDirectoryURL(populatedWorkspaceURL)
                    && workspace.taskState.tasks.count == 1
                    && workspace.insightsState.graphData == nil
                    && workspace.insightsState.analytics.lastRefreshedAt == nil
            }
        }

        XCTAssertTrue(retargeted)
    }
}
