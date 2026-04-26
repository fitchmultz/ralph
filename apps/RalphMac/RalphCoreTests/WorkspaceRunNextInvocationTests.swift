/**
 WorkspaceRunNextInvocationTests

 Purpose:
 - Validate `runNextTask` preparation-state transitions, CLI argument selection, and streamed machine-run output.

 Responsibilities:
 - Validate `runNextTask` preparation-state transitions, CLI argument selection, and streamed machine-run output.

 Does not handle:
 - Resume or blocking-state application, parallel status, loop/cancel, or watcher health.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunNextInvocationTests: WorkspacePerformanceTestCase {
    func test_runNextTask_entersPreparationWithoutQueueReadOrImplicitTaskID() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-preparing",
            scriptName: "mock-ralph-run-preparing",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let commandLogURL = fixture.rootURL.appendingPathComponent("command-log.txt", isDirectory: false)

        let script = """
            #!/bin/sh
            printf '%s\\n' "$*" >> "\(commandLogURL.path)"

            sleep 1

            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "run" ] && [ "$4" = "one" ]; then
              shift 4
              if [ "$#" -ne 1 ] || [ "$1" != "--resume" ]; then
                echo "unexpected run one args: $*" 1>&2
                exit 65
              fi
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed","blocking":null}'
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: fixture.scriptURL.lastPathComponent,
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )

        workspace.runNextTask()

        XCTAssertTrue(workspace.runState.isPreparingRun)
        XCTAssertTrue(workspace.runState.isExecutionActive)
        XCTAssertFalse(workspace.runState.isRunning)
        XCTAssertNil(workspace.runState.currentTaskID)

        let finished = await WorkspacePerformanceTestSupport.waitFor(timeout: 4.0) {
            !workspace.runState.isExecutionActive
        }
        XCTAssertTrue(finished)
        XCTAssertEqual(workspace.runState.lastExitStatus?.code, 0)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run one --resume"))
        XCTAssertFalse(commandLog.contains("machine queue read"))
        XCTAssertFalse(commandLog.contains(" --id "))
    }

    func test_runNextTask_usesCanonicalCLISelection_andStreamsSelectedTask() async throws {
        var workspace: Workspace!
        let stalePreviewTask = RalphMockCLITestSupport.task(
            id: "RQ-OLD",
            status: .todo,
            title: "Stale preview task",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-stream",
            scriptName: "mock-ralph-run-stream",
            seedQueueTasks: [stalePreviewTask]
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let commandLogURL = fixture.rootURL.appendingPathComponent("command-log.txt", isDirectory: false)

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 2)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )

        let script = """
            #!/bin/sh
            printf '%s\\n' "$*" >> "\(commandLogURL.path)"

            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine queue read"*)
              echo "unexpected queue read during runNextTask" 1>&2
              exit 65
              ;;
              *"--no-color machine run one --resume"*)
              sleep 1
              echo '{"version":3,"kind":"run_started","timestamp":"2026-03-10T00:00:00Z","task_id":null,"phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"task_selected","timestamp":"2026-03-10T00:00:00Z","task_id":"RQ-NEW","phase":null,"message":"Canonical task","payload":null}'
              echo '{"version":3,"kind":"phase_entered","timestamp":"2026-03-10T00:00:01Z","task_id":"RQ-NEW","phase":"plan","message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","timestamp":"2026-03-10T00:00:02Z","task_id":"RQ-NEW","phase":"plan","message":null,"payload":{"text":"planning started\\n"}}'
              sleep 1
              echo '{"version":3,"kind":"phase_completed","timestamp":"2026-03-10T00:00:03Z","task_id":"RQ-NEW","phase":"plan","message":null,"payload":null}'
              echo '{"version":3,"kind":"phase_entered","timestamp":"2026-03-10T00:00:04Z","task_id":"RQ-NEW","phase":"implement","message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","timestamp":"2026-03-10T00:00:05Z","task_id":"RQ-NEW","phase":"implement","message":null,"payload":{"text":"implementation running\\n"}}'
              sleep 1
              echo '{"version":3,"kind":"phase_completed","timestamp":"2026-03-10T00:00:06Z","task_id":"RQ-NEW","phase":"implement","message":null,"payload":null}'
              echo '{"version":2,"task_id":"RQ-NEW","exit_code":0,"outcome":"success"}'
              exit 0
              ;;
            esac
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: fixture.scriptURL.lastPathComponent,
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        workspace.currentTaskID = "RQ-PREVIOUS"

        workspace.runNextTask()

        let clearedStaleTaskID = await WorkspacePerformanceTestSupport.waitFor(timeout: 0.5) {
            workspace.currentTaskID == nil && workspace.runState.isExecutionActive
        }
        XCTAssertTrue(clearedStaleTaskID)

        let startedStreaming = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-NEW"
                && workspace.currentPhase == .plan
                && workspace.output.contains("planning started")
        }
        XCTAssertTrue(startedStreaming)

        XCTAssertEqual(workspace.currentTaskID, "RQ-NEW")
        XCTAssertEqual(workspace.currentPhase, .plan)
        XCTAssertTrue(workspace.output.contains("planning started"))
        XCTAssertTrue(workspace.isRunning)

        let finishedStreaming = await WorkspacePerformanceTestSupport.waitFor(timeout: 4.0) {
            !workspace.isRunning
        }
        XCTAssertTrue(finishedStreaming)

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertTrue(workspace.output.contains("implementation running"))
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-NEW")
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, false)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run one --resume"))
        XCTAssertFalse(commandLog.contains("machine queue read"))
        XCTAssertFalse(commandLog.contains(" --id "))
    }

    func test_runNextTask_withExplicitIDAndForce_usesExpectedArguments() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-explicit",
            scriptName: "mock-ralph-run-explicit"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let commandLogURL = fixture.rootURL.appendingPathComponent("command-log.txt", isDirectory: false)

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 1)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )

        let script = """
            #!/bin/sh
            printf '%s\\n' "$*" >> "\(commandLogURL.path)"

            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine run one --resume --force --id RQ-5555"*)
              echo '{"version":3,"kind":"run_started","timestamp":"2026-03-10T00:00:00Z","task_id":"RQ-5555","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","timestamp":"2026-03-10T00:00:01Z","task_id":"RQ-5555","phase":null,"message":null,"payload":{"text":"running explicit\\n"}}'
              echo '{"version":2,"task_id":"RQ-5555","exit_code":0,"outcome":"success"}'
              exit 0
              ;;
            esac
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: fixture.scriptURL.lastPathComponent,
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        workspace.runNextTask(taskIDOverride: "RQ-5555", forceDirtyRepo: true)

        let explicitRunStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-5555" && workspace.isRunning
        }
        XCTAssertTrue(explicitRunStarted)
        let explicitRunFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            !workspace.isRunning
        }
        XCTAssertTrue(explicitRunFinished)

        XCTAssertEqual(workspace.currentTaskID, nil)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-5555")
        XCTAssertTrue(workspace.output.contains("running explicit"))

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run one --resume --force --id RQ-5555"))
    }
}
