/**
 MachineContractTests

 Purpose:
 - Verify shared RalphMac machine-contract decoding and version gates.

 Responsibilities:
 - Verify shared RalphMac machine-contract decoding and version gates.
 - Prove unsupported machine versions fail fast instead of silently decoding.
 - Cover contract-expansion regressions that would reintroduce drift.

 Does not handle:
 - End-to-end CLI subprocess execution.
 - UI rendering or view-model behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - These tests exercise typed machine payload decoding only.
 - Versioned machine documents must use the shared RalphMachineContract layer.
 */

import XCTest
@testable import RalphCore

final class MachineContractTests: XCTestCase {
    func testDecodeRejectsUnsupportedMachineErrorVersion() {
        let data = Data("""
        {
          "version": 999,
          "code": "resource_busy",
          "message": "Resource temporarily unavailable.",
          "detail": "resource busy",
          "retryable": true
        }
        """.utf8)

        XCTAssertThrowsError(
            try RalphMachineContract.decode(
                MachineErrorDocument.self,
                from: data,
                operation: "machine error"
            )
        ) { error in
            let recovery = error as? RecoveryError
            XCTAssertEqual(recovery?.category, .versionMismatch)
            XCTAssertTrue(recovery?.message.contains("Unsupported machine error version 999") ?? false)
        }
    }

    func testDecodeRejectsUnsupportedMachineSystemInfoVersion() {
        let data = Data("""
        {
          "version": 999,
          "cli_version": "0.4.0"
        }
        """.utf8)

        XCTAssertThrowsError(
            try RalphMachineContract.decode(
                MachineSystemInfoDocument.self,
                from: data,
                operation: "check CLI version"
            )
        ) { error in
            let recovery = error as? RecoveryError
            XCTAssertEqual(recovery?.category, .versionMismatch)
            XCTAssertTrue(recovery?.message.contains("Unsupported machine system info version 999") ?? false)
        }
    }

    func testDecodeRejectsUnsupportedQueueValidateVersion() {
        let data = Data("""
        {
          "version": 999,
          "valid": true,
          "warnings": [],
          "continuation": {
            "headline": "Queue valid",
            "detail": "No repairs needed.",
            "next_steps": []
          }
        }
        """.utf8)

        XCTAssertThrowsError(
            try RalphMachineContract.decode(
                MachineQueueValidateDocument.self,
                from: data,
                operation: "queue validate"
            )
        )
    }

    func testDecodeAcceptsMachineRunStopDocument() throws {
        let data = Data("""
        {
          "version": 1,
          "dry_run": false,
          "action": "created",
          "paths": {
            "repo_root": "/tmp/repo",
            "queue_path": "/tmp/repo/.ralph/queue.jsonc",
            "done_path": "/tmp/repo/.ralph/done.jsonc",
            "project_config_path": "/tmp/repo/.ralph/config.jsonc",
            "global_config_path": null
          },
          "marker": {
            "path": "/tmp/repo/.ralph/cache/stop_requested",
            "existed_before": false,
            "exists_after": true
          },
          "blocking": null,
          "continuation": {
            "headline": "Stop request recorded.",
            "detail": "The stop marker is recorded.",
            "next_steps": []
          }
        }
        """.utf8)

        let document = try RalphMachineContract.decode(
            MachineRunStopDocument.self,
            from: data,
            operation: "machine run stop"
        )
        XCTAssertEqual(document.action, .created)
        XCTAssertEqual(document.marker.existsAfter, true)
    }

    func testDecodeRejectsUnsupportedRunEventVersion() {
        let data = Data("""
        {
          "version": 999,
          "kind": "blocked_state_changed",
          "timestamp": "2026-04-23T12:00:00Z",
          "message": "blocked",
          "payload": {
            "status": "blocked",
            "reason": { "kind": "dependency_blocked", "blocked_tasks": 1 },
            "task_id": null,
            "message": "blocked",
            "detail": "detail"
          }
        }
        """.utf8)

        XCTAssertThrowsError(
            try RalphMachineContract.decode(
                WorkspaceRunnerController.MachineRunEventEnvelope.self,
                from: data,
                operation: "run event"
            )
        )
    }

    func testRunOutputDecoderSurfacesUnsupportedRunEventVersion() {
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()

        let items = decoder.append("""
        {"version":999,"kind":"run_started","task_id":"RQ-1","phase":null,"message":null,"payload":null}

        """)

        XCTAssertEqual(items.count, 1)
        guard case .rawText(let text) = items[0] else {
            return XCTFail("expected version mismatch to surface as console text")
        }
        XCTAssertTrue(text.contains("Unsupported machine run event version 999"))
    }

    func testRunOutputDecoderRejectsUnknownBlockingKind() {
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()

        let items = decoder.append("{" +
            "\"version\":3," +
            "\"kind\":\"blocked_state_changed\"," +
            "\"timestamp\":\"2026-04-23T12:00:00Z\"," +
            "\"message\":\"blocked\"," +
            "\"payload\":{\"status\":\"blocked\",\"reason\":{\"kind\":\"totally_new_kind\"},\"task_id\":null,\"message\":\"blocked\",\"detail\":\"detail\"}}\n")

        XCTAssertEqual(items.count, 1)
        guard case .rawText(let text) = items[0] else {
            return XCTFail("expected undecodable payload to remain raw text")
        }
        XCTAssertTrue(text.contains("totally_new_kind"))
    }

    func testRunOutputDecoderDropsOversizedPartialLine() {
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()

        let items = decoder.append(String(repeating: "x", count: 1_100_000))

        XCTAssertEqual(items.count, 1)
        guard case .rawText(let warning) = items[0] else {
            return XCTFail("expected oversized partial line warning")
        }
        XCTAssertTrue(warning.contains("machine output line exceeded app buffer limit"))
        XCTAssertTrue(decoder.finish().isEmpty)
    }

    @MainActor
    func testRunEventSkipsConfigApplyOnNestedVersionMismatch() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "nested-config-version")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()

        let items = decoder.append("""
        {"version":3,"kind":"config_resolved","payload":{"config":{"version":999,"paths":{"repo_root":"/tmp/bad-root","queue_path":"/tmp/bad-queue.jsonc","done_path":"/tmp/bad-done.jsonc","project_config_path":null,"global_config_path":null},"safety":{"repo_trusted":false,"dirty_repo":false,"git_publish_mode":"never","approval_mode":null,"ci_gate_enabled":false,"git_revert_mode":"ask","parallel_configured":false,"execution_interactivity":"noninteractive","interactive_approval_supported":false},"config":{},"execution_controls":{"runners":[],"reasoning_efforts":["low","medium","high","xhigh"],"parallel_workers":{"min":2,"max":255,"default_missing_value":2}},"resume_preview":null}}}

        """)

        guard case .event(let event) = items.first else {
            return XCTFail("expected decoded config event")
        }

        workspace.runnerController.applyMachineRunOutputItem(.event(event), workspace: workspace)
        workspace.runState.flushConsoleRenderState()

        XCTAssertNil(workspace.resolvedQueueFileURL)
        XCTAssertEqual(workspace.diagnosticsState.lastRecoveryError?.category, .versionMismatch)
        XCTAssertTrue(workspace.output.contains("Unsupported machine config resolve version 999"))
    }

    @MainActor
    func testWorkspaceOverviewRejectsNestedContractVersionMismatchBeforeApplying() throws {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "overview-nested-version")
        let workspace = Workspace(
            workingDirectoryURL: workspaceURL,
            bootstrapRepositoryStateOnInit: false
        )
        let goodQueue = RalphMockCLITestSupport.queueReadDocument(
            workspaceURL: workspaceURL,
            activeTasks: [
                RalphMockCLITestSupport.task(
                    id: "RQ-9999",
                    status: .todo,
                    title: "Should not apply",
                    priority: .medium
                )
            ],
            nextRunnableTaskID: "RQ-9999"
        )
        let badQueue = MachineQueueReadDocument(
            version: 999,
            paths: goodQueue.paths,
            active: goodQueue.active,
            done: goodQueue.done,
            nextRunnableTaskID: goodQueue.nextRunnableTaskID,
            runnability: goodQueue.runnability
        )
        let document = MachineWorkspaceOverviewDocument(
            version: MachineWorkspaceOverviewDocument.expectedVersion,
            queue: badQueue,
            config: RalphMockCLITestSupport.configResolveDocument(workspaceURL: workspaceURL)
        )

        XCTAssertThrowsError(try workspace.validateWorkspaceOverviewDocument(document)) { error in
            XCTAssertEqual((error as? RecoveryError)?.category, .versionMismatch)
        }
        XCTAssertTrue(workspace.taskState.tasks.isEmpty)
        XCTAssertNil(workspace.identityState.resolvedPaths)
    }

    @MainActor
    func testApplyConfigResolveDocumentPreservesCustomResolvedPaths() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "custom-resolved-paths")
        let workspace = Workspace(
            workingDirectoryURL: workspaceURL,
            bootstrapRepositoryStateOnInit: false
        )
        let overrides = RalphMockCLITestSupport.MockResolvedPathOverrides(
            queueURL: workspaceURL.appendingPathComponent("state/custom-queue.jsonc", isDirectory: false),
            doneURL: workspaceURL.appendingPathComponent("state/custom-done.jsonc", isDirectory: false),
            projectConfigURL: workspaceURL.appendingPathComponent(".ralph/custom-config.jsonc", isDirectory: false)
        )
        let document = RalphMockCLITestSupport.configResolveDocument(
            workspaceURL: workspaceURL,
            pathOverrides: overrides
        )

        workspace.runnerController.applyConfigResolveDocument(document, workspace: workspace)

        XCTAssertEqual(workspace.identityState.resolvedPaths, document.paths)
        XCTAssertEqual(workspace.queueFileURL.path, document.paths.queuePath)
        XCTAssertEqual(workspace.doneFileURL.path, document.paths.donePath)
        XCTAssertEqual(workspace.projectConfigFileURL?.path, document.paths.projectConfigPath)
    }
}
