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
              *"--no-color machine run one --resume --id RQ-4242"*)
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

    func test_loadRunnerConfiguration_appliesResumePreview() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-resume-preview",
            scriptName: "mock-ralph-resume-preview"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "model-test", iterations: 1),
                resumePreview: MachineResumeDecision(
                    status: "refusing_to_resume",
                    scope: "run_session",
                    reason: "session_timed_out_requires_confirmation",
                    taskID: "RQ-9000",
                    message: "Resume: refusing to continue timed-out session RQ-9000 without explicit confirmation.",
                    detail: "The saved session is older than the configured safety threshold."
                )
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

        XCTAssertEqual(workspace.runState.resumeState?.status, .refusingToResume)
        XCTAssertEqual(workspace.runState.resumeState?.taskID, "RQ-9000")
        XCTAssertEqual(
            workspace.runState.resumeState?.message,
            "Resume: refusing to continue timed-out session RQ-9000 without explicit confirmation."
        )
        XCTAssertEqual(workspace.runState.blockingState?.status, .stalled)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .runnerRecovery(
                scope: "run_session",
                reason: "session_timed_out_requires_confirmation",
                taskID: "RQ-9000"
            )
        )
    }

    func test_loadTasks_appliesPreflightBlockingStateAndClearsNextTask() async throws {
        var workspace: Workspace!
        let blockedTask = RalphMockCLITestSupport.task(
            id: "RQ-9001",
            status: .todo,
            title: "Blocked until future schedule",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-blocking-preflight",
            scriptName: "mock-ralph-blocking-preflight",
            seedQueueTasks: [blockedTask]
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let runnability = RalphJSONValue.object([
            "summary": .object([
                "blocking": .object([
                    "status": .string("waiting"),
                    "reason": .object([
                        "kind": .string("schedule_blocked"),
                        "blocked_tasks": .number(1),
                        "next_runnable_at": .string("2026-12-31T00:00:00Z"),
                        "seconds_until_next_runnable": .number(86400)
                    ]),
                    "task_id": .null,
                    "message": .string("Ralph is waiting for scheduled work to become runnable."),
                    "detail": .string("1 candidate task(s) are scheduled for the future. The next one becomes runnable at 2026-12-31T00:00:00Z (86400s remaining).")
                ])
            ])
        ])
        let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: [blockedTask],
                nextRunnableTaskID: nil,
                runnability: runnability
            ),
            in: fixture.rootURL,
            name: "queue-read.json"
        )

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "queue" ] && [ "$4" = "read" ]; then
              cat "\(queueReadURL.path)"
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

        await workspace.loadTasks(retryConfiguration: .minimal)

        XCTAssertNil(workspace.taskState.nextRunnableTaskID)
        XCTAssertNil(workspace.nextTask())
        XCTAssertEqual(workspace.runState.blockingState?.status, .waiting)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .scheduleBlocked(
                blockedTasks: 1,
                nextRunnableAt: "2026-12-31T00:00:00Z",
                secondsUntilNextRunnable: 86400
            )
        )
    }

    func test_runNextTask_appliesResumeDecisionEvent() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "resume-decision-event")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"resume_decision\",\"task_id\":\"RQ-7777\",\"phase\":null,\"message\":\"Resume: continuing the interrupted session for task RQ-7777.\",\"payload\":{\"status\":\"resuming_same_session\",\"scope\":\"run_session\",\"reason\":\"session_valid\",\"task_id\":\"RQ-7777\",\"message\":\"Resume: continuing the interrupted session for task RQ-7777.\",\"detail\":\"Saved session is current and will resume from phase 2.\"}}\n"
        )

        guard case .event(let event) = items.first else {
            return XCTFail("expected decoded run event")
        }

        workspace.runnerController.applyMachineRunOutputItem(.event(event), workspace: workspace)

        XCTAssertEqual(workspace.runState.resumeState?.status, .resumingSameSession)
        XCTAssertEqual(workspace.runState.resumeState?.taskID, "RQ-7777")
        XCTAssertTrue(workspace.output.contains("Resume: continuing the interrupted session for task RQ-7777."))
    }

    func test_runNextTask_deduplicatesRunnerRecoveryConsoleNarration() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "runner-recovery-event")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"resume_decision\",\"task_id\":\"RQ-7777\",\"phase\":null,\"message\":\"Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.\",\"payload\":{\"status\":\"refusing_to_resume\",\"scope\":\"run_session\",\"reason\":\"session_timed_out_requires_confirmation\",\"task_id\":\"RQ-7777\",\"message\":\"Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.\",\"detail\":\"The saved session is 25 hour(s) old, exceeding the configured 24-hour safety threshold.\"}}\n"
                + "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":\"RQ-7777\",\"phase\":null,\"message\":\"Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.\",\"payload\":{\"status\":\"stalled\",\"reason\":{\"kind\":\"runner_recovery\",\"scope\":\"run_session\",\"reason\":\"session_timed_out_requires_confirmation\",\"task_id\":\"RQ-7777\"},\"task_id\":\"RQ-7777\",\"message\":\"Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.\",\"detail\":\"The saved session is 25 hour(s) old, exceeding the configured 24-hour safety threshold.\"}}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertEqual(workspace.runState.blockingState?.status, .stalled)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .runnerRecovery(
                scope: "run_session",
                reason: "session_timed_out_requires_confirmation",
                taskID: "RQ-7777"
            )
        )
        XCTAssertEqual(
            workspace.output.components(separatedBy: "Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.").count,
            2,
            "message should appear exactly once in console output"
        )
    }

    func test_runNextTask_appliesBlockingStateEvent() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "blocking-state-event")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\"}}\n"
        )

        guard case .event(let event) = items.first else {
            return XCTFail("expected decoded blocked-state event")
        }

        workspace.runnerController.applyMachineRunOutputItem(.event(event), workspace: workspace)

        XCTAssertEqual(workspace.runState.blockingState?.status, .blocked)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .dependencyBlocked(blockedTasks: 2)
        )
        XCTAssertTrue(workspace.output.contains("Ralph is blocked by unfinished dependencies."))
    }

    func test_runSummary_appliesBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "blocking-state-summary")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":2,\"task_id\":null,\"exit_code\":0,\"outcome\":\"blocked\",\"blocking\":{\"status\":\"waiting\",\"reason\":{\"kind\":\"schedule_blocked\",\"blocked_tasks\":1,\"next_runnable_at\":\"2026-12-31T00:00:00Z\",\"seconds_until_next_runnable\":86400},\"task_id\":null,\"message\":\"Ralph is waiting for scheduled work to become runnable.\",\"detail\":\"1 candidate task(s) are scheduled for the future. The next one becomes runnable at 2026-12-31T00:00:00Z (86400s remaining).\"}}\n"
        )

        guard case .summary(let summary) = items.first else {
            return XCTFail("expected decoded run summary")
        }

        workspace.runnerController.applyMachineRunOutputItem(.summary(summary), workspace: workspace)

        XCTAssertEqual(workspace.runState.blockingState?.status, .waiting)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .scheduleBlocked(
                blockedTasks: 1,
                nextRunnableAt: "2026-12-31T00:00:00Z",
                secondsUntilNextRunnable: 86400
            )
        )
    }

    func test_runControlPreviewTask_prefersSelectedTodoTask() {
        let workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-preview"))
        workspace.tasks = [
            RalphTask(id: "RQ-1001", status: .todo, title: "First", priority: .medium),
            RalphTask(id: "RQ-1002", status: .todo, title: "Second", priority: .high)
        ]

        workspace.taskState.nextRunnableTaskID = "RQ-1001"

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
              *"--no-color machine run one --resume --id RQ-LOOP-1"*)
              echo '{"version":1,"kind":"run_started","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":null}'
              echo '{"version":1,"kind":"runner_output","task_id":"RQ-LOOP-1","phase":null,"message":null,"payload":{"text":"running first\\n"}}'
              echo '{"version":1,"task_id":"RQ-LOOP-1","exit_code":0,"outcome":"success"}'
              echo "done" > "$state_file"
              exit 0
              ;;
              *"--no-color machine run one --resume --id RQ-LOOP-2"*)
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
        workspace = RalphMockCLITestSupport.makeWorkspaceWithoutInitialRefresh(
            workingDirectoryURL: fixture.workspaceURL,
            client: client
        )
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

    func test_loadParallelStatus_decodesSharedContinuationDocument() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-parallel-status")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let parallelStatusURL = fixture.rootURL.appendingPathComponent("parallel-status.json", isDirectory: false)
        try """
            {"version":2,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Parallel workers are active on target branch main.","blocking":null,"next_steps":[{"title":"Inspect worker snapshot","command":"ralph run parallel status --json","detail":"Review lifecycle counts and retained worker details."}]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-7001","workspace_path":"\(fixture.workspaceURL.appendingPathComponent(".ralph/workspaces/RQ-7001", isDirectory: true).path)","lifecycle":"running","started_at":"2026-03-22T00:00:00Z","push_attempts":1}]}}
            """.write(to: parallelStatusURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "run" ] && [ "$4" = "parallel-status" ]; then
              cat "\(parallelStatusURL.path)"
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

        await workspace.loadParallelStatus(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.runState.parallelStatus?.headline, "Parallel execution is in progress.")
        XCTAssertEqual(workspace.runState.parallelStatus?.snapshot.targetBranch, "main")
        XCTAssertEqual(workspace.runState.parallelStatus?.snapshot.lifecycleCounts.running, 1)
        XCTAssertEqual(workspace.runState.parallelStatus?.nextSteps.first?.command, "ralph run parallel status --json")
    }

    func test_loadParallelStatus_rejectsUnsupportedVersion_andClearsRetainedState() async throws {
        try await assertParallelStatusLoadFailure(
            prefix: "ralph-workspace-parallel-version-failure",
            payload: """
                {"version":99,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-7001","lifecycle":"running"}]}}
                """
        )
    }

    func test_loadParallelStatus_rejectsMalformedJSON_andClearsRetainedState() async throws {
        try await assertParallelStatusLoadFailure(
            prefix: "ralph-workspace-parallel-json-failure",
            payload: """
                {"version":2,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},
                """
        )
    }

    func test_loadParallelStatus_rejectsMissingRequiredFields_andClearsRetainedState() async throws {
        try await assertParallelStatusLoadFailure(
            prefix: "ralph-workspace-parallel-missing-field-failure",
            payload: """
                {"version":2,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[]}}
                """
        )
    }

    func test_runState_runControlDisplayBlockingState_suppressesSharedAndResumeDuplicates() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-display-blocking")
        )
        let sharedBlocking = Workspace.BlockingState(
            status: .blocked,
            reason: .dependencyBlocked(blockedTasks: 2),
            taskID: nil,
            message: "Ralph is blocked by unfinished dependencies.",
            detail: "2 candidate task(s) are waiting on dependency completion."
        )
        workspace.runState.parallelStatus = retainedParallelStatus(blocking: sharedBlocking)
        workspace.runState.blockingState = sharedBlocking
        XCTAssertNil(workspace.runState.runControlDisplayBlockingState)

        workspace.runState.parallelStatus = nil
        workspace.runState.resumeState = Workspace.ResumeState(
            status: .refusingToResume,
            scope: "run_session",
            reason: "session_timed_out_requires_confirmation",
            taskID: "RQ-7777",
            message: "Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.",
            detail: "The saved session is older than the configured safety threshold."
        )
        workspace.runState.blockingState = Workspace.BlockingState(
            status: .stalled,
            reason: .runnerRecovery(
                scope: "run_session",
                reason: "session_timed_out_requires_confirmation",
                taskID: "RQ-7777"
            ),
            taskID: "RQ-7777",
            message: "Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.",
            detail: "The saved session is older than the configured safety threshold."
        )
        XCTAssertNil(workspace.runState.runControlDisplayBlockingState)

        let scheduleBlocking = Workspace.BlockingState(
            status: .waiting,
            reason: .scheduleBlocked(
                blockedTasks: 1,
                nextRunnableAt: "2026-12-31T00:00:00Z",
                secondsUntilNextRunnable: 86400
            ),
            taskID: nil,
            message: "Ralph is waiting for scheduled work to become runnable.",
            detail: "1 candidate task(s) are scheduled for the future."
        )
        workspace.runState.blockingState = scheduleBlocking
        XCTAssertEqual(workspace.runState.runControlDisplayBlockingState, scheduleBlocking)
    }

    func test_runState_shouldShowRunControlParallelStatus_tracksLoadingErrorConfiguredAndRetainedState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-parallel-visibility")
        )

        XCTAssertFalse(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.parallelStatusLoading = true
        XCTAssertTrue(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.parallelStatusLoading = false
        workspace.runState.parallelStatusErrorMessage = "Failed to load shared parallel status."
        XCTAssertTrue(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.parallelStatusErrorMessage = nil
        workspace.runState.currentRunnerConfig = Workspace.RunnerConfig(
            model: nil,
            phases: nil,
            maxIterations: nil,
            safety: Workspace.RunnerSafetySummary(
                repoTrusted: true,
                dirtyRepo: false,
                gitPublishMode: "off",
                approvalMode: "default",
                ciGateEnabled: true,
                gitRevertMode: "ask",
                parallelConfigured: true,
                executionInteractivity: "noninteractive_streaming",
                interactiveApprovalSupported: false
            )
        )
        XCTAssertTrue(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.currentRunnerConfig = nil
        workspace.runState.parallelStatus = retainedParallelStatus()
        XCTAssertTrue(workspace.runState.shouldShowRunControlParallelStatus)
    }

    func test_refreshRunControlStatusData_reloadsConfigAndParallelStatusWithoutQueueRead() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-control-status-refresh",
            logFileName: "commands.log",
            seedQueueTasks: [
                RalphMockCLITestSupport.task(
                    id: "RQ-2222",
                    status: .todo,
                    title: "Leave task state alone",
                    priority: .medium,
                    createdAt: "2026-03-10T00:00:00Z",
                    updatedAt: "2026-03-10T00:00:00Z"
                )
            ]
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let safety = MachineConfigSafetySummary(
            repoTrusted: true,
            dirtyRepo: false,
            gitPublishMode: "off",
            approvalMode: "default",
            ciGateEnabled: true,
            gitRevertMode: "ask",
            parallelConfigured: true,
            executionInteractivity: "noninteractive_streaming",
            interactiveApprovalSupported: false
        )
        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                safety: safety
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let parallelStatusURL = fixture.rootURL.appendingPathComponent("parallel-status.json", isDirectory: false)
        try """
            {"version":2,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-2222","lifecycle":"running"}]}}
            """.write(to: parallelStatusURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            echo "$*" >> "\(fixture.logURL!.path)"
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "run" ] && [ "$4" = "parallel-status" ]; then
              cat "\(parallelStatusURL.path)"
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
        let seededTask = RalphMockCLITestSupport.task(
            id: "RQ-2222",
            status: .todo,
            title: "Leave task state alone",
            priority: .medium,
            createdAt: "2026-03-10T00:00:00Z",
            updatedAt: "2026-03-10T00:00:00Z"
        )
        workspace.tasks = [seededTask]
        workspace.taskState.nextRunnableTaskID = seededTask.id

        await workspace.refreshRunControlStatusData()

        XCTAssertEqual(workspace.tasks.map(\.id), ["RQ-2222"])
        XCTAssertEqual(workspace.taskState.nextRunnableTaskID, "RQ-2222")
        XCTAssertEqual(workspace.runState.parallelStatus?.snapshot.lifecycleCounts.running, 1)
        let commandLog = try String(contentsOf: fixture.logURL!, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("machine config resolve"))
        XCTAssertTrue(commandLog.contains("machine run parallel-status"))
        XCTAssertFalse(commandLog.contains("machine queue read"))
    }

    func test_refreshRepositoryState_clearsInactiveParallelStatusWhenParallelNotConfigured() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-parallel-refresh-clear",
            logFileName: "commands.log"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(workspaceURL: fixture.workspaceURL),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )

        let script = """
            #!/bin/sh
            echo "$*" >> "\(fixture.logURL!.path)"
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
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
        workspace.runState.parallelStatus = Workspace.ParallelStatus(
            headline: "No retained workers.",
            detail: "Nothing is running.",
            blocking: nil,
            nextSteps: [],
            snapshot: ParallelStatusSnapshot(schemaVersion: 3, targetBranch: "main", workers: [])
        )
        workspace.runState.parallelStatusErrorMessage = "stale error"

        await workspace.refreshRepositoryState(retryConfiguration: .minimal, includeCLISpec: false)

        XCTAssertNil(workspace.runState.parallelStatus)
        XCTAssertNil(workspace.runState.parallelStatusErrorMessage)
        let commandLog = try String(contentsOf: fixture.logURL!, encoding: .utf8)
        XCTAssertFalse(commandLog.contains("parallel-status"))
    }

    func test_refreshRepositoryState_reloadsMeaningfulParallelStatusWhenParallelNotConfigured() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-parallel-refresh-reload",
            logFileName: "commands.log"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(workspaceURL: fixture.workspaceURL),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let parallelStatusURL = fixture.rootURL.appendingPathComponent("parallel-status.json", isDirectory: false)
        try """
            {"version":2,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[{"title":"Inspect worker snapshot","command":"ralph run parallel status --json","detail":"Review retained worker lifecycles."}]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-8111","workspace_path":"\(fixture.workspaceURL.appendingPathComponent(".ralph/workspaces/RQ-8111", isDirectory: true).path)","lifecycle":"running","started_at":"2026-03-22T00:00:00Z","push_attempts":1}]}}
            """.write(to: parallelStatusURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            echo "$*" >> "\(fixture.logURL!.path)"
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              cat "\(configResolveURL.path)"
              exit 0
            fi
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "run" ] && [ "$4" = "parallel-status" ]; then
              cat "\(parallelStatusURL.path)"
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
        workspace.runState.parallelStatus = Workspace.ParallelStatus(
            headline: "Retained workers detected.",
            detail: "Reload the shared snapshot.",
            blocking: nil,
            nextSteps: [],
            snapshot: ParallelStatusSnapshot(
                schemaVersion: 3,
                targetBranch: "main",
                workers: [ParallelWorkerStatus(taskID: "RQ-8111", lifecycle: .running)]
            )
        )

        await workspace.refreshRepositoryState(retryConfiguration: .minimal, includeCLISpec: false)

        XCTAssertEqual(workspace.runState.parallelStatus?.headline, "Parallel execution is in progress.")
        XCTAssertEqual(workspace.runState.parallelStatus?.snapshot.lifecycleCounts.running, 1)
        let commandLog = try String(contentsOf: fixture.logURL!, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("parallel-status"))
    }

    func test_beginRepositoryRetarget_clearsParallelStatusState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "parallel-status-retarget")
        )
        workspace.runState.parallelStatus = Workspace.ParallelStatus(
            headline: "Parallel execution is stalled on queue lock recovery.",
            detail: "Unlock the queue before continuing.",
            blocking: nil,
            nextSteps: [],
            snapshot: ParallelStatusSnapshot(
                schemaVersion: 3,
                targetBranch: "main",
                workers: []
            )
        )
        workspace.runState.parallelStatusLoading = true
        workspace.runState.parallelStatusErrorMessage = "should clear"

        _ = workspace.beginRepositoryRetarget(
            to: RalphCoreTestSupport.workspaceURL(label: "parallel-status-retarget-next")
        )

        XCTAssertNil(workspace.runState.parallelStatus)
        XCTAssertFalse(workspace.runState.parallelStatusLoading)
        XCTAssertNil(workspace.runState.parallelStatusErrorMessage)
    }

    private func assertParallelStatusLoadFailure(prefix: String, payload: String) async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: prefix)
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let parallelStatusURL = fixture.rootURL.appendingPathComponent("parallel-status.json", isDirectory: false)
        try payload.write(to: parallelStatusURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "run" ] && [ "$4" = "parallel-status" ]; then
              cat "\(parallelStatusURL.path)"
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
        workspace.runState.parallelStatus = retainedParallelStatus()
        workspace.runState.parallelStatusErrorMessage = "stale error"

        await workspace.loadParallelStatus(retryConfiguration: .minimal)

        XCTAssertNil(workspace.runState.parallelStatus)
        XCTAssertEqual(workspace.runState.parallelStatusErrorMessage, "Failed to load shared parallel status.")
    }

    private func retainedParallelStatus(
        blocking: Workspace.BlockingState? = nil,
        workers: [ParallelWorkerStatus] = [ParallelWorkerStatus(taskID: "RQ-7001", lifecycle: .running)]
    ) -> Workspace.ParallelStatus {
        Workspace.ParallelStatus(
            headline: "Parallel execution is in progress.",
            detail: "Retained worker state still exists for this repository.",
            blocking: blocking,
            nextSteps: [],
            snapshot: ParallelStatusSnapshot(schemaVersion: 3, targetBranch: "main", workers: workers)
        )
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
