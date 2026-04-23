/**
 MachineContractTests

 Responsibilities:
 - Verify shared RalphMac machine-contract decoding and version gates.
 - Prove unsupported machine versions fail fast instead of silently decoding.
 - Cover contract-expansion regressions that would reintroduce drift.

 Does not handle:
 - End-to-end CLI subprocess execution.
 - UI rendering or view-model behavior.

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
}
