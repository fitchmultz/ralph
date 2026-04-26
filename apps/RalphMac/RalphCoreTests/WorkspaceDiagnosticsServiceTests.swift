/**
 WorkspaceDiagnosticsServiceTests

 Purpose:
 - Verify queue-lock diagnostics consume structured machine unlock inspection documents.

 Responsibilities:
 - Verify queue-lock diagnostics consume structured machine unlock inspection documents.
 - Guard against regressions back to human-text parsing for queue-lock state.

 Does not handle:
 - Queue unlock mutation flows.
 - Doctor report rendering beyond the exercised snapshot path.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

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

    func testQueueValidationOutput_reportsConfiguredQueuePathWhenCustomQueueIsMissing() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "workspace-diagnostics-custom-queue-missing",
            workspaceName: "workspace",
            createConfigFile: true
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let customQueueURL = fixture.workspaceURL.appendingPathComponent("custom/queue.jsonc", isDirectory: false)
        let customDoneURL = fixture.workspaceURL.appendingPathComponent("custom/done.jsonc", isDirectory: false)
        let overrides = RalphMockCLITestSupport.MockResolvedPathOverrides(
            queueURL: customQueueURL,
            doneURL: customDoneURL,
            projectConfigURL: fixture.configURL
        )
        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                pathOverrides: overrides
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-diagnostics-custom-missing",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        let output = await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        XCTAssertTrue(output.contains("Queue validation skipped"))
        XCTAssertTrue(output.contains(customQueueURL.path))
        XCTAssertTrue(output.contains("ralph machine config resolve"))
        XCTAssertFalse(output.contains(".ralph/config.jsonc"))
        XCTAssertFalse(output.contains("ralph init --non-interactive"))
    }

    func testQueueValidationOutput_reportsConfigResolutionFailure() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "workspace-diagnostics-config-resolve-failure",
            workspaceName: "workspace"
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              echo "load project config: unsupported config version 999" >&2
              exit 11
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-diagnostics-config-fail",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        let output = await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        XCTAssertTrue(output.contains("could not resolve the workspace queue paths"))
        XCTAssertTrue(output.contains("Workspace config is incompatible with this Ralph version"))
    }
}
