/**
 WorkspaceLoopStopTests

 Purpose:
 - Validate loop-stop signaling, blocking preservation, and recovery-path behavior.

 Responsibilities:
 - Validate machine stop-marker semantics, operator-state actions, and structured stop-failure handling.

 Does not handle:
 - Active-run cancellation or loop-start command construction.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceLoopStopTests: WorkspacePerformanceTestCase {
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
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop running\\n"}}'
              while [ ! -f "\(stopSignalURL.path)" ]; do
                sleep 0.05
              done
              sleep 0.4
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop stopping\\n"}}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
              exit 0
              ;;
              *"--no-color machine run stop"*)
              echo "stop command received" > "\(stopSignalURL.path)"
              printf '%s\n' '{"version":1,"dry_run":false,"action":"created","paths":{"repo_root":"\(fixture.workspaceURL.path)","queue_path":"\(fixture.workspaceURL.path)/.ralph/queue.jsonc","done_path":"\(fixture.workspaceURL.path)/.ralph/done.jsonc","project_config_path":"\(fixture.workspaceURL.path)/.ralph/config.jsonc","global_config_path":null},"marker":{"path":"\(fixture.workspaceURL.path)/.ralph/cache/stop_requested","existed_before":false,"exists_after":true},"blocking":null,"continuation":{"headline":"Stop request recorded.","detail":"The stop marker is recorded.","next_steps":[]}}'
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
        XCTAssertTrue(commandLog.contains("--no-color machine run stop"))
        XCTAssertTrue(workspace.output.contains("loop running"))
        XCTAssertTrue(workspace.output.contains("loop stopping"))
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertFalse(workspace.isLoopMode)
        XCTAssertFalse(workspace.stopAfterCurrent)
    }

    func test_stopLoop_keepsStopRequestedWhenMachineRunStopReportsAlreadyPresent() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-stop-already-present",
            scriptName: "mock-ralph-loop-stop-already-present",
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
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop running\\n"}}'
              while [ ! -f "\(stopSignalURL.path)" ]; do
                sleep 0.05
              done
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop stopping\\n"}}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
              exit 0
              ;;
              *"--no-color machine run stop"*)
              echo "stop command received" > "\(stopSignalURL.path)"
              printf '%s\n' '{"version":1,"dry_run":false,"action":"already_present","paths":{"repo_root":"\(fixture.workspaceURL.path)","queue_path":"\(fixture.workspaceURL.path)/.ralph/queue.jsonc","done_path":"\(fixture.workspaceURL.path)/.ralph/done.jsonc","project_config_path":"\(fixture.workspaceURL.path)/.ralph/config.jsonc","global_config_path":null},"marker":{"path":"\(fixture.workspaceURL.path)/.ralph/cache/stop_requested","existed_before":true,"exists_after":true},"blocking":null,"continuation":{"headline":"Stop request is already recorded.","detail":"The stop marker already exists.","next_steps":[]}}'
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
        let currentWorkspace = workspace!
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        currentWorkspace.startLoop()
        let loopStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            currentWorkspace.output.contains("loop running")
        }
        XCTAssertTrue(loopStarted)

        currentWorkspace.stopLoop()

        let stopRequested = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            FileManager.default.fileExists(atPath: stopSignalURL.path)
        }
        XCTAssertTrue(stopRequested)
        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !currentWorkspace.isRunning && currentWorkspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)
        XCTAssertNil(currentWorkspace.runState.errorMessage)
    }

    func test_stopLoop_preservesExistingBlockingWhenStopDocumentHasNoBlocking() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-stop-preserve-blocking",
            scriptName: "mock-ralph-loop-stop-preserve-blocking",
            seedQueueTasks: []
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
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
            case "$*" in
              *"--no-color machine config resolve"*)
              cat "\(configResolveURL.path)"
              exit 0
              ;;
              *"--no-color machine run loop --resume --max-tasks 0"*)
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop running\\n"}}'
              while [ ! -f "\(stopSignalURL.path)" ]; do
                sleep 0.05
              done
              sleep 0.4
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop stopping\\n"}}'
              echo '{"version":2,"task_id":null,"exit_code":0,"outcome":"completed"}'
              exit 0
              ;;
              *"--no-color machine run stop"*)
              echo "stop command received" > "\(stopSignalURL.path)"
              printf '%s\n' '{"version":1,"dry_run":false,"action":"created","paths":{"repo_root":"\(fixture.workspaceURL.path)","queue_path":"\(fixture.workspaceURL.path)/.ralph/queue.jsonc","done_path":"\(fixture.workspaceURL.path)/.ralph/done.jsonc","project_config_path":"\(fixture.workspaceURL.path)/.ralph/config.jsonc","global_config_path":null},"marker":{"path":"\(fixture.workspaceURL.path)/.ralph/cache/stop_requested","existed_before":false,"exists_after":true},"blocking":null,"continuation":{"headline":"Stop request recorded.","detail":"The stop marker is recorded.","next_steps":[]}}'
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
        let currentWorkspace = workspace!
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        currentWorkspace.startLoop()
        let loopStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            currentWorkspace.output.contains("loop running")
        }
        XCTAssertTrue(loopStarted)

        currentWorkspace.runState.setLiveBlockingState(
            Workspace.BlockingState(
                status: .waiting,
                reason: .dependencyBlocked(blockedTasks: 2),
                taskID: nil,
                message: "Waiting on dependencies.",
                detail: "Two tasks must finish before more work can start."
            )
        )

        currentWorkspace.stopLoop()

        let blockingPreserved = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            currentWorkspace.runState.blockingState?.message == "Waiting on dependencies."
        }
        XCTAssertTrue(blockingPreserved)

        let loopFinished = await WorkspacePerformanceTestSupport.waitFor(timeout: 2.0) {
            !currentWorkspace.isRunning && currentWorkspace.lastExitStatus?.code == 0
        }
        XCTAssertTrue(loopFinished)
    }

    func test_runControlOperatorState_exposesStopAfterCurrentActionDuringLoopBlocking() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-stop-after-current-action")
        )
        workspace.isLoopMode = true
        workspace.runState.setLiveBlockingState(
            Workspace.BlockingState(
                status: .blocked,
                reason: .dependencyBlocked(blockedTasks: 2),
                taskID: nil,
                message: "Ralph is blocked by unfinished dependencies.",
                detail: "2 candidate task(s) are waiting on dependency completion."
            )
        )

        let actions = workspace.runState.runControlOperatorState?.actions ?? []
        XCTAssertTrue(actions.contains { action in
            guard case .native(.stopAfterCurrent) = action.disposition else { return false }
            return true
        })
    }

    func test_stopLoop_structuredFailureClearsStopRequestedAndShowsRecoveryError() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-loop-stop-failure",
            scriptName: "mock-ralph-loop-stop-failure",
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
              echo '{"version":3,"kind":"run_started","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":null}'
              echo '{"version":3,"kind":"runner_output","task_id":"RQ-LOOP-STOP","phase":null,"message":null,"payload":{"text":"loop running\\n"}}'
              while :; do
                sleep 0.1
              done
              ;;
              *"--no-color machine run stop"*)
              printf '%s\n' '{"version":1,"code":"resource_busy","message":"Failed to record stop request.","detail":"cache directory is locked","retryable":true}' >&2
              exit 23
              ;;
            esac

            echo "unexpected args: $*" >&2
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
        let currentWorkspace = workspace!
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        currentWorkspace.startLoop()
        let loopStarted = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            currentWorkspace.output.contains("loop running")
        }
        XCTAssertTrue(loopStarted)

        currentWorkspace.stopLoop()

        let failureSurfaced = await WorkspacePerformanceTestSupport.waitFor(timeout: 1.0) {
            currentWorkspace.runState.errorMessage?.contains("Code: resource_busy") == true
        }
        XCTAssertTrue(failureSurfaced)
        XCTAssertFalse(currentWorkspace.stopAfterCurrent)
        XCTAssertTrue(currentWorkspace.diagnosticsState.showErrorRecovery)
    }
}
