/**
 WorkspaceRunControlTests

 Responsibilities:
 - Validate run-control preview, streaming, cancellation, and loop behavior.
 - Cover watcher-health operational surfacing.

 Does not handle:
 - Runner-configuration refresh or task-mutation payload serialization.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunControlTests: WorkspacePerformanceTestCase {
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
              *"--no-color machine run one --id RQ-4242"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-4242","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"phase_entered","task_id":"RQ-4242","phase":"plan","message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-4242","phase":"plan","message":null,"payload":{"text":"planning started\\n"}}'
              sleep 1
              echo '{"version":1,"kind":"phase_completed","task_id":"RQ-4242","phase":"plan","message":null,"payload":null}'
              echo '{"version":1,"kind":"phase_entered","task_id":"RQ-4242","phase":"implement","message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-4242","phase":"implement","message":null,"payload":{"text":"implementation running\\n"}}'
              sleep 1
              echo '{"version":1,"kind":"phase_completed","task_id":"RQ-4242","phase":"implement","message":null,"payload":null}'
              echo '{"version":1,"task_id":"RQ-4242","exit_code":0,"outcome":"success"}'
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
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)
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
              *"--no-color machine run one --force --id RQ-5555"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-5555","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-5555","phase":null,"message":null,"payload":{"text":"running explicit\\n"}}'
              echo '{"version":1,"task_id":"RQ-5555","exit_code":0,"outcome":"success"}'
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
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)
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

    func test_runControlPreviewTask_prefersSelectedTodoTask() {
        let workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-preview"))
        workspace.tasks = [
            RalphTask(id: "RQ-1001", status: .todo, title: "First", priority: .medium),
            RalphTask(id: "RQ-1002", status: .todo, title: "Second", priority: .high)
        ]

        workspace.runControlSelectedTaskID = "RQ-1002"
        XCTAssertEqual(workspace.runControlPreviewTask?.id, "RQ-1002")

        workspace.runControlSelectedTaskID = "RQ-9999"
        XCTAssertEqual(workspace.runControlPreviewTask?.id, "RQ-1001")
    }

    func test_cancel_stopsActiveRun_andRecordsCancellation() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-cancel",
            scriptName: "mock-ralph-run-cancel"
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

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            exec /bin/sleep "$@"
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: fixture.scriptURL.lastPathComponent,
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)
                await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        workspace.run(arguments: ["60"])

        let cancelRunStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            workspace.isRunning
        }
        XCTAssertTrue(cancelRunStarted)
        XCTAssertTrue(workspace.isRunning)

        workspace.cancel()

        let cancelRunFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 6.0) {
            !workspace.isRunning
        }
        XCTAssertTrue(cancelRunFinished)

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, true)
        XCTAssertNil(workspace.executionHistory.first?.exitCode)
        XCTAssertEqual(workspace.isLoopMode, false)
        XCTAssertEqual(workspace.stopAfterCurrent, true)
    }

    func test_startLoop_schedulesNextRunWithoutSleepDelay() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop",
            scriptName: "mock-ralph-loop",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let stateURL = fixture.rootURL.appendingPathComponent("loop-state.txt", isDirectory: false)

        let loopTaskOne = RalphMockCLITestSupport.task(
            id: "RQ-LOOP-1",
            status: .todo,
            title: "First loop task",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )
        let loopTaskTwo = RalphMockCLITestSupport.task(
            id: "RQ-LOOP-2",
            status: .todo,
            title: "Second loop task",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 2)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let loopQueueOneURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: [loopTaskOne],
                nextRunnableTaskID: "RQ-LOOP-1"
            ),
            in: fixture.rootURL,
            name: "queue-read-first.json"
        )
        let loopQueueTwoURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: [loopTaskTwo],
                nextRunnableTaskID: "RQ-LOOP-2"
            ),
            in: fixture.rootURL,
            name: "queue-read-second.json"
        )

        let script = """
            #!/bin/sh
            state_file="\(stateURL.path)"

            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine queue read"*)
              if [ ! -f "$state_file" ]; then
                cat "\(loopQueueOneURL.path)"
              else
                cat "\(loopQueueTwoURL.path)"
              fi
              exit 0
              ;;
              *"--no-color machine run one --id RQ-LOOP-1"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":{"text":"running first\\n"}}'
              echo '{"version":1,"task_id":"RQ-LOOP-1","exit_code":0,"outcome":"success"}'
              echo "done" > "$state_file"
              exit 0
              ;;
              *"--no-color machine run one --id RQ-LOOP-2"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-2","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-2","phase":null,"message":null,"payload":{"text":"running second\\n"}}'
              echo '{"version":1,"task_id":"RQ-LOOP-2","exit_code":64,"outcome":"failure"}'
              exit 64
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
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: client)
                await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        let startedAt = Date()
        workspace.startLoop()

        let loopAdvanced = await WorkspacePerformanceTestSupport.waitFor(timeout: 0.75) {
            workspace.output.contains("running second")
        }
        XCTAssertTrue(loopAdvanced)

        XCTAssertLessThan(Date().timeIntervalSince(startedAt), 0.9)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning
        }
        XCTAssertTrue(loopFinished)

        XCTAssertTrue(workspace.output.contains("running first"))
        XCTAssertTrue(workspace.output.contains("running second"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 64)
        XCTAssertFalse(workspace.isLoopMode)
    }

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
