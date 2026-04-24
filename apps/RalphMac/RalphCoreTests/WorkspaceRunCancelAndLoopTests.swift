/**
 WorkspaceRunCancelAndLoopTests

 Purpose:
 - Validate active-run cancellation and CLI-owned loop command/control behavior.

 Responsibilities:
 - Validate active-run cancellation and CLI-owned loop command/control behavior.

 Does not handle:
 - Resume/blocking semantics, parallel status, run-next argument matrix, watcher health.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunCancelAndLoopTests: WorkspacePerformanceTestCase {
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
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )
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

    func test_startLoop_usesMachineRunLoopCommand() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop",
            scriptName: "mock-ralph-loop",
            seedQueueTasks: []
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
              *"--no-color machine run loop --resume --max-tasks 0"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":{"text":"running first\\n"}}'
              echo '{"version":1,"kind":"task_selected","task_id":"RQ-LOOP-2","phase":null,"message":"Second loop task","payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-2","phase":null,"message":null,"payload":{"text":"running second\\n"}}'
              echo '{"version":1,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

        workspace.startLoop()

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run loop --resume --max-tasks 0"))
        XCTAssertFalse(commandLog.contains("machine run one"))
        XCTAssertTrue(workspace.output.contains("running first"))
        XCTAssertTrue(workspace.output.contains("running second"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertFalse(workspace.isLoopMode)
    }

    func test_startLoop_clearsLoopModeWhenProcessLaunchFails() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-launch-failure",
            scriptName: "mock-ralph-loop-launch-failure",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let script = """
            #!/bin/sh
            echo "unexpected launch"
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
        try FileManager.default.removeItem(at: fixture.workspaceURL)

        workspace.startLoop()

        let failureSurfaced = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            workspace.runState.errorMessage != nil
        }
        XCTAssertTrue(failureSurfaced)
        XCTAssertFalse(workspace.isRunning)
        XCTAssertFalse(workspace.isLoopMode)
        XCTAssertFalse(workspace.stopAfterCurrent)
    }

    func test_startLoop_withParallelWorkers_appendsParallelOverride() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-parallel",
            scriptName: "mock-ralph-loop-parallel",
            seedQueueTasks: []
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
              *"--no-color machine run loop --resume --max-tasks 0 --parallel 2"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-PARALLEL","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

        workspace.startLoop(parallelWorkers: 2)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run loop --resume --max-tasks 0 --parallel 2"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertFalse(workspace.isLoopMode)
    }

    func test_stopLoop_requestsQueueStopSignalForActiveMachineLoop() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-stop",
            scriptName: "mock-ralph-loop-stop",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let commandLogURL = fixture.rootURL.appendingPathComponent("command-log.txt", isDirectory: false)
        let stopSignalURL = fixture.rootURL.appendingPathComponent("stop-signal.txt", isDirectory: false)

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
              *"--no-color machine run loop --resume --max-tasks 0"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop running\\n"}}'
              while [ ! -f "\(stopSignalURL.path)" ]; do
                sleep 0.05
              done
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop stopping\\n"}}'
              echo '{"version":1,"task_id":null,"exit_code":0,"outcome":"completed"}'
              exit 0
              ;;
              *"queue stop"*)
              echo "stop command received" > "\(stopSignalURL.path)"
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

        workspace.startLoop()

        let loopStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            workspace.output.contains("loop running")
        }
        XCTAssertTrue(loopStarted)

        workspace.stopLoop()

        let stopRequested = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            FileManager.default.fileExists(atPath: stopSignalURL.path)
        }
        XCTAssertTrue(stopRequested)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run loop --resume --max-tasks 0"))
        XCTAssertTrue(commandLog.contains("queue stop"))
        XCTAssertTrue(workspace.output.contains("loop running"))
        XCTAssertTrue(workspace.output.contains("loop stopping"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertFalse(workspace.isLoopMode)
        XCTAssertFalse(workspace.stopAfterCurrent)
    }
}
