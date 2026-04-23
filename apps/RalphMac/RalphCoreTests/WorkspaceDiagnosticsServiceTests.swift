/**
 WorkspaceDiagnosticsServiceTests

 Responsibilities:
 - Verify queue-lock diagnostics consume structured machine unlock inspection documents.
 - Guard against regressions back to human-text parsing for queue-lock state.

 Does not handle:
 - Queue unlock mutation flows.
 - Doctor report rendering beyond the exercised snapshot path.

 Invariants/assumptions callers must respect:
 - Mock CLI scripts emit only the commands required by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceDiagnosticsServiceTests: RalphCoreTestCase {
    func testQueueLockDiagnosticSnapshot_usesStructuredUnlockInspectDocument() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "workspace-diagnostics-lock-inspect")
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let doctorURL = fixture.rootURL.appendingPathComponent("doctor.json", isDirectory: false)
        try """
        {
          "version": 2,
          "blocking": {
            "status": "stalled",
            "reason": {
              "kind": "lock_blocked",
              "lock_path": "/tmp/.ralph/lock",
              "owner": "test",
              "owner_pid": 42
            },
            "task_id": null,
            "message": "Ralph is stalled on a stale queue lock.",
            "detail": "dead pid"
          },
          "report": { "success": false }
        }
        """.write(to: doctorURL, atomically: true, encoding: .utf8)

        let unlockURL = fixture.rootURL.appendingPathComponent("unlock-inspect.json", isDirectory: false)
        try """
        {
          "version": 1,
          "condition": "stale",
          "blocking": null,
          "unlock_allowed": true,
          "continuation": {
            "headline": "Queue lock is stale",
            "detail": "Safe to clear.",
            "next_steps": []
          }
        }
        """.write(to: unlockURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "doctor" ] && [ "$4" = "report" ]; then
              cat "\(doctorURL.path)"
              exit 0
            fi
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "queue" ] && [ "$4" = "unlock-inspect" ]; then
              cat "\(unlockURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, body: script)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: try RalphCLIClient(executableURL: scriptURL))

        let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
        XCTAssertEqual(snapshot.condition, .stale)
        XCTAssertTrue(snapshot.unlockPreview.contains("Unlock allowed: yes"))
        XCTAssertTrue(snapshot.canClearStaleLock)
    }
}
