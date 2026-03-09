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
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":2}}'
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--dry-run" ]; then
              echo "Dry run: would run RQ-4242 (status: Todo)"
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--id" ] && [ "$5" = "RQ-4242" ]; then
              echo "PHASE 1 starting"
              sleep 1
              echo "PHASE 2 running"
              sleep 1
              echo "done"
              exit 0
            fi
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

        await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-4242" && workspace.output.contains("PHASE 1")
        }

        XCTAssertEqual(workspace.currentTaskID, "RQ-4242")
        XCTAssertTrue(workspace.output.contains("PHASE 1"))
        XCTAssertTrue(workspace.isRunning)

        await WorkspacePerformanceTestSupport.waitFor(timeout: 4.0) {
            !workspace.isRunning
        }

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertTrue(workspace.output.contains("PHASE 2"))
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-4242")
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, false)
    }

    func test_runNextTask_withExplicitIDAndForce_usesExpectedArguments() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-run-explicit-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":1}}'
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ]; then
              case "$*" in
                *"--no-color run one --force --id RQ-5555"*)
                  echo "running explicit"
                  exit 0
                  ;;
              esac
            fi
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

        await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-5555" && workspace.isRunning
        }
        await WorkspacePerformanceTestSupport.waitFor(timeout: 3.0) {
            !workspace.isRunning
        }

        XCTAssertEqual(workspace.currentTaskID, nil)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-5555")
        XCTAssertTrue(workspace.output.contains("running explicit"))
    }

    func test_runControlPreviewTask_prefersSelectedTodoTask() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
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
        defer { try? FileManager.default.removeItem(at: tempDir) }
        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":2}}'
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

        await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            workspace.isRunning
        }
        XCTAssertTrue(workspace.isRunning)
        try await Task.sleep(nanoseconds: 150_000_000)

        workspace.cancel()

        await WorkspacePerformanceTestSupport.waitFor(timeout: 6.0) {
            !workspace.isRunning
        }

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, true)
        XCTAssertNil(workspace.executionHistory.first?.exitCode)
        XCTAssertEqual(workspace.isLoopMode, false)
        XCTAssertEqual(workspace.stopAfterCurrent, true)
    }

    func test_startLoop_schedulesNextRunWithoutSleepDelay() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-loop-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        let stateURL = tempDir.appendingPathComponent("loop-state.txt")

        let script = """
            #!/bin/sh
            state_file="\(stateURL.path)"

            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":2}}'
              exit 0
            fi

            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--dry-run" ]; then
              if [ ! -f "$state_file" ]; then
                echo "Dry run: would run RQ-LOOP-1"
              else
                echo "Dry run: would run RQ-LOOP-2"
              fi
              exit 0
            fi

            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--id" ] && [ "$5" = "RQ-LOOP-1" ]; then
              echo "running first"
              echo "done" > "$state_file"
              exit 0
            fi

            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--id" ] && [ "$5" = "RQ-LOOP-2" ]; then
              echo "running second"
              exit 64
            fi

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

        await WorkspacePerformanceTestSupport.waitFor(timeout: 0.75) {
            workspace.output.contains("running second")
        }

        XCTAssertLessThan(Date().timeIntervalSince(startedAt), 0.9)

        await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning
        }

        XCTAssertTrue(workspace.output.contains("running first"))
        XCTAssertTrue(workspace.output.contains("running second"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 64)
        XCTAssertFalse(workspace.isLoopMode)
    }

    func test_updateWatcherHealth_surfacesOperationalIssue() {
        let workspace = Workspace(
            workingDirectoryURL: URL(fileURLWithPath: "/tmp/ralph-operational-\(UUID().uuidString)")
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
