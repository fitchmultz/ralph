/**
 WorkspaceTaskMutationAgentTests

 Responsibilities:
 - Validate task-mutation payload generation for agent override edits.
 - Cover add, clear, and semantic-noop override scenarios.

 Does not handle:
 - Run-control streaming or runner-configuration refresh.

 Invariants/assumptions callers must respect:
 - Mock CLIs log argv and payload bodies so assertions inspect serialized mutation requests.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceTaskMutationAgentTests: WorkspacePerformanceTestCase {
    func test_updateTask_agentOverride_emitsAgentEditCommand() async throws {
        let fixture = try Self.makeMutationFixture(
            prefix: "ralph-workspace-agent-edit",
            scriptName: "mock-ralph-task-mutate-agent",
            mutationReportJSON: #"{"version":1,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9001","applied_edits":1}]}}"#
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        let original = RalphTask(
            id: "RQ-9001",
            status: .todo,
            title: "Task",
            priority: .medium
        )
        var updated = original
        updated.agent = RalphTaskAgent(
            runner: "codex",
            model: "gpt-5.3-codex",
            modelEffort: "high",
            phases: 2,
            iterations: 1,
            phaseOverrides: RalphTaskPhaseOverrides(
                phase2: RalphTaskPhaseOverride(
                    runner: "kimi",
                    model: "kimi-code/kimi-for-coding",
                    reasoningEffort: nil
                )
            )
        )

        try await workspace.updateTask(from: original, to: updated)

        let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
        let lines = log.split(separator: "\n").map(String.init)
        let payloadLine = lines.first { $0.contains("\"task_id\" : \"RQ-9001\"") }

        XCTAssertNotNil(payloadLine)
        XCTAssertTrue(log.contains("\"field\" : \"agent\""))
        XCTAssertTrue(log.contains("\\\"runner\\\":\\\"codex\\\""))
        XCTAssertTrue(log.contains("\\\"model\\\":\\\"gpt-5.3-codex\\\""))
        XCTAssertTrue(log.contains("\\\"model_effort\\\":\\\"high\\\""))
        XCTAssertTrue(log.contains("\\\"phases\\\":2"))
        XCTAssertTrue(log.contains("\\\"iterations\\\":1"))
        XCTAssertTrue(log.contains("\\\"phase_overrides\\\":{\\\"phase2\\\""))
        XCTAssertTrue(lines.contains { $0.contains("<--no-color><machine><queue><read>") })
    }

    func test_updateTask_clearingAgentOverride_emitsEmptyAgentValue() async throws {
        let fixture = try Self.makeMutationFixture(
            prefix: "ralph-workspace-agent-clear",
            scriptName: "mock-ralph-task-mutate-agent-clear",
            mutationReportJSON: #"{"version":1,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9002","applied_edits":1}]}}"#
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        let original = RalphTask(
            id: "RQ-9002",
            status: .todo,
            title: "Task",
            priority: .medium,
            agent: RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.3-codex",
                phases: 2
            )
        )
        var updated = original
        updated.agent = nil

        try await workspace.updateTask(from: original, to: updated)

        let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
        XCTAssertTrue(log.contains("\"task_id\" : \"RQ-9002\""))
        XCTAssertTrue(log.contains("\"field\" : \"agent\""))
        XCTAssertTrue(log.contains("\"value\" : \"\""))
    }

    func test_updateTask_semanticallyEmptyAgentOverride_doesNotEmitAgentEdit() async throws {
        let fixture = try Self.makeMutationFixture(
            prefix: "ralph-workspace-agent-noop",
            scriptName: "mock-ralph-task-mutate-agent-noop",
            mutationReportJSON: #"{"version":1,"report":{"version":1,"atomic":true,"tasks":[]}}"#
        )
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        let original = RalphTask(
            id: "RQ-9003",
            status: .todo,
            title: "Task",
            priority: .medium
        )
        var updated = original
        updated.agent = RalphTaskAgent(
            runner: "   ",
            model: "  ",
            modelEffort: "default",
            phases: 8,
            iterations: 0
        )

        try await workspace.updateTask(from: original, to: updated)

        if FileManager.default.fileExists(atPath: fixture.logURL.path) {
            let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
            let lines = log.split(separator: "\n").map(String.init)
            XCTAssertFalse(lines.contains { $0.contains("<--no-color><machine><task><mutate><--input><") })
            XCTAssertFalse(log.contains("\"field\" : \"agent\""))
        }
    }

    private struct MutationFixture {
        let rootURL: URL
        let workspaceURL: URL
        let scriptURL: URL
        let logURL: URL
    }

    private static func makeMutationFixture(
        prefix: String,
        scriptName: String,
        mutationReportJSON: String
    ) throws -> MutationFixture {
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: prefix,
            scriptName: scriptName,
            logFileName: "commands.log",
            seedQueueTasks: []
        )
        let logURL = try XCTUnwrap(fixture.logURL)

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "codex", phases: 2, iterations: 1)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: []
            ),
            in: fixture.rootURL,
            name: "queue-read.json"
        )
        let mutationReportURL = fixture.rootURL.appendingPathComponent("mutation-report.json", isDirectory: false)
        try mutationReportJSON.write(to: mutationReportURL, atomically: true, encoding: .utf8)

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            case "$*" in
            *"--no-color machine config resolve"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "\(configResolveURL.path)"
              exit 0
              ;;
            *"--no-color machine queue read"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "\(queueReadURL.path)"
              exit 0
              ;;
            *"--no-color machine task mutate --input "*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$6" >> "$log_file"
              printf '\n' >> "$log_file"
              cat "\(mutationReportURL.path)"
              exit 0
              ;;
            esac

            echo "unexpected args: $*" >&2
            exit 64
            """

        _ = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, name: scriptName, body: script)
        return MutationFixture(
            rootURL: fixture.rootURL,
            workspaceURL: fixture.workspaceURL,
            scriptURL: fixture.scriptURL,
            logURL: logURL
        )
    }
}
