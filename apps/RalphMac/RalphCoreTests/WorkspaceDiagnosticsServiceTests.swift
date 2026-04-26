/**
 WorkspaceDiagnosticsServiceTests

 Purpose:
 - Verify queue-lock diagnostics consume structured machine unlock inspection documents.

 Responsibilities:
 - Verify queue-lock diagnostics consume structured machine unlock inspection documents.
 - Guard against regressions back to human-text parsing for queue-lock state.

 Does not handle:
 - Queue unlock mutation flows.
 - Doctor report rendering beyond the exercised snapshot path.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLI scripts emit only the commands required by each scenario.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceDiagnosticsServiceTests: RalphCoreTestCase {
    func testQueueLockDiagnosticSnapshot_usesStructuredUnlockInspectDocument() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(prefix: "workspace-diagnostics-lock-inspect")
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let doctorURL = fixture.rootURL.appendingPathComponent("doctor.json", isDirectory: false)
        try """
        {
          "version": 2,
          "blocking": {
            "status": "stalled",
            "reason": {
              "kind": "lock_blocked",
              "lock_path": "/tmp/.ralph/lock",
              "owner": "test",
              "owner_pid": 42
            },
            "task_id": null,
            "message": "Ralph is stalled on a stale queue lock.",
            "detail": "dead pid"
          },
          "report": { "success": false }
        }
        """.write(to: doctorURL, atomically: true, encoding: .utf8)

        let unlockURL = fixture.rootURL.appendingPathComponent("unlock-inspect.json", isDirectory: false)
        try """
        {
          "version": 1,
          "condition": "stale",
          "blocking": null,
          "unlock_allowed": true,
          "continuation": {
            "headline": "Queue lock is stale",
            "detail": "Safe to clear.",
            "next_steps": []
          }
        }
        """.write(to: unlockURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "doctor" ] && [ "$4" = "report" ]; then
              cat "\(doctorURL.path)"
              exit 0
            fi
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "queue" ] && [ "$4" = "unlock-inspect" ]; then
              cat "\(unlockURL.path)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, body: script)
        workspace = Workspace(workingDirectoryURL: fixture.workspaceURL, client: try RalphCLIClient(executableURL: scriptURL))

        let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
        XCTAssertEqual(snapshot.condition, .stale)
        XCTAssertTrue(snapshot.unlockPreview.contains("Unlock allowed: yes"))
        XCTAssertTrue(snapshot.canClearStaleLock)
    }

    func testQueueValidationOutput_reportsConfiguredQueuePathWhenCustomQueueIsMissing() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "workspace-diagnostics-custom-queue-missing",
            workspaceName: "workspace",
            createConfigFile: true
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let customQueueURL = fixture.workspaceURL.appendingPathComponent("custom/queue.jsonc", isDirectory: false)
        let customDoneURL = fixture.workspaceURL.appendingPathComponent("custom/done.jsonc", isDirectory: false)
        let overrides = RalphMockCLITestSupport.MockResolvedPathOverrides(
            queueURL: customQueueURL,
            doneURL: customDoneURL,
            projectConfigURL: fixture.configURL
        )
        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                pathOverrides: overrides
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
            name: "mock-ralph-diagnostics-custom-missing",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        let output = await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        XCTAssertTrue(output.contains("Queue validation skipped"))
        XCTAssertTrue(output.contains(customQueueURL.path))
        XCTAssertTrue(output.contains("ralph machine config resolve"))
        XCTAssertFalse(output.contains(".ralph/config.jsonc"))
        XCTAssertFalse(output.contains("ralph init --non-interactive"))
    }

    func testQueueValidationOutput_reportsConfigResolutionFailure() async throws {
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "workspace-diagnostics-config-resolve-failure",
            workspaceName: "workspace"
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "config" ] && [ "$4" = "resolve" ]; then
              echo "load project config: unsupported config version 999" >&2
              exit 11
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: "mock-ralph-diagnostics-config-fail",
            body: script
        )
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: scriptURL),
            bootstrapRepositoryStateOnInit: false
        )

        let output = await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        XCTAssertTrue(output.contains("could not resolve the workspace queue paths"))
        XCTAssertTrue(output.contains("Workspace config is incompatible with this Ralph version"))
    }
}

/*
 Purpose:
 - Verify contract diagnostics persistence surfaces explicit status on write failures.

 Responsibilities:
 - Validate settings and workspace diagnostics persistence returns failure status and telemetry.
 - Verify persistence status returns to success after a transient storage failure.
 - Confirm persisted diagnostics JSON includes the structured persistence outcome.

 Scope:
 - ContractDiagnosticsPersistence helper behavior used by settings and workspace diagnostics coordinators.

 Usage:
 - Runs as part of the RalphCore XCTest suite.

 Invariants/Assumptions:
 - Tests run on the main actor because presentation coordinators are main-actor isolated.
 - Storage failures are injected through ContractDiagnosticsPersistenceStorage closures.
 */
@MainActor
final class ContractDiagnosticsPersistenceTests: RalphCoreTestCase {
    private struct PersistableSnapshot: Codable, Equatable {
        var requestSequence: Int
        var persistence: ContractDiagnosticsPersistenceStatus
    }

    private enum ExpectedFailure: Error {
        case createDirectoryFailed
        case writeFailed
    }

    private final class FailFirstWriteRecorder: @unchecked Sendable {
        private let lock = NSLock()
        private var shouldFail = true
        private var persistedData: Data?

        func write(_ data: Data, to url: URL) throws {
            lock.lock()
            defer { lock.unlock() }

            if shouldFail {
                shouldFail = false
                throw ExpectedFailure.writeFailed
            }

            try data.write(to: url, options: .atomic)
            persistedData = data
        }

        func latestData() -> Data? {
            lock.lock()
            defer { lock.unlock() }
            return persistedData
        }
    }

    func test_persist_createDirectoryFailure_returnsFailureStatus_andTelemetry() throws {
        let directory = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "settings-diagnostics-create-failure")
        defer { RalphCoreTestSupport.assertRemoved(directory) }
        let diagnosticsURL = directory.appendingPathComponent("settings-diagnostics.json", isDirectory: false)
        var telemetry: [ContractDiagnosticsPersistenceFailureTelemetry] = []

        let status = ContractDiagnosticsPersistence.persist(
            snapshot: PersistableSnapshot(requestSequence: 1, persistence: .disabled),
            diagnosticsFileURL: diagnosticsURL,
            storage: ContractDiagnosticsPersistenceStorage(
                createDirectory: { _ in throw ExpectedFailure.createDirectoryFailed },
                writeData: { _, _ in }
            ),
            diagnosticsType: "settings",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            },
            failureTelemetry: { entry in
                telemetry.append(entry)
            }
        )

        XCTAssertEqual(status.outcome, .failure)
        XCTAssertEqual(status.path, diagnosticsURL.path)
        XCTAssertTrue(status.errorMessage?.contains("createDirectoryFailed") == true)
        XCTAssertEqual(telemetry.count, 1)
        XCTAssertEqual(telemetry[0].diagnosticsType, "settings")
        XCTAssertEqual(telemetry[0].path, diagnosticsURL.path)
        XCTAssertTrue(telemetry[0].errorMessage.contains("createDirectoryFailed"))
        XCTAssertTrue(telemetry[0].message.contains("settings"))
    }

    func test_persist_writeFailure_returnsFailureStatus_andTelemetry_forWorkspaceRouting() throws {
        let directory = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "workspace-diagnostics-write-failure")
        defer { RalphCoreTestSupport.assertRemoved(directory) }
        let diagnosticsURL = directory.appendingPathComponent("workspace-diagnostics.json", isDirectory: false)
        var telemetry: [ContractDiagnosticsPersistenceFailureTelemetry] = []

        let status = ContractDiagnosticsPersistence.persist(
            snapshot: PersistableSnapshot(requestSequence: 1, persistence: .disabled),
            diagnosticsFileURL: diagnosticsURL,
            storage: ContractDiagnosticsPersistenceStorage(
                createDirectory: { _ in },
                writeData: { _, _ in throw ExpectedFailure.writeFailed }
            ),
            diagnosticsType: "workspace-routing",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            },
            failureTelemetry: { entry in
                telemetry.append(entry)
            }
        )

        XCTAssertEqual(status.outcome, .failure)
        XCTAssertEqual(status.path, diagnosticsURL.path)
        XCTAssertTrue(status.errorMessage?.contains("writeFailed") == true)
        XCTAssertEqual(telemetry.count, 1)
        XCTAssertEqual(telemetry[0].diagnosticsType, "workspace-routing")
        XCTAssertEqual(telemetry[0].path, diagnosticsURL.path)
        XCTAssertTrue(telemetry[0].errorMessage.contains("writeFailed"))
        XCTAssertTrue(telemetry[0].message.contains("workspace-routing"))
    }

    func test_persist_withoutDiagnosticsPath_returnsDisabled_withoutTelemetry() {
        var telemetry: [ContractDiagnosticsPersistenceFailureTelemetry] = []
        let status = ContractDiagnosticsPersistence.persist(
            snapshot: PersistableSnapshot(requestSequence: 1, persistence: .success(path: "/tmp/ignore")),
            diagnosticsFileURL: nil,
            storage: ContractDiagnosticsPersistenceStorage(
                createDirectory: { _ in XCTFail("createDirectory should not be called when diagnostics are disabled") },
                writeData: { _, _ in XCTFail("writeData should not be called when diagnostics are disabled") }
            ),
            diagnosticsType: "settings",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            },
            failureTelemetry: { entry in
                telemetry.append(entry)
            }
        )

        XCTAssertEqual(status, .disabled)
        XCTAssertTrue(telemetry.isEmpty)
    }

    func test_persist_recoversToSuccess_andWritesSuccessOutcome() throws {
        let directory = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "settings-diagnostics-recover-success")
        defer { RalphCoreTestSupport.assertRemoved(directory) }
        let diagnosticsURL = directory.appendingPathComponent("settings-diagnostics.json", isDirectory: false)
        let recorder = FailFirstWriteRecorder()
        var snapshot = PersistableSnapshot(requestSequence: 1, persistence: .disabled)

        let firstStatus = ContractDiagnosticsPersistence.persist(
            snapshot: snapshot,
            diagnosticsFileURL: diagnosticsURL,
            storage: ContractDiagnosticsPersistenceStorage(
                createDirectory: { url in
                    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
                },
                writeData: { data, url in
                    try recorder.write(data, to: url)
                }
            ),
            diagnosticsType: "settings",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            }
        )
        snapshot.persistence = firstStatus

        let secondStatus = ContractDiagnosticsPersistence.persist(
            snapshot: snapshot,
            diagnosticsFileURL: diagnosticsURL,
            storage: ContractDiagnosticsPersistenceStorage(
                createDirectory: { url in
                    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
                },
                writeData: { data, url in
                    try recorder.write(data, to: url)
                }
            ),
            diagnosticsType: "settings",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            }
        )

        XCTAssertEqual(firstStatus.outcome, .failure)
        XCTAssertEqual(secondStatus.outcome, .success)
        XCTAssertEqual(secondStatus.path, diagnosticsURL.path)
        XCTAssertNil(secondStatus.errorMessage)

        let persistedData = try XCTUnwrap(recorder.latestData())
        let decodedSnapshot = try JSONDecoder().decode(PersistableSnapshot.self, from: persistedData)
        XCTAssertEqual(decodedSnapshot.persistence.outcome, .success)
        XCTAssertEqual(decodedSnapshot.persistence.path, diagnosticsURL.path)
    }
}
