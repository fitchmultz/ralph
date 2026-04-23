/**
 WorkspaceErrorRecoveryTests

 Responsibilities:
 - Verify workspace queue recovery commands surface machine-command failures clearly.
 - Exercise the shared recovery wrapper against structured and legacy stderr fixtures.
 - Guard the machine stderr formatting contract for validate/repair flows.

 Does not handle:
 - Recovery category classification heuristics.
 - Successful queue repair or undo application flows.
 - SwiftUI error presentation.

 Invariants/assumptions callers must respect:
 - Tests run against an isolated mock CLI script rather than the real Ralph binary.
 - Failed `ralph machine queue ...` commands emit stderr only and no success JSON payload.
 - Structured `machine_error` stderr should be reformatted before it reaches localized descriptions.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceErrorRecoveryTests: RalphCoreTestCase {
    func test_validateQueueContinuation_prefersStructuredMachineErrorFromStderr() async throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .queueCorrupted,
            message: "Queue validation could not continue.",
            detail: "read queue file .ralph/queue.jsonc: missing terminal completed_at",
            retryable: false
        )
        let fixture = try Self.makeMockCLIFixture(validateFailure: .machine(document))
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        do {
            _ = try await workspace.validateQueueContinuation()
            XCTFail("Expected structured machine failure")
        } catch {
            XCTAssertEqual(error.localizedDescription, document.userFacingDescription)
        }
    }

    func test_repairQueueContinuation_fallsBackToTrimmedRawStderr() async throws {
        let fixture = try Self.makeMockCLIFixture(repairFailure: .stderr("  queue repair preview failed  \n"))
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        do {
            _ = try await workspace.repairQueueContinuation(dryRun: true)
            XCTFail("Expected raw stderr failure")
        } catch {
            XCTAssertEqual(error.localizedDescription, "queue repair preview failed")
        }
    }
}

private extension WorkspaceErrorRecoveryTests {
    enum MockFailure {
        case machine(MachineErrorDocument)
        case stderr(String)
    }

    struct MockCLIFixture {
        let rootURL: URL
        let workspaceURL: URL
        let scriptURL: URL
    }

    static func makeMockCLIFixture(
        validateFailure: MockFailure? = nil,
        repairFailure: MockFailure? = nil
    ) throws -> MockCLIFixture {
        let queueTasks = [
            RalphMockCLITestSupport.task(
                id: "RQ-0007",
                status: .todo,
                title: "Auth epic",
                priority: .high
            ),
        ]
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-error-recovery",
            workspaceName: "workspace",
            seedQueueTasks: queueTasks
        )

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "gpt-5.3-codex", phases: 2, iterations: 3)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: queueTasks,
                nextRunnableTaskID: "RQ-0007"
            ),
            in: fixture.rootURL,
            name: "queue-read.json"
        )
        let validateFailureURL = try writeFailureFixture(
            validateFailure,
            in: fixture.rootURL,
            name: "validate-error.txt"
        )
        let repairFailureURL = try writeFailureFixture(
            repairFailure,
            in: fixture.rootURL,
            name: "repair-error.txt"
        )

        let script = """
        #!/bin/sh
        set -eu
        if [ "$1" = "--version" ] || [ "$1" = "version" ]; then
          echo "ralph \(VersionCompatibility.minimumCLIVersion)"
          exit 0
        fi
        if [ "$1" = "--no-color" ]; then
          shift
        fi
        if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then
          cat "\(configResolveURL.path)"
          exit 0
        fi
        if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then
          cat "\(queueReadURL.path)"
          exit 0
        fi
        if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "validate" ]; then
          if [ -n "__VALIDATE_FAILURE_PATH__" ]; then
            cat "__VALIDATE_FAILURE_PATH__" >&2
            exit 11
          fi
          echo '{"version":1}'
          exit 0
        fi
        if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "repair" ]; then
          if [ -n "__REPAIR_FAILURE_PATH__" ]; then
            cat "__REPAIR_FAILURE_PATH__" >&2
            exit 12
          fi
          echo '{"version":1}'
          exit 0
        fi
        echo "unsupported command: $*" >&2
        exit 1
        """

        let resolvedScript = script
            .replacingOccurrences(of: "__VALIDATE_FAILURE_PATH__", with: validateFailureURL?.path ?? "")
            .replacingOccurrences(of: "__REPAIR_FAILURE_PATH__", with: repairFailureURL?.path ?? "")

        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(
            in: fixture.rootURL,
            name: fixture.scriptURL.lastPathComponent,
            body: resolvedScript
        )

        return MockCLIFixture(
            rootURL: fixture.rootURL,
            workspaceURL: fixture.workspaceURL,
            scriptURL: scriptURL
        )
    }

    static func writeFailureFixture(
        _ failure: MockFailure?,
        in rootURL: URL,
        name: String
    ) throws -> URL? {
        guard let failure else { return nil }
        let fileURL = rootURL.appendingPathComponent(name, isDirectory: false)
        let contents: String
        switch failure {
        case .machine(let document):
            contents = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        case .stderr(let value):
            contents = value
        }
        try contents.write(to: fileURL, atomically: true, encoding: .utf8)
        return fileURL
    }
}
