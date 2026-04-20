/**
 WorkspaceBackgroundTaskOwnershipTests

 Responsibilities:
 - Validate manager-owned workspace bootstrap tasks cancel on close.
 - Validate workspace-owned health-check and runner-launch tasks cancel on shutdown and retarget.
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
        let pidFileURL = rootDir.appendingPathComponent("cli-spec-bootstrap.pid", isDirectory: false)
        let specURL = try WorkspaceRunnerConfigurationTestSupport.writeCLISpecDocument(
            in: rootDir,
            name: "cli-spec.json",
            machineLeafName: "task-bootstrap"
        )

        let script = """
            #!/bin/sh
            echo $$ > "\(pidFileURL.path)"
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
        let recordedPID = await RalphCoreTestSupport.waitForFile(pidFileURL, timeout: .seconds(2))
        XCTAssertTrue(recordedPID)

        manager.closeWorkspace(workspace)

        let pidText = try XCTUnwrap(
            String(contentsOf: pidFileURL, encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines)
        )
        let pid = pid_t(try XCTUnwrap(Int32(pidText)))
        let terminated = await RalphCoreTestSupport.waitForProcessExit(pid, timeout: .seconds(5))
        XCTAssertTrue(terminated)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertFalse(log.contains("finished"))
    }

    @MainActor
    func test_createWorkspace_doesNotScheduleAutomaticCLISpecBootstrap() throws {
        let manager = WorkspaceManager.shared
        let originalClient = manager.client
        resetManagerState(manager)
        defer {
            manager.client = originalClient
            resetManagerState(manager)
        }

        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-bootstrap-auto")
        defer { RalphCoreTestSupport.assertRemoved(rootDir) }
        let workspaceURL = rootDir.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceURL, withIntermediateDirectories: true)

        manager.client = nil
        let workspace = manager.createWorkspace(workingDirectory: workspaceURL)

        XCTAssertNil(manager.workspaceBootstrapTasks[workspace.id])
        XCTAssertNil(manager.workspaceBootstrapRevisions[workspace.id])
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

    @MainActor
    func test_shutdown_cancelsPendingRunLaunchBeforeProcessStarts() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-run-shutdown")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }

        let logURL = rootDir.appendingPathComponent("run-shutdown.log", isDirectory: false)
        let script = """
            #!/bin/sh
            printf 'run-started\n' >> "\(logURL.path)"
            sleep 5
            printf 'run-finished\n' >> "\(logURL.path)"
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-run-shutdown",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: rootDir)
        workspace.client = client

        workspace.run(arguments: ["run", "one"])
        workspace.shutdown()

        try await Task.sleep(for: .milliseconds(300))

        let log = try? String(contentsOf: logURL, encoding: .utf8)
        XCTAssertFalse(log?.contains("run-started") == true)
        XCTAssertFalse(workspace.runState.isRunning)
        XCTAssertTrue(workspace.runState.output.isEmpty)
        XCTAssertTrue(workspace.runState.executionHistory.isEmpty)
    }

    @MainActor
    func test_setWorkingDirectory_cancelsPendingRunNextTaskResolution() async throws {
        var workspace: Workspace!
        let rootDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-background-run-retarget")
        defer { RalphCoreTestSupport.shutdownAndRemove(rootDir, workspace) }
        let workspaceAURL = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBURL = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceAURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBURL, withIntermediateDirectories: true)
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceAURL, tasks: [])

        let logURL = rootDir.appendingPathComponent("run-retarget.log", isDirectory: false)
        let pidFileURL = rootDir.appendingPathComponent("run-retarget.pid", isDirectory: false)
        let queueAURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-a.json",
            workspaceURL: workspaceAURL,
            activeTasks: [],
            nextRunnableTaskID: "RQ-A"
        )
        let queueBURL = try WorkspaceRunnerConfigurationTestSupport.writeQueueReadDocument(
            in: rootDir,
            name: "queue-b.json",
            workspaceURL: workspaceBURL,
            activeTasks: [],
            nextRunnableTaskID: nil
        )
        let configAURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-a.json",
            workspaceURL: workspaceAURL,
            model: "model-a"
        )
        let configBURL = try WorkspaceRunnerConfigurationTestSupport.writeConfigResolveDocument(
            in: rootDir,
            name: "config-b.json",
            workspaceURL: workspaceBURL,
            model: "model-b"
        )
        let systemInfoURL = rootDir.appendingPathComponent("system-info.json", isDirectory: false)
        try Data("{\"version\":1,\"cli_version\":\"1.0.0\"}\n".utf8).write(to: systemInfoURL)

        let script = """
            #!/bin/sh
            case "$*" in
              *"--no-color machine system info"*)
              cat "\(systemInfoURL.path)"
              exit 0
              ;;
              *"--no-color machine queue read"*)
              case "$PWD" in
                */workspace-a)
                echo $$ > "\(pidFileURL.path)"
                trap 'printf "queue-read-canceled\\n" >> "\(logURL.path)"; exit 130' INT TERM
                printf 'queue-read-started\n' >> "\(logURL.path)"
                sleep 5
                printf 'queue-read-finished\n' >> "\(logURL.path)"
                cat "\(queueAURL.path)"
                exit 0
                ;;
                *)
                cat "\(queueBURL.path)"
                exit 0
                ;;
              esac
              ;;
              *"--no-color machine config resolve"*)
              case "$PWD" in
                */workspace-a)
                cat "\(configAURL.path)"
                exit 0
                ;;
                *)
                cat "\(configBURL.path)"
                exit 0
                ;;
              esac
              ;;
              *"--no-color machine run one"*)
              printf 'run-started\n' >> "\(logURL.path)"
              exit 0
              ;;
            esac
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-run-retarget",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: workspaceAURL)
        workspace.client = client

        workspace.runNextTask()

        let queueReadStarted = await RalphCoreTestSupport.waitUntil(timeout: .seconds(2)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("queue-read-started")) == true
        }
        XCTAssertTrue(queueReadStarted)

        workspace.setWorkingDirectory(workspaceBURL)

        let retargetSettled = await RalphCoreTestSupport.waitUntil(timeout: .seconds(3)) {
            await MainActor.run {
                workspace.identityState.workingDirectoryURL == workspaceBURL
                    && workspace.runState.isRunning == false
                    && workspace.runState.currentTaskID == nil
            }
        }
        XCTAssertTrue(retargetSettled)

        let pidRecorded = await RalphCoreTestSupport.waitForFile(pidFileURL, timeout: .seconds(2))
        XCTAssertTrue(pidRecorded)

        let pidText = try String(contentsOf: pidFileURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let pid = pid_t(try XCTUnwrap(Int32(pidText)))
        let cancelled = await RalphCoreTestSupport.waitForProcessExit(pid, timeout: .seconds(5))
        XCTAssertTrue(cancelled)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertTrue(log.contains("queue-read-canceled"))
        XCTAssertFalse(log.contains("queue-read-finished"))
        XCTAssertFalse(log.contains("run-started"))
        XCTAssertEqual(workspace.identityState.workingDirectoryURL, workspaceBURL)
        XCTAssertTrue(workspace.runState.executionHistory.isEmpty)
    }
}
