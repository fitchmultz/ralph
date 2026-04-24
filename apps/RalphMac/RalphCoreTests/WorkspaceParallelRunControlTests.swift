/**
 WorkspaceParallelRunControlTests

 Purpose:
 - Validate parallel-status CLI loading, decode failures, run-control display helpers,

 Responsibilities:
 - Validate parallel-status CLI loading, decode failures, run-control display helpers,
   repository refresh interactions, and retarget clearing for parallel run-control state.

 Does not handle:
 - Primary `runNextTask` invocation, loop/cancel, or queue watcher operational surfacing.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs intentionally implement only the command paths exercised by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceParallelRunControlTests: WorkspacePerformanceTestCase {
    func test_loadParallelStatus_decodesSharedContinuationDocument() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "ralph-workspace-parallel-status")
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let parallelStatusURL = fixture.rootURL.appendingPathComponent("parallel-status.json", isDirectory: false)
        try """
            {"version":3,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Parallel workers are active on target branch main.","blocking":null,"next_steps":[{"title":"Inspect worker snapshot","command":"ralph machine run parallel-status","detail":"Review lifecycle counts and retained worker details."}]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-7001","workspace_path":"\(fixture.workspaceURL.appendingPathComponent(".ralph/workspaces/RQ-7001", isDirectory: true).path)","lifecycle":"running","started_at":"2026-03-22T00:00:00Z","push_attempts":1}]}}
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
        XCTAssertEqual(workspace.runState.parallelStatus?.nextSteps.first?.command, "ralph machine run parallel-status")
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
                {"version":3,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},
                """
        )
    }

    func test_loadParallelStatus_rejectsMissingRequiredFields_andClearsRetainedState() async throws {
        try await assertParallelStatusLoadFailure(
            prefix: "ralph-workspace-parallel-missing-field-failure",
            payload: """
                {"version":3,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[]}}
                """
        )
    }

    func test_runState_runControlOperatorState_appliesPrecedence_andDeduplicatesResumeRecovery() {
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
        workspace.runState.setLiveBlockingState(sharedBlocking)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .liveRun)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.blockingState, sharedBlocking)
        XCTAssertNil(workspace.runState.runControlOperatorState?.secondaryResumeState)

        workspace.runState.parallelStatus = nil
        workspace.runState.resumeState = Workspace.ResumeState(
            status: .refusingToResume,
            scope: "run_session",
            reason: "session_timed_out_requires_confirmation",
            taskID: "RQ-7777",
            message: "Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.",
            detail: "The saved session is older than the configured safety threshold."
        )
        workspace.runState.setLiveBlockingState(Workspace.BlockingState(
            status: .stalled,
            reason: .runnerRecovery(
                scope: "run_session",
                reason: "session_timed_out_requires_confirmation",
                taskID: "RQ-7777"
            ),
            taskID: "RQ-7777",
            message: "Resume: refusing to continue timed-out session RQ-7777 without explicit confirmation.",
            detail: "The saved session is older than the configured safety threshold."
        ))
        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .liveRun)
        XCTAssertNil(workspace.runState.runControlOperatorState?.secondaryResumeState)

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
        workspace.runState.resumeState = nil
        workspace.runState.clearLiveBlockingState()
        workspace.runState.setQueueBlockingState(scheduleBlocking)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .queueSnapshot)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.blockingState, scheduleBlocking)
    }

    func test_runState_shouldShowRunControlParallelStatus_tracksLoadingErrorConfiguredAndRetainedState() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-parallel-visibility")
        )

        XCTAssertFalse(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.runControlParallelWorkersOverride = 2
        XCTAssertTrue(workspace.runState.shouldShowRunControlParallelStatus)

        workspace.runState.runControlParallelWorkersOverride = nil
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

    func test_runState_runControlOperatorState_prefersParallelBlockingOverQueueSnapshot() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "run-control-parallel-precedence")
        )
        let queueBlocking = Workspace.BlockingState(
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
        let parallelBlocking = Workspace.BlockingState(
            status: .blocked,
            reason: .dependencyBlocked(blockedTasks: 3),
            taskID: nil,
            message: "Parallel execution is blocked on worker integration outcomes that need operator action.",
            detail: "3 worker outcomes need review."
        )

        workspace.runState.setQueueBlockingState(queueBlocking)
        workspace.runState.parallelStatus = retainedParallelStatus(blocking: parallelBlocking)

        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .parallel)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.blockingState, parallelBlocking)
    }

    func test_loadRunnerConfiguration_failure_preservesExistingQueueOperatorState() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-run-control-preserve-state",
            scriptName: "mock-ralph-run-control-preserve-state"
        )
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let queueBlocking = Workspace.BlockingState(
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

        let script = """
            #!/bin/sh
            echo "Queue lock already held at: \(fixture.workspaceURL.path)/.ralph/lock" 1>&2
            exit 1
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
        workspace.runState.setQueueBlockingState(queueBlocking)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.runState.blockingState, queueBlocking)
        XCTAssertEqual(workspace.runState.runControlOperatorState?.source, .queueSnapshot)
        XCTAssertEqual(
            workspace.runState.runnerConfigErrorMessage,
            "Queue lock requires attention"
        )
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
            {"version":3,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-2222","lifecycle":"running"}]}}
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
            {"version":3,"lifecycle_counts":{"running":1,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":1},"blocking":null,"continuation":{"headline":"Parallel execution is in progress.","detail":"Retained worker state still exists for this repository.","blocking":null,"next_steps":[{"title":"Inspect worker snapshot","command":"ralph machine run parallel-status","detail":"Review retained worker lifecycles."}]},"status":{"schema_version":3,"target_branch":"main","workers":[{"task_id":"RQ-8111","workspace_path":"\(fixture.workspaceURL.appendingPathComponent(".ralph/workspaces/RQ-8111", isDirectory: true).path)","lifecycle":"running","started_at":"2026-03-22T00:00:00Z","push_attempts":1}]}}
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

    func test_refreshRepositoryState_reloadsParallelStatusWhenRunControlOverrideRequestsParallelLoop() async throws {
        var workspace: Workspace!
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-parallel-refresh-override",
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
            {"version":3,"lifecycle_counts":{"running":0,"integrating":0,"completed":0,"failed":0,"blocked":0,"total":0},"blocking":null,"continuation":{"headline":"Parallel execution has not started.","detail":"No persisted parallel state was found for this repository. Start a coordinator run to create worker state and begin parallel execution.","blocking":null,"next_steps":[{"title":"Start parallel execution","command":"ralph machine run loop --resume --max-tasks 0 --parallel <N>","detail":"Start the coordinator with the desired worker count."}]},"status":{"schema_version":3,"workers":[],"message":"No parallel state found"}}
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
        workspace.runState.runControlParallelWorkersOverride = 2

        await workspace.refreshRepositoryState(retryConfiguration: .minimal, includeCLISpec: false)

        XCTAssertEqual(workspace.runState.parallelStatus?.headline, "Parallel execution has not started.")
        let commandLog = try String(contentsOf: fixture.logURL!, encoding: .utf8)
        XCTAssertTrue(commandLog.contains("machine run parallel-status"))
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

        XCTAssertEqual(workspace.runState.parallelStatus?.headline, "Parallel execution is in progress.")
        XCTAssertNotNil(workspace.runState.parallelStatusErrorMessage)
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
}
