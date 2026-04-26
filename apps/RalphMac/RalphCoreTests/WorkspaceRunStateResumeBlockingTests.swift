/**
 WorkspaceRunStateResumeBlockingTests

 Purpose:
 - Validate runner configuration resume preview, queue preflight blocking, machine-run

 Responsibilities:
 - Validate runner configuration resume preview, queue preflight blocking, machine-run
   output application for resume/blocking/summary, and run-control preview task selection.

 Does not handle:
 - Live `runNextTask` streaming, loop/cancel scheduling, parallel status loading, watcher health.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceRunStateResumeBlockingTests: WorkspacePerformanceTestCase {
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
        workspace.runState.flushConsoleRenderState()

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
        workspace.runState.flushConsoleRenderState()

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
        workspace.runState.flushConsoleRenderState()

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

    func test_runSummary_stoppedWithoutBlocking_clearsExistingLiveBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "summary-preserves-live-blocking")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\",\"observed_at\":\"2026-12-30T00:00:00Z\"}}\n"
                + "{\"version\":2,\"task_id\":null,\"exit_code\":0,\"outcome\":\"stopped\",\"blocking\":null}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertNil(workspace.runState.blockingState)
        XCTAssertNil(workspace.runState.runControlOperatorState)
    }

    func test_runSummary_explicitBlocking_supersedesEarlierLiveBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "summary-supersedes-live-blocking")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\",\"observed_at\":\"2026-12-30T00:00:00Z\"}}\n"
                + "{\"version\":2,\"task_id\":null,\"exit_code\":0,\"outcome\":\"no_candidates\",\"blocking\":{\"status\":\"waiting\",\"reason\":{\"kind\":\"idle\",\"include_draft\":false},\"task_id\":null,\"message\":\"Ralph is idle: no todo tasks are available.\",\"detail\":\"The queue currently has no runnable todo candidates; Ralph is waiting for new work.\",\"observed_at\":\"2026-12-31T00:00:00Z\"}}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertEqual(workspace.runState.blockingState?.status, .waiting)
        XCTAssertEqual(workspace.runState.blockingState?.reason, .idle(includeDraft: false))
        XCTAssertEqual(workspace.runState.blockingState?.observedAt, "2026-12-31T00:00:00Z")
        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .liveRun)
    }

    func test_runSummary_completedOutcomeClearsExistingLiveBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "summary-clears-live-blocking")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\"}}\n"
                + "{\"version\":2,\"task_id\":null,\"exit_code\":0,\"outcome\":\"completed\",\"blocking\":null}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertNil(workspace.runState.blockingState)
        XCTAssertNil(workspace.runState.runControlOperatorState)
    }

    func test_runSummary_failedOutcomeClearsExistingLiveBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "summary-failed-clears-live-blocking")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\"}}\n"
                + "{\"version\":2,\"task_id\":null,\"exit_code\":1,\"outcome\":\"failed\",\"blocking\":null}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertNil(workspace.runState.blockingState)
        XCTAssertNil(workspace.runState.runControlOperatorState)
    }

    func test_runSummary_unknownOutcomePreservesExistingLiveBlockingState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "summary-unknown-outcome")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()
        let items = decoder.append(
            "{\"version\":3,\"kind\":\"blocked_state_changed\",\"task_id\":null,\"phase\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"dependency_blocked\",\"blocked_tasks\":2},\"task_id\":null,\"message\":\"Ralph is blocked by unfinished dependencies.\",\"detail\":\"2 candidate task(s) are waiting on dependency completion.\"}}\n"
                + "{\"version\":2,\"task_id\":null,\"exit_code\":0,\"outcome\":\"future_mode\",\"blocking\":null}\n"
        )

        for item in items {
            workspace.runnerController.applyMachineRunOutputItem(item, workspace: workspace)
        }

        XCTAssertEqual(workspace.runState.blockingState?.status, .blocked)
        XCTAssertEqual(
            workspace.runState.blockingState?.reason,
            .dependencyBlocked(blockedTasks: 2)
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
}
