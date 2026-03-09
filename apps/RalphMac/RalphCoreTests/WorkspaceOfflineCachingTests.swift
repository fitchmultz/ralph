/**
 WorkspaceOfflineCachingTests

 Responsibilities:
 - Validate workspace offline-banner and cached-task presentation behavior.

 Does not handle:
 - CLI health probe execution or recovery category formatting.

 Invariants/assumptions callers must respect:
 - Tests mutate in-memory workspace state only.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceOfflineCachingTests: XCTestCase {
    func testShowOfflineBannerWhenUnavailable() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        XCTAssertFalse(workspace.showOfflineBanner)

        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )

        XCTAssertTrue(workspace.showOfflineBanner)
    }

    func testShowOfflineBannerWhenAvailable() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        XCTAssertFalse(workspace.showOfflineBanner)
    }

    func testIsShowingCachedTasks() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        XCTAssertFalse(workspace.isShowingCachedTasks)

        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]

        XCTAssertTrue(workspace.isShowingCachedTasks)
    }

    func testDisplayTasksWhenOffline() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)
        let cachedTask = RalphTask(id: "RQ-CACHED", status: .done, title: "Cached", priority: .low)

        workspace.tasks = [onlineTask]
        workspace.cachedTasks = [cachedTask]
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )

        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-CACHED")
    }

    func testDisplayTasksWhenOnline() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)

        workspace.tasks = [onlineTask]
        workspace.cachedTasks = []
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )

        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-ONLINE")
    }

    func testClearCachedTasks() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]

        workspace.clearCachedTasks()
        XCTAssertTrue(workspace.cachedTasks.isEmpty)
    }
}
