/**
 WorkspaceRunNextInvocationTests

 Responsibilities:
 - Validate `runNextTask` CLI argument selection and streamed machine-run output.

 Does not handle:
 - Resume or blocking-state application, parallel status, loop/cancel, or watcher health.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunNextInvocationTests: WorkspacePerformanceTestCase {
    func test_runNextTask_resolvesCLISelection_andStreamsOutput() async throws {
        var workspace: Workspace!
        let queuedTask = RalphMockCLITestSupport.task(
            id: "RQ-4242",
            status: .todo,
            title: "Queued task",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-stream",
            scriptName: "mock-ralph-run-stream",
            seedQueueTasks: [queuedTask]
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 2)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: [queuedTask],
                nextRunnableTaskID: "RQ-4242"
            ),
            in: fixture.rootURL,
            name: "queue-read.json"
        )

        let script = """
            #!/bin/sh
            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine queue read"*)
              cat "\(queueReadURL.path)"
              exit 0
              ;;
              *"--no-color machine run one --resume --id RQ-4242"*)
              echo '{"version":3,"kind":"run_started","timestamp":"2026-03-10T00:00:00Z","task_id":"RQ-4242","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"phase_entered","timestamp":"2026-03-10T00:00:01Z","task_id":"RQ-4242","phase":"plan","message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","timestamp":"2026-03-10T00:00:02Z","task_id":"RQ-4242","phase":"plan","message":null,"payload":{"text":"planning started\\n"}}'
              sleep 1
              echo '{"version":3,"kind":"phase_completed","timestamp":"2026-03-10T00:00:03Z","task_id":"RQ-4242","phase":"plan","message":null,"payload":null}'
              echo '{"version":3,"kind":"phase_entered","timestamp":"2026-03-10T00:00:04Z","task_id":"RQ-4242","phase":"implement","message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","timestamp":"2026-03-10T00:00:05Z","task_id":"RQ-4242","phase":"implement","message":null,"payload":{"text":"implementation running\\n"}}'
              sleep 1
              echo '{"version":3,"kind":"phase_completed","timestamp":"2026-03-10T00:00:06Z","task_id":"RQ-4242","phase":"implement","message":null,"payload":null}'
              echo '{"version":2,"task_id":"RQ-4242","exit_code":0,"outcome":"success"}'
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

        workspace.runNextTask()

        let startedStreaming = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-4242"
                && workspace.currentPhase == .plan
                && workspace.output.contains("planning started")
        }
        XCTAssertTrue(startedStreaming)

        XCTAssertEqual(workspace.currentTaskID, "RQ-4242")
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
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-4242")
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, false)
    }

    func test_runNextTask_withExplicitIDAndForce_usesExpectedArguments() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-explicit",
            scriptName: "mock-ralph-run-explicit"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

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
    }
}
