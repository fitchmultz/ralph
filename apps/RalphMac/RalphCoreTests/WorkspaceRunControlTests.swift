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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-run-stream-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
            #!/bin/sh
            case "$*" in
              *"--no-color machine config resolve"*)
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"model-test","iterations":2}}}'
              exit 0
              ;;
              *"--no-color machine queue read"*)
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[{"id":"RQ-4242","status":"todo","title":"Queued task","priority":"medium","tags":[],"created_at":"2026-03-10T00:00:00Z","updated_at":"2026-03-10T00:00:00Z"}]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":"RQ-4242","runnability":{}}'
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
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-stream",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-run-explicit-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
            #!/bin/sh
            case "$*" in
              *"--no-color machine config resolve"*)
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"model-test","iterations":1}}}'
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
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-explicit",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-run-cancel-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"model-test","iterations":2}}}'
              exit 0
            fi
            exec /bin/sleep "$@"
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-cancel",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-loop-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        let stateURL = tempDir.appendingPathComponent("loop-state.txt")

        let script = """
            #!/bin/sh
            state_file="\(stateURL.path)"

            case "$*" in
              *"--no-color machine config resolve"*)
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"model-test","iterations":2}}}'
              exit 0
              ;;
              *"--no-color machine queue read"*)
              if [ ! -f "$state_file" ]; then
                echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[{"id":"RQ-LOOP-1","status":"todo","title":"First loop task","priority":"medium","tags":[],"created_at":"2026-03-10T00:00:00Z","updated_at":"2026-03-10T00:00:00Z"}]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":"RQ-LOOP-1","runnability":{}}'
              else
                echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[{"id":"RQ-LOOP-2","status":"todo","title":"Second loop task","priority":"medium","tags":[],"created_at":"2026-03-10T00:00:00Z","updated_at":"2026-03-10T00:00:00Z"}]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":"RQ-LOOP-2","runnability":{}}'
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
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-loop",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
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
