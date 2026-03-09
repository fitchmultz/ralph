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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-agent-edit-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            if [ "$1" = "--no-color" ] && [ "$2" = "__cli-spec" ] && [ "$3" = "--format" ] && [ "$4" = "json" ]; then
              echo '{"version":2,"root":{"name":"ralph","about":"mock","subcommands":[]}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "config" ] && [ "$3" = "show" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              echo '{"agent":{"model":"gpt-5.3-codex","iterations":1}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "queue" ] && [ "$3" = "list" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"tasks":[]}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "task" ] && [ "$3" = "mutate" ] && [ "$4" = "--input" ] && [ -n "$5" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$5" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9001","applied_edits":1}]}'
              exit 0
            fi

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-task-mutate-agent",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)

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

        let log = try String(contentsOf: logURL, encoding: .utf8)
        let lines = log.split(separator: "\n").map(String.init)
        let mutateInvocationLine = lines.first { $0.contains("<--no-color><task><mutate><--input><") }
        let payloadLine = lines.first { $0.contains("\"task_id\" : \"RQ-9001\"") }

        XCTAssertNotNil(mutateInvocationLine)
        XCTAssertNotNil(payloadLine)
        XCTAssertTrue(log.contains("\"field\" : \"agent\""))
        XCTAssertTrue(log.contains("\\\"runner\\\":\\\"codex\\\""))
        XCTAssertTrue(log.contains("\\\"model\\\":\\\"gpt-5.3-codex\\\""))
        XCTAssertTrue(log.contains("\\\"model_effort\\\":\\\"high\\\""))
        XCTAssertTrue(log.contains("\\\"phases\\\":2"))
        XCTAssertTrue(log.contains("\\\"iterations\\\":1"))
        XCTAssertTrue(log.contains("\\\"phase_overrides\\\":{\\\"phase2\\\""))
        XCTAssertTrue(lines.contains { $0.contains("<--no-color><queue><list><--format><json>") })
    }

    func test_updateTask_clearingAgentOverride_emitsEmptyAgentValue() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-agent-clear-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            if [ "$1" = "--no-color" ] && [ "$2" = "__cli-spec" ] && [ "$3" = "--format" ] && [ "$4" = "json" ]; then
              echo '{"version":2,"root":{"name":"ralph","about":"mock","subcommands":[]}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "config" ] && [ "$3" = "show" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              echo '{"agent":{"model":"gpt-5.3-codex","iterations":1}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "queue" ] && [ "$3" = "list" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"tasks":[]}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "task" ] && [ "$3" = "mutate" ] && [ "$4" = "--input" ] && [ -n "$5" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$5" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9002","applied_edits":1}]}'
              exit 0
            fi

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-task-mutate-agent-clear",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)

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

        let log = try String(contentsOf: logURL, encoding: .utf8)
        let lines = log.split(separator: "\n").map(String.init)
        let mutateInvocationLine = lines.first { $0.contains("<--no-color><task><mutate><--input><") }

        XCTAssertNotNil(mutateInvocationLine)
        XCTAssertTrue(log.contains("\"task_id\" : \"RQ-9002\""))
        XCTAssertTrue(log.contains("\"field\" : \"agent\""))
        XCTAssertTrue(log.contains("\"value\" : \"\""))
    }

    func test_updateTask_semanticallyEmptyAgentOverride_doesNotEmitAgentEdit() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-agent-noop-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            if [ "$1" = "--no-color" ] && [ "$2" = "__cli-spec" ] && [ "$3" = "--format" ] && [ "$4" = "json" ]; then
              echo '{"version":2,"root":{"name":"ralph","about":"mock","subcommands":[]}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "config" ] && [ "$3" = "show" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              echo '{"agent":{"model":"gpt-5.3-codex","iterations":1}}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "queue" ] && [ "$3" = "list" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"tasks":[]}'
              exit 0
            fi

            if [ "$1" = "--no-color" ] && [ "$2" = "task" ] && [ "$3" = "mutate" ] && [ "$4" = "--input" ] && [ -n "$5" ]; then
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$5" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"atomic":true,"tasks":[]}'
              exit 0
            fi

            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try WorkspacePerformanceTestSupport.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-task-mutate-agent-noop",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)

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

        if FileManager.default.fileExists(atPath: logURL.path) {
            let log = try String(contentsOf: logURL, encoding: .utf8)
            let lines = log.split(separator: "\n").map(String.init)
            XCTAssertFalse(lines.contains { $0.contains("<--no-color><task><mutate><--input><") })
            XCTAssertFalse(log.contains("\"field\" : \"agent\""))
        }
    }
}
