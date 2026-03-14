/**
 WorkspaceBackgroundTaskOwnershipTests

 Responsibilities:
 - Validate manager-owned workspace bootstrap tasks cancel on close.
 - Validate workspace-owned health-check tasks cancel on shutdown and retarget.
 - Guard against stale background task results bleeding across repository lifecycle changes.

 Does not handle:
 - Queue watcher low-level retry behavior.
 - UI-scene routing or window-placement assertions.

 Invariants/assumptions callers must respect:
 - Mock CLIs cooperate with SIGINT/SIGTERM cancellation and expose only the exercised command paths.
 - Tests restore the shared WorkspaceManager singleton state before returning.
 */

import Foundation
import XCTest

@testable import RalphCore

final class WorkspaceBackgroundTaskOwnershipTests: RalphCoreTestCase {
    @MainActor
    private func resetManagerState(_ manager: WorkspaceManager) {
        manager.resetWindowStateClaimPool()
        manager.resetSceneRoutingForTests()
        for workspace in manager.workspaces {
            manager.closeWorkspace(workspace)
        }
    }

    @MainActor
    func test_closeWorkspace_cancelsInFlightCLISpecBootstrap() async throws {
        let manager = WorkspaceManager.shared
        let originalClient = manager.client
        resetManagerState(manager)
        defer {
            manager.client = originalClient
            resetManagerState(manager)
        }

        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-bootstrap-close")
        defer { RalphCoreTestSupport.assertRemoved(rootDir) }
        let workspaceURL = rootDir.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceURL, withIntermediateDirectories: true)

        let logURL = rootDir.appendingPathComponent("cli-spec-bootstrap.log", isDirectory: false)
        let specURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec.json",
            machineLeafName: "task-bootstrap"
        )

        let script = """
            #!/bin/sh
            trap 'printf "canceled\\n" >> "\(logURL.path)"; exit 130' INT TERM
            printf 'started\n' >> "\(logURL.path)"
            sleep 5
            printf 'finished\n' >> "\(logURL.path)"
            cat "\(specURL.path)"
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-cli-spec-bootstrap",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)

        let workspace = Workspace(workingDirectoryURL: workspaceURL)
        workspace.client = client
        manager.workspaces.append(workspace)

        manager.scheduleWorkspaceBootstrap(for: workspace)

        let started = await RalphCoreTestSupport.waitUntil(timeout: .seconds(2)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("started")) == true
        }
        XCTAssertTrue(started)

        manager.closeWorkspace(workspace)

        let canceled = await RalphCoreTestSupport.waitUntil(timeout: .seconds(3)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("canceled")) == true
        }
        XCTAssertTrue(canceled)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertFalse(log.contains("finished"))
    }

    @MainActor
    func test_scheduleHealthCheck_shutdownCancelsInFlightProbe() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-health-shutdown")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }

        let logURL = rootDir.appendingPathComponent("health-check.log", isDirectory: false)
        let script = """
            #!/bin/sh
            trap 'printf "canceled\\n" >> "\(logURL.path)"; exit 130' INT TERM
            printf 'started\n' >> "\(logURL.path)"
            sleep 5
            printf 'finished\n' >> "\(logURL.path)"
            printf '{"version":1,"cli_version":"1.0.0"}\n'
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-health-shutdown",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: rootDir)
        workspace.client = client

        workspace.scheduleHealthCheck(loadCachedTasksOnUnavailable: false)

        let started = await RalphCoreTestSupport.waitUntil(timeout: .seconds(2)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("started")) == true
        }
        XCTAssertTrue(started)

        workspace.shutdown()

        let shutdownSettled = await RalphCoreTestSupport.waitUntil(timeout: .seconds(3)) {
            await MainActor.run {
                workspace.diagnosticsState.cliHealthStatus == nil
                    && workspace.diagnosticsState.isCheckingHealth == false
            }
        }
        XCTAssertTrue(shutdownSettled)
        XCTAssertNil(workspace.diagnosticsState.cliHealthStatus)
        XCTAssertFalse(workspace.diagnosticsState.isCheckingHealth)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertFalse(log.contains("finished"))
    }

    @MainActor
    func test_scheduleHealthCheck_retargetDiscardsStaleProbeResult() async throws {
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-health-retarget")
        let workspaceAURL = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBURL = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceAURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBURL, withIntermediateDirectories: true)

        let logURL = rootDir.appendingPathComponent("health-retarget.log", isDirectory: false)
        let counterURL = rootDir.appendingPathComponent("health-retarget.count", isDirectory: false)
        let script = """
            #!/bin/sh
            count=0
            if [ -f "\(counterURL.path)" ]; then
              count=$(cat "\(counterURL.path)")
            fi
            count=$((count + 1))
            printf '%s' "$count" > "\(counterURL.path)"

            if [ "$count" -eq 1 ]; then
              trap 'printf "first-canceled\\n" >> "\(logURL.path)"; exit 130' INT TERM
              printf 'first-started\n' >> "\(logURL.path)"
              sleep 5
              printf 'first-finished\n' >> "\(logURL.path)"
            else
              printf 'second-started\n' >> "\(logURL.path)"
            fi

            printf '{"version":1,"cli_version":"1.0.0"}\n'
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-health-retarget",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: workspaceAURL)
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        workspace.client = client

        workspace.scheduleHealthCheck(loadCachedTasksOnUnavailable: false)

        let firstStarted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(2)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("first-started")) == true
        }
        XCTAssertTrue(firstStarted)

        _ = workspace.beginRepositoryRetarget(to: workspaceBURL)
        workspace.scheduleHealthCheck(loadCachedTasksOnUnavailable: false)

        let retargeted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(3)) {
            await MainActor.run {
                workspace.diagnosticsState.cliHealthStatus?.workspaceURL == workspaceBURL
                    && workspace.identityState.workingDirectoryURL == workspaceBURL
            }
        }
        XCTAssertTrue(retargeted)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertTrue(log.contains("second-started"))
        XCTAssertFalse(log.contains("first-finished"))
        XCTAssertEqual(workspace.diagnosticsState.cliHealthStatus?.workspaceURL, workspaceBURL)
    }
}
