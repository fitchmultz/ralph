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

    @MainActor
    func testRunEventSkipsConfigApplyOnNestedVersionMismatch() {
        let workspace = Workspace(
            workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "nested-config-version")
        )
        var decoder = WorkspaceRunnerController.MachineRunOutputDecoder()

        let items = decoder.append("""
        {"version":3,"kind":"config_resolved","payload":{"config":{"version":999,"paths":{"repo_root":"/tmp/bad-root","queue_path":"/tmp/bad-queue.jsonc","done_path":"/tmp/bad-done.jsonc","project_config_path":null,"global_config_path":null},"safety":{"repo_trusted":false,"dirty_repo":false,"git_publish_mode":"never","approval_mode":null,"ci_gate_enabled":false,"git_revert_mode":"ask","parallel_configured":false,"execution_interactivity":"noninteractive","interactive_approval_supported":false},"config":{},"resume_preview":null}}}

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
}
