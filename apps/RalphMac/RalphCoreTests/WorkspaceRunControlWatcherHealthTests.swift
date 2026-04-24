/**
 WorkspaceRunControlWatcherHealthTests

 Purpose:
 - Validate queue watcher health is reflected in workspace operational summaries.

 Responsibilities:
 - Validate queue watcher health is reflected in workspace operational summaries.

 Does not handle:
 - Run invocation, blocking/resume state, parallel status, or loop/cancel behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests construct in-memory workspaces without mock CLI fixtures.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunControlWatcherHealthTests: WorkspacePerformanceTestCase {
    func test_updateWatcherHealth_surfacesOperationalIssue() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "watcher-health-operational")
        )

        workspace.updateWatcherHealth(
            QueueWatcherHealth(
                state: .failed(reason: "stream bootstrap failed", attempts: 3),
                workingDirectoryURL: workspace.workingDirectoryURL
            )
        )

        XCTAssertEqual(workspace.operationalSummary.severity, .error)
        XCTAssertEqual(workspace.operationalIssues.first?.source, .watcher)
        XCTAssertEqual(workspace.operationalIssues.first?.title, "Queue watcher failed")
    }
}
