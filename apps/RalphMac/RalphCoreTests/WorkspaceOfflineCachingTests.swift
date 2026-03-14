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
final class WorkspaceOfflineCachingTests: RalphCoreTestCase {
    func testShowOfflineBannerWhenUnavailable() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "offline-banner-unavailable")
        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        XCTAssertFalse(workspace.showOfflineBanner)

        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )

        XCTAssertTrue(workspace.showOfflineBanner)
    }

    func testShowOfflineBannerWhenAvailable() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "offline-banner-available")
        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )
        XCTAssertFalse(workspace.showOfflineBanner)
    }

    func testIsShowingCachedTasks() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "offline-cached-tasks")
        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        XCTAssertFalse(workspace.isShowingCachedTasks)

        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]

        XCTAssertTrue(workspace.isShowingCachedTasks)
    }

    func testDisplayTasksWhenOffline() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "offline-display")
        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)
        let cachedTask = RalphTask(id: "RQ-CACHED", status: .done, title: "Cached", priority: .low)

        workspace.tasks = [onlineTask]
        workspace.cachedTasks = [cachedTask]
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )

        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-CACHED")
    }

    func testDisplayTasksWhenOnline() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "online-display")
        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)

        workspace.tasks = [onlineTask]
        workspace.cachedTasks = []
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )

        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-ONLINE")
    }

    func testClearCachedTasks() {
        let workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "clear-cached"))
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]

        workspace.clearCachedTasks()
        XCTAssertTrue(workspace.cachedTasks.isEmpty)
    }
}
