/**
 WorkspacePerformanceTests

 Responsibilities:
 - Validate performance characteristics of Workspace methods with large datasets.
 - Ensure detectTaskChanges and isTaskBlocked maintain O(N) time complexity.
 - Verify key async loaders can be called from detached tasks without actor-isolation crashes.
 - Cover regression-sensitive run-control and CLI-adoption behaviors.

 Does not handle:
 - Functional correctness (covered by other tests).
 - Memory pressure testing.

 Invariants/assumptions callers must respect:
 - Tests run on the main actor.
 - Tests use synthetic data; actual task file structure not required.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class WorkspacePerformanceTests: XCTestCase {
    
    var workspace: Workspace!
    
    override func setUp() async throws {
        try await super.setUp()
        workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
    }
    
    override func tearDown() async throws {
        workspace = nil
        try await super.tearDown()
    }
    
    // MARK: - Performance Tests
    
    func test_detectTaskChanges_performance_1000Tasks() {
        let previous = generateTasks(count: 1000)
        let current = generateTasks(count: 1000, mutateFrom: previous)
        
        measure {
            _ = workspace.detectTaskChanges(previous: previous, current: current)
        }
    }
    
    func test_isTaskBlocked_performance_500Tasks() {
        // Set up workspace with 500 tasks
        workspace.tasks = generateTasksWithDependencies(count: 500)
        
        let testTask = RalphTask(
            id: "RQ-TEST",
            status: .todo,
            title: "Test Task",
            priority: .high,
            dependsOn: (1...10).map { "RQ-\($0)" }  // Depends on 10 tasks
        )
        
        measure {
            for _ in 0..<100 {
                _ = workspace.isTaskBlocked(testTask)
            }
        }
    }
    
    // MARK: - Output Buffer Tests

    func test_outputBuffer_enforcesMaxCharacters() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 100)

        // Add 150 characters
        buffer.append(String(repeating: "a", count: 150))

        // Should be truncated to ~100 (accounting for indicator)
        XCTAssertLessThanOrEqual(buffer.content.count, 110)
        XCTAssertTrue(buffer.isTruncated)
    }

    func test_outputBuffer_preservesTrailingContent() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 50)

        buffer.append("START_")
        buffer.append(String(repeating: "x", count: 100))
        buffer.append("_END")

        // Should contain the END marker, not the START marker
        XCTAssertTrue(buffer.content.contains("END"))
        XCTAssertFalse(buffer.content.contains("START"))
    }

    func test_outputBuffer_tracksOriginalLength() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 10)

        buffer.append("12345")
        XCTAssertEqual(buffer.originalLength, 5)
        XCTAssertFalse(buffer.isTruncated)

        buffer.append("6789012345")
        XCTAssertEqual(buffer.originalLength, 15)
        XCTAssertTrue(buffer.isTruncated)
    }

    func test_outputBuffer_setContent_enforcesLimit() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 20)

        buffer.setContent(String(repeating: "x", count: 100))

        XCTAssertTrue(buffer.isTruncated)
        XCTAssertEqual(buffer.originalLength, 100)
        XCTAssertLessThanOrEqual(buffer.content.count, 30) // 20 + indicator
    }

    func test_outputBuffer_clear_resetsState() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 10)

        buffer.append("12345678901") // 11 chars
        XCTAssertTrue(buffer.isTruncated)

        buffer.clear()
        XCTAssertEqual(buffer.content, "")
        XCTAssertEqual(buffer.originalLength, 0)
        XCTAssertFalse(buffer.isTruncated)
    }

    func test_outputBuffer_dynamicLimitChange() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 100)

        buffer.append(String(repeating: "x", count: 80))
        XCTAssertFalse(buffer.isTruncated)

        // Lower the limit
        buffer.maxCharacters = 50
        XCTAssertTrue(buffer.isTruncated)
        XCTAssertLessThanOrEqual(buffer.content.count, 70) // 50 + indicator
    }

    func test_outputBuffer_clampsToHardLimit() {
        let tooLarge = ConsoleOutputBuffer.hardMaxCharacters + 500_000
        let buffer = ConsoleOutputBuffer(maxCharacters: tooLarge)
        XCTAssertEqual(buffer.maxCharacters, ConsoleOutputBuffer.hardMaxCharacters)

        buffer.maxCharacters = tooLarge
        XCTAssertEqual(buffer.maxCharacters, ConsoleOutputBuffer.hardMaxCharacters)
    }

    // MARK: - ANSI Segment Limit Tests

    func test_ansiSegmentLimit_enforced() {
        // Generate output that creates many segments
        var ansiOutput = ""
        for i in 0..<200 {
            ansiOutput += "\u{001B}[3\(i % 8)mtext\(i)\u{001B}[0m "
        }

        workspace.maxANSISegments = 50
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        // Should be limited to max + 1 (for indicator)
        XCTAssertLessThanOrEqual(workspace.attributedOutput.count, 51)
    }

    func test_ansiSegmentLimit_indicatorAdded() {
        // Generate output that creates many segments
        var ansiOutput = ""
        for i in 0..<100 {
            ansiOutput += "\u{001B}[3\(i % 8)mtext\(i)\u{001B}[0m "
        }

        workspace.maxANSISegments = 20
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        // Should have indicator segment at the beginning
        XCTAssertEqual(workspace.attributedOutput.first?.text, "\n... [console output truncated due to length] ...\n")
        XCTAssertEqual(workspace.attributedOutput.first?.color, .yellow)
        XCTAssertTrue(workspace.attributedOutput.first?.isItalic ?? false)
    }

    func test_ansiSegmentLimit_dynamicChange() {
        // Generate output with many segments
        var ansiOutput = ""
        for i in 0..<100 {
            ansiOutput += "\u{001B}[3\(i % 8)mtext\(i)\u{001B}[0m "
        }

        workspace.maxANSISegments = 200
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        let initialCount = workspace.attributedOutput.count
        XCTAssertGreaterThan(initialCount, 0)

        // Lower the limit
        workspace.maxANSISegments = 10
        XCTAssertLessThanOrEqual(workspace.attributedOutput.count, 11) // 10 + indicator
    }

    func test_ansiSegmentLimit_noTruncationWhenUnderLimit() {
        // Small output shouldn't be truncated
        workspace.parseANSICodes(from: "Simple text without ANSI codes")
        workspace.enforceANSISegmentLimit()

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput.first?.text, "Simple text without ANSI codes")
    }

    func test_parseANSICodes_replaceMode_rebuildsWithoutDuplicateGrowth() {
        workspace.parseANSICodes(from: "line 1\n", appendToExisting: false)
        workspace.parseANSICodes(from: "line 1\nline 2\n", appendToExisting: false)

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput.first?.text, "line 1\nline 2\n")
    }

    // MARK: - MainActor Isolation Regression Tests

    func test_loadTasks_fromDetachedTask_reportsMissingClientError() async {
        let workspace = self.workspace!

        await Task.detached(priority: .userInitiated) {
            await workspace.loadTasks(retryConfiguration: .minimal)
        }.value

        XCTAssertEqual(workspace.tasksErrorMessage, "CLI client not available.")
        XCTAssertFalse(workspace.tasksLoading)
    }

    func test_loadCLISpec_fromDetachedTask_reportsMissingClientError() async {
        let workspace = self.workspace!

        await Task.detached(priority: .userInitiated) {
            await workspace.loadCLISpec(retryConfiguration: .minimal)
        }.value

        XCTAssertEqual(workspace.cliSpecErrorMessage, "CLI client not available.")
        XCTAssertFalse(workspace.cliSpecIsLoading)
    }

    // MARK: - Runner Config Loading Tests

    func test_loadRunnerConfiguration_setsCurrentRunnerConfig() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-config-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let script = """
            #!/bin/sh
            if [ "$1" = "--no-color" ] && [ "$2" = "config" ] && [ "$3" = "show" ] && [ "$4" = "--format" ] && [ "$5" = "json" ]; then
              cat <<'JSON'
            {"agent":{"model":"kimi-code/kimi-for-coding","phases":2,"iterations":3}}
            JSON
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        XCTAssertEqual(workspace.currentRunnerConfig?.model, "kimi-code/kimi-for-coding")
        XCTAssertEqual(workspace.currentRunnerConfig?.phases, 2)
        XCTAssertEqual(workspace.currentRunnerConfig?.maxIterations, 3)
    }

    func test_loadRunnerConfiguration_onFailure_clearsCurrentRunnerConfig() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-config-failure-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let successScript = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"kimi-initial","phases":3,"iterations":2}}'
              exit 0
            fi
            exit 64
            """
        let successScriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-success",
            body: successScript
        )
        let successClient = try RalphCLIClient(executableURL: successScriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: successClient)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        XCTAssertEqual(workspace.currentRunnerConfig?.model, "kimi-initial")
        XCTAssertEqual(workspace.currentRunnerConfig?.phases, 3)
        XCTAssertEqual(workspace.currentRunnerConfig?.maxIterations, 2)

        let failScript = """
            #!/bin/sh
            echo "config failed" 1>&2
            exit 1
            """
        let failScriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-fail",
            body: failScript
        )
        let failClient = try RalphCLIClient(executableURL: failScriptURL)
        workspace.injectClient(failClient)

        await Self.waitFor(timeout: 2.0) {
            workspace.currentRunnerConfig == nil
        }

        XCTAssertNil(workspace.currentRunnerConfig)
    }

    func test_setWorkingDirectory_refreshesRunnerConfiguration() async throws {
        let rootDir = try Self.makeTempDir(prefix: "ralph-workspace-config-switch-")
        defer { try? FileManager.default.removeItem(at: rootDir) }
        let workspaceADir = rootDir.appendingPathComponent("workspace-a", isDirectory: true)
        let workspaceBDir = rootDir.appendingPathComponent("workspace-b", isDirectory: true)
        try FileManager.default.createDirectory(at: workspaceADir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspaceBDir, withIntermediateDirectories: true)

        let switchScript = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              case "$PWD" in
              */workspace-a)
                echo '{"agent":{"model":"model-a","phases":1,"iterations":1}}'
                ;;
              */workspace-b)
                echo '{"agent":{"model":"model-b","phases":2,"iterations":4}}'
                ;;
              *)
                echo '{"agent":{"model":"model-unknown","phases":3,"iterations":9}}'
                ;;
              esac
              exit 0
            fi
            exit 64
            """
        let scriptURL = try Self.makeExecutableScript(
            in: rootDir,
            name: "mock-ralph-switch",
            body: switchScript
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: workspaceADir, client: client)

        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)
        XCTAssertEqual(workspace.currentRunnerConfig?.model, "model-a")
        XCTAssertEqual(workspace.currentRunnerConfig?.phases, 1)
        XCTAssertEqual(workspace.currentRunnerConfig?.maxIterations, 1)

        workspace.setWorkingDirectory(workspaceBDir)

        await Self.waitFor(timeout: 2.0) {
            workspace.currentRunnerConfig?.model == "model-b"
                && workspace.currentRunnerConfig?.phases == 2
                && workspace.currentRunnerConfig?.maxIterations == 4
        }

        XCTAssertEqual(workspace.currentRunnerConfig?.model, "model-b")
        XCTAssertEqual(workspace.currentRunnerConfig?.phases, 2)
        XCTAssertEqual(workspace.currentRunnerConfig?.maxIterations, 4)
    }

    // MARK: - WorkspaceManager CLI Override Rejection Tests

    func test_workspaceManager_adoptCLIExecutable_rejectsValidPathOverride() async throws {
        let manager = WorkspaceManager.shared
        let baselinePath = manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-manager-cli-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        let overrideURL = try Self.makeVersionAwareMockCLI(in: tempDir, name: "mock-ralph-version-ok")
        let overridePath = overrideURL.path

        manager.adoptCLIExecutable(path: overridePath)

        if let baselinePath {
            XCTAssertEqual(
                manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path,
                baselinePath
            )
        } else {
            XCTAssertNil(manager.client)
        }
    }

    func test_workspaceManager_adoptCLIExecutable_preservesClientOnInvalidPath() {
        let manager = WorkspaceManager.shared
        let baselinePath = manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path

        manager.adoptCLIExecutable(path: "/definitely/not/a/real/ralph-binary")

        if let baselinePath {
            XCTAssertEqual(
                manager.client?.executableURL.standardizedFileURL.resolvingSymlinksInPath().path,
                baselinePath
            )
        } else {
            XCTAssertNotNil(manager.errorMessage)
        }
    }

    // MARK: - Run Control Behavior Tests

    func test_runNextTask_resolvesCLISelection_andStreamsOutput() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-run-stream-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":2}}'
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--dry-run" ]; then
              echo "Dry run: would run RQ-4242 (status: Todo)"
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ] && [ "$4" = "--id" ] && [ "$5" = "RQ-4242" ]; then
              echo "PHASE 1 starting"
              sleep 1
              echo "PHASE 2 running"
              sleep 1
              echo "done"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-stream",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        workspace.runNextTask()

        await Self.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-4242" && workspace.output.contains("PHASE 1")
        }

        XCTAssertEqual(workspace.currentTaskID, "RQ-4242")
        XCTAssertTrue(workspace.output.contains("PHASE 1"))
        XCTAssertTrue(workspace.isRunning)

        await Self.waitFor(timeout: 4.0) {
            !workspace.isRunning
        }

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertTrue(workspace.output.contains("PHASE 2"))
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-4242")
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, false)
    }

    func test_runNextTask_withExplicitIDAndForce_usesExpectedArguments() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-run-explicit-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":1}}'
              exit 0
            fi
            if [ "$2" = "run" ] && [ "$3" = "one" ]; then
              case "$*" in
                *"--no-color run one --force --id RQ-5555"*)
                  echo "running explicit"
                  exit 0
                  ;;
              esac
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        let scriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-explicit",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        workspace.runNextTask(taskIDOverride: "RQ-5555", forceDirtyRepo: true)

        await Self.waitFor(timeout: 2.0) {
            workspace.currentTaskID == "RQ-5555" && workspace.isRunning
        }
        await Self.waitFor(timeout: 3.0) {
            !workspace.isRunning
        }

        XCTAssertEqual(workspace.currentTaskID, nil)
        XCTAssertEqual(workspace.lastExitStatus?.code, 0)
        XCTAssertEqual(workspace.executionHistory.first?.taskID, "RQ-5555")
        XCTAssertTrue(workspace.output.contains("running explicit"))
    }

    func test_runControlPreviewTask_prefersSelectedTodoTask() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        workspace.tasks = [
            RalphTask(
                id: "RQ-1001",
                status: .todo,
                title: "First",
                priority: .medium
            ),
            RalphTask(
                id: "RQ-1002",
                status: .todo,
                title: "Second",
                priority: .high
            )
        ]

        workspace.runControlSelectedTaskID = "RQ-1002"
        XCTAssertEqual(workspace.runControlPreviewTask?.id, "RQ-1002")

        workspace.runControlSelectedTaskID = "RQ-9999"
        XCTAssertEqual(workspace.runControlPreviewTask?.id, "RQ-1001")
    }

    func test_cancel_stopsActiveRun_andRecordsCancellation() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-run-cancel-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        let script = """
            #!/bin/sh
            if [ "$2" = "config" ] && [ "$3" = "show" ]; then
              echo '{"agent":{"model":"model-test","iterations":2}}'
              exit 0
            fi
            exec /bin/sleep "$@"
            """
        let scriptURL = try Self.makeExecutableScript(
            in: tempDir,
            name: "mock-ralph-run-cancel",
            body: script
        )
        let client = try RalphCLIClient(executableURL: scriptURL)
        let workspace = Workspace(workingDirectoryURL: tempDir, client: client)
        await workspace.loadRunnerConfiguration(retryConfiguration: .minimal)

        workspace.run(arguments: ["60"])

        await Self.waitFor(timeout: 1.0) {
            workspace.isRunning
        }
        XCTAssertTrue(workspace.isRunning)
        try await Task.sleep(nanoseconds: 150_000_000)

        workspace.cancel()

        await Self.waitFor(timeout: 6.0) {
            !workspace.isRunning
        }

        XCTAssertFalse(workspace.isRunning)
        XCTAssertEqual(workspace.executionHistory.first?.wasCancelled, true)
        XCTAssertNil(workspace.executionHistory.first?.exitCode)
        XCTAssertEqual(workspace.isLoopMode, false)
        XCTAssertEqual(workspace.stopAfterCurrent, true)
    }

    // MARK: - Task Mutation Agent Override Tests

    func test_updateTask_agentOverride_emitsAgentEditCommand() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-agent-edit-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try Self.writeEmptyQueueFile(in: tempDir)
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
        let scriptURL = try Self.makeExecutableScript(
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
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-agent-clear-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try Self.writeEmptyQueueFile(in: tempDir)
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
        let scriptURL = try Self.makeExecutableScript(
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
        let tempDir = try Self.makeTempDir(prefix: "ralph-workspace-agent-noop-")
        defer { try? FileManager.default.removeItem(at: tempDir) }
        try Self.writeEmptyQueueFile(in: tempDir)
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
        let scriptURL = try Self.makeExecutableScript(
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

    // MARK: - Helpers

    private static func makeTempDir(prefix: String) throws -> URL {
        let base = FileManager.default.temporaryDirectory
        let dir = base.appendingPathComponent("\(prefix)\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private static func makeExecutableScript(in directory: URL, name: String, body: String) throws -> URL {
        let scriptURL = directory.appendingPathComponent(name, isDirectory: false)
        try body.write(to: scriptURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o755))],
            ofItemAtPath: scriptURL.path
        )
        return scriptURL
    }

    private static func makeVersionAwareMockCLI(in directory: URL, name: String) throws -> URL {
        let script = """
            #!/bin/sh
            if [ "$1" = "--version" ] || [ "$1" = "version" ]; then
              echo "ralph \(VersionCompatibility.minimumCLIVersion)"
              exit 0
            fi
            echo "unexpected args: $*" 1>&2
            exit 64
            """
        return try makeExecutableScript(in: directory, name: name, body: script)
    }

    private static func writeEmptyQueueFile(in workspaceDir: URL) throws {
        let ralphDir = workspaceDir.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphDir, withIntermediateDirectories: true)
        let queueFile = ralphDir.appendingPathComponent("queue.jsonc", isDirectory: false)
        try #"{"version":1,"tasks":[]}"#.write(to: queueFile, atomically: true, encoding: .utf8)
    }

    private static func waitFor(
        timeout: TimeInterval,
        pollIntervalNanoseconds: UInt64 = 50_000_000,
        condition: @escaping @MainActor () -> Bool
    ) async {
        let start = Date()
        while !(await MainActor.run { condition() }) {
            if Date().timeIntervalSince(start) >= timeout {
                break
            }
            try? await Task.sleep(nanoseconds: pollIntervalNanoseconds)
        }
    }

    private func generateTasks(count: Int) -> [RalphTask] {
        return (1...count).map { index in
            RalphTask(
                id: String(format: "RQ-%04d", index),
                status: index % 5 == 0 ? .done : .todo,
                title: "Task \(index)",
                description: "Description for task \(index)",
                priority: [.low, .medium, .high, .critical][index % 4],
                tags: ["tag\(index % 5)", "tag\(index % 3)"],
                createdAt: Date().addingTimeInterval(-Double(index * 3600)),
                updatedAt: Date()
            )
        }
    }
    
    private func generateTasks(count: Int, mutateFrom base: [RalphTask]) -> [RalphTask] {
        return base.map { task in
            // Modify ~10% of tasks
            if Int.random(in: 1...10) == 1 {
                return RalphTask(
                    id: task.id,
                    status: task.status == .todo ? .doing : .todo,
                    title: task.title + " (modified)",
                    description: task.description,
                    priority: task.priority,
                    tags: task.tags,
                    scope: task.scope,
                    evidence: task.evidence,
                    plan: task.plan,
                    notes: task.notes,
                    request: task.request,
                    createdAt: task.createdAt,
                    updatedAt: Date(),
                    startedAt: task.startedAt,
                    completedAt: task.completedAt,
                    dependsOn: task.dependsOn,
                    blocks: task.blocks,
                    relatesTo: task.relatesTo,
                    customFields: task.customFields
                )
            }
            return task
        }
    }
    
    private func generateTasksWithDependencies(count: Int) -> [RalphTask] {
        return (1...count).map { index in
            let dependsOn: [String]?
            if index > 10 {
                // Each task depends on up to 3 previous tasks
                dependsOn = (1...min(3, index - 1)).map { "RQ-\(index - $0)" }
            } else {
                dependsOn = nil
            }
            
            return RalphTask(
                id: String(format: "RQ-%04d", index),
                status: index % 3 == 0 ? .done : .todo,
                title: "Task \(index)",
                priority: .medium,
                dependsOn: dependsOn
            )
        }
    }
}
