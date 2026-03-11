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
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            case "$*" in
            *"--no-color machine config resolve"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"codex","phases":2,"iterations":1}}}'
              exit 0
              ;;
            *"--no-color machine queue read"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":null,"runnability":{}}'
              exit 0
              ;;
            *"--no-color machine task mutate --input "*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$6" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9001","applied_edits":1}]}}'
              exit 0
              ;;
            esac

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
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-agent-clear-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            case "$*" in
            *"--no-color machine config resolve"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"codex","phases":2,"iterations":1}}}'
              exit 0
              ;;
            *"--no-color machine queue read"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":null,"runnability":{}}'
              exit 0
              ;;
            *"--no-color machine task mutate --input "*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$6" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9002","applied_edits":1}]}}'
              exit 0
              ;;
            esac

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
        XCTAssertTrue(log.contains("\"task_id\" : \"RQ-9002\""))
        XCTAssertTrue(log.contains("\"field\" : \"agent\""))
        XCTAssertTrue(log.contains("\"value\" : \"\""))
    }

    func test_updateTask_semanticallyEmptyAgentOverride_doesNotEmitAgentEdit() async throws {
        let tempDir = try WorkspacePerformanceTestSupport.makeTempDir(prefix: "ralph-workspace-agent-noop-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }
        try WorkspacePerformanceTestSupport.writeEmptyQueueFile(in: tempDir)
        let logURL = tempDir.appendingPathComponent("commands.log")

        let script = """
            #!/bin/sh
            log_file="\(logURL.path)"

            case "$*" in
            *"--no-color machine config resolve"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"config":{"agent":{"model":"codex","phases":2,"iterations":1}}}'
              exit 0
              ;;
            *"--no-color machine queue read"*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              echo '{"version":1,"paths":{"repo_root":"'"$PWD"'","queue_path":"'"$PWD"'/.ralph/queue.jsonc","done_path":"'"$PWD"'/.ralph/done.jsonc","project_config_path":"'"$PWD"'/.ralph/config.jsonc","global_config_path":null},"active":{"version":1,"tasks":[]},"done":{"version":1,"tasks":[]},"next_runnable_task_id":null,"runnability":{}}'
              exit 0
              ;;
            *"--no-color machine task mutate --input "*)
              for arg in "$@"; do
                printf '<%s>' "$arg" >> "$log_file"
              done
              printf '\n' >> "$log_file"
              cat "$6" >> "$log_file"
              printf '\n' >> "$log_file"
              echo '{"version":1,"report":{"version":1,"atomic":true,"tasks":[]}}'
              exit 0
              ;;
            esac

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
            XCTAssertFalse(lines.contains { $0.contains("<--no-color><machine><task><mutate><--input><") })
            XCTAssertFalse(log.contains("\"field\" : \"agent\""))
        }
    }
}
