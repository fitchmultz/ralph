/**
 WorkspaceCorePerformanceTests

 Purpose:
 - Validate workspace task-diffing and task-blocking performance characteristics.

 Responsibilities:
 - Validate workspace task-diffing and task-blocking performance characteristics.
 - Cover console buffer and ANSI truncation regressions.
 - Verify detached-task entrypoints remain main-actor safe.

 Does not handle:
 - Runner configuration, run-control flows, or task-mutation CLI payload assertions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests inherit a fresh main-actor workspace from `WorkspacePerformanceTestCase`.
 */

import XCTest
@testable import RalphCore

@MainActor
final class WorkspaceCorePerformanceTests: WorkspacePerformanceTestCase {
    func test_detectTaskChanges_performance_1000Tasks() {
        let previous = generateTasks(count: 1000)
        let current = generateTasks(count: 1000, mutateFrom: previous)

        measure {
            _ = workspace.detectTaskChanges(previous: previous, current: current)
        }
    }

    func test_isTaskBlocked_performance_500Tasks() {
        workspace.tasks = generateTasksWithDependencies(count: 500)
        let testTask = RalphTask(
            id: "RQ-TEST",
            status: .todo,
            title: "Test Task",
            priority: .high,
            dependsOn: (1...10).map { "RQ-\($0)" }
        )

        measure {
            for _ in 0..<100 {
                _ = workspace.isTaskBlocked(testTask)
            }
        }
    }

    func test_outputBuffer_enforcesMaxCharacters() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 100)
        buffer.append(String(repeating: "a", count: 150))

        XCTAssertLessThanOrEqual(buffer.content.count, 110)
        XCTAssertTrue(buffer.isTruncated)
    }

    func test_outputBuffer_preservesTrailingContent() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 50)
        buffer.append("START_")
        buffer.append(String(repeating: "x", count: 100))
        buffer.append("_END")

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
        XCTAssertLessThanOrEqual(buffer.content.count, 30)
    }

    func test_outputBuffer_clear_resetsState() {
        let buffer = ConsoleOutputBuffer(maxCharacters: 10)
        buffer.append("12345678901")
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

        buffer.maxCharacters = 50
        XCTAssertTrue(buffer.isTruncated)
        XCTAssertLessThanOrEqual(buffer.content.count, 70)
    }

    func test_outputBuffer_clampsToHardLimit() {
        let tooLarge = ConsoleOutputBuffer.hardMaxCharacters + 500_000
        let buffer = ConsoleOutputBuffer(maxCharacters: tooLarge)
        XCTAssertEqual(buffer.maxCharacters, ConsoleOutputBuffer.hardMaxCharacters)

        buffer.maxCharacters = tooLarge
        XCTAssertEqual(buffer.maxCharacters, ConsoleOutputBuffer.hardMaxCharacters)
    }

    func test_ansiSegmentLimit_enforced() {
        var ansiOutput = ""
        for index in 0..<200 {
            ansiOutput += "\u{001B}[3\(index % 8)mtext\(index)\u{001B}[0m "
        }

        workspace.maxANSISegments = 50
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        XCTAssertLessThanOrEqual(workspace.attributedOutput.count, 51)
    }

    func test_ansiSegmentLimit_indicatorAdded() {
        var ansiOutput = ""
        for index in 0..<100 {
            ansiOutput += "\u{001B}[3\(index % 8)mtext\(index)\u{001B}[0m "
        }

        workspace.maxANSISegments = 20
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        XCTAssertEqual(workspace.attributedOutput.first?.text, "\n... [console output truncated due to length] ...\n")
        XCTAssertEqual(workspace.attributedOutput.first?.color, .yellow)
        XCTAssertTrue(workspace.attributedOutput.first?.isItalic ?? false)
    }

    func test_ansiSegmentLimit_dynamicChange() {
        var ansiOutput = ""
        for index in 0..<100 {
            ansiOutput += "\u{001B}[3\(index % 8)mtext\(index)\u{001B}[0m "
        }

        workspace.maxANSISegments = 200
        workspace.parseANSICodes(from: ansiOutput)
        workspace.enforceANSISegmentLimit()

        let initialCount = workspace.attributedOutput.count
        XCTAssertGreaterThan(initialCount, 0)

        workspace.maxANSISegments = 10
        XCTAssertLessThanOrEqual(workspace.attributedOutput.count, 11)
    }

    func test_ansiSegmentLimit_noTruncationWhenUnderLimit() {
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

    func test_consumeStreamTextChunk_incrementalFlush_mergesPlainTextIntoSingleSegment() {
        workspace.runState.ingestConsoleText("hello")
        workspace.consumeStreamTextChunk("hello")
        workspace.runState.ingestConsoleText(" world")
        workspace.consumeStreamTextChunk(" world")
        workspace.runState.flushConsoleRenderState()

        XCTAssertEqual(workspace.output, "hello world")
        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput.first?.text, "hello world")
    }

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
}
