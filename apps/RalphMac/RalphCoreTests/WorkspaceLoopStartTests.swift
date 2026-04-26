/**
 WorkspaceLoopStartTests

 Purpose:
 - Validate loop-start command construction and start-of-run state transitions.

 Responsibilities:
 - Validate canonical loop invocation, launch-failure handling, summary-driven failure handling, and parallel override propagation.

 Does not handle:
 - Active-run cancellation or loop-stop marker behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceLoopStartTests: WorkspacePerformanceTestCase {
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
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":{"text":"running first\\n"}}'
              echo '{"version":3,"kind":"task_selected","task_id":"RQ-LOOP-2","phase":null,"message":"Second loop task","payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-2","phase":null,"message":null,"payload":{"text":"running second\\n"}}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

    func test_startLoop_summaryDrivenFailureClearsLoopModeWithoutRelyingOnStderr() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-summary-failure",
            scriptName: "mock-ralph-loop-summary-failure",
            seedQueueTasks: []
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
            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine run loop --resume --max-tasks 0"*)
              echo '{"version":3,"kind":"run_started","task_id":null,"phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"blocked_state_changed","task_id":null,"phase":null,"message":"Ralph is blocked by unfinished dependencies.","payload":{"status":"blocked","reason":{"kind":"dependency_blocked","blocked_tasks":2},"task_id":null,"message":"Ralph is blocked by unfinished dependencies.","detail":"2 candidate task(s) are waiting on dependency completion."}}'
              echo '{"version":3,"kind":"warning","task_id":null,"phase":null,"message":"Loop task failed after stream start.","payload":null}'
              echo '{"version":2,"task_id":null,"exit_code":1,"outcome":"failed","blocking":null}'
              echo '{"version":1,"code":"runner_failed","message":"machine stderr should not drive loop failure state","details":null}' 1>&2
              exit 1
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

        let failureFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 1
        }
        XCTAssertTrue(failureFinished)
        XCTAssertEqual(workspace.lastExitStatus?.code, 1)
        XCTAssertFalse(workspace.isLoopMode)
        XCTAssertFalse(workspace.stopAfterCurrent)
        XCTAssertNil(workspace.runState.blockingState)
        XCTAssertNil(workspace.runState.runControlOperatorState)
        XCTAssertTrue(workspace.output.contains("[warning] Loop task failed after stream start."))
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
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-PARALLEL","phase":null,"message":null,"payload":null}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

    func test_startLoop_withParallelWorkersAboveLegacyMenuCeiling_appendsParallelOverride() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-parallel-forty-two",
            scriptName: "mock-ralph-loop-parallel-forty-two",
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
              *"--no-color machine run loop --resume --max-tasks 0 --parallel 42"*)
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-PARALLEL-42","phase":null,"message":null,"payload":null}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

        workspace.startLoop(parallelWorkers: 42)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run loop --resume --max-tasks 0 --parallel 42"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertFalse(workspace.isLoopMode)
    }

    func test_startLoop_honorsMachineAdvertisedParallelWorkerMinimum() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-parallel-minimum-three",
            scriptName: "mock-ralph-loop-parallel-minimum-three",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        let commandLogURL = fixture.rootURL.appendingPathComponent("command-log-minimum.txt", isDirectory: false)

        let executionControls = MachineExecutionControls(
            runners: RalphMockCLITestSupport.defaultExecutionControls.runners,
            reasoningEfforts: RalphMockCLITestSupport.defaultExecutionControls.reasoningEfforts,
            parallelWorkers: MachineParallelWorkersControl(
                min: 3,
                max: 6,
                defaultMissingValue: 3
            )
        )
        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 2),
                executionControls: executionControls
            ),
            in: fixture.rootURL,
            name: "config-resolve-minimum-three.json"
        )

        let script = """
            #!/bin/sh
            printf '%s\\n' "$*" >> "\(commandLogURL.path)"

            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine run loop --resume --max-tasks 0 --parallel 3"*)
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-PARALLEL-MINIMUM","phase":null,"message":null,"payload":null}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
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

        workspace.startLoop(parallelWorkers: 3)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !workspace.isRunning && workspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)

        let commandLog = try String(contentsOf: commandLogURL, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("--no-color machine run loop --resume --max-tasks 0 --parallel 3"))
        XCTAssertFalse(commandLog.contains("--parallel 2"))
    }
}
