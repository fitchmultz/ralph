/**
 RalphCoreTestSupport

 Responsibilities:
 - Centralize deterministic temp-directory, workspace-path, cleanup, and async wait utilities for RalphCore tests.
 - Provide one portable source of workspace fixtures so tests do not hardcode `/tmp` or hide filesystem failures.
 - Offer assertion-friendly helpers for readiness checks that would otherwise use ad hoc sleeps or polling loops.

 Does not handle:
 - Production workspace behavior.
 - UI automation helpers for the separate UI-test target.

 Usage:
 - Use `makeTemporaryDirectory(prefix:)` for filesystem fixtures that must be cleaned up explicitly.
 - Use `makeWorkspace(label:)` or `workspaceURL(label:)` for synthetic test workspaces.
 - Use `waitUntil(timeout:pollInterval:condition:)` to express readiness without embedding sleeps in individual tests.

 Invariants/assumptions:
 - All temp fixtures live under `FileManager.default.temporaryDirectory`.
 - Cleanup helpers treat missing paths as success so repeated teardown remains deterministic.
 - Wait helpers are timeout-bounded and return a final condition evaluation before failing.
 */

import Foundation
import XCTest
@testable import RalphCore

#if canImport(Darwin)
import Darwin
#endif

class RalphCoreTestCase: XCTestCase {
    override func setUpWithError() throws {
        try super.setUpWithError()
        RalphCoreTestSupport.resetPersistentTestState()
    }

    override func tearDownWithError() throws {
        RalphCoreTestSupport.resetPersistentTestState()
        try super.tearDownWithError()
    }
}

enum RalphCoreTestSupport {
    private static let tempRootName = "ralph-core-tests"

    static func workspaceURL(label: String = #function) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent(tempRootName, isDirectory: true)
            .appendingPathComponent(sanitizedPathComponent(label), isDirectory: true)
    }

    static func makeTemporaryDirectory(
        prefix: String,
        fileID: String = #fileID,
        function: String = #function
    ) throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(tempRootName, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

        let fileComponent = sanitizedPathComponent((fileID as NSString).lastPathComponent.replacingOccurrences(of: ".swift", with: ""))
        let functionComponent = sanitizedPathComponent(function)
        let directory = root.appendingPathComponent(
            "\(sanitizedPathComponent(prefix))-\(fileComponent)-\(functionComponent)-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    @MainActor
    static func makeWorkspace(
        label: String = #function,
        client: RalphCLIClient? = nil
    ) throws -> Workspace {
        let directory = try makeTemporaryDirectory(prefix: label)
        if let client {
            return Workspace(workingDirectoryURL: directory, client: client)
        }
        return Workspace(workingDirectoryURL: directory)
    }

    static func ensureDirectory(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }

    static func resetPersistentTestState() {
        RalphAppDefaults.resetUnitTestingDefaults()
    }

    static func removeItemIfExists(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return
        }
        try FileManager.default.removeItem(at: url)
    }

    static func assertRemoved(_ url: URL, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertNoThrow(
            try removeItemIfExists(url),
            "Expected cleanup for \(url.path) to succeed",
            file: file,
            line: line
        )
    }

    @MainActor
    static func shutdownAndRemove(
        _ url: URL,
        _ workspaces: Workspace?...,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        for workspace in workspaces {
            workspace?.shutdown()
            workspace?.clearCachedTasks()
            workspace?.removePersistedState()
        }
        assertRemoved(url, file: file, line: line)
    }

    static func waitUntil(
        timeout: Duration = .seconds(5),
        pollInterval: Duration = .milliseconds(20),
        condition: @escaping @Sendable () async -> Bool
    ) async -> Bool {
        if await condition() {
            return true
        }

        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        var now = clock.now

        while now < deadline {
            do {
                try await clock.sleep(for: pollInterval)
            } catch {
                return await condition()
            }

            if await condition() {
                return true
            }
            now = clock.now
        }

        return await condition()
    }

    static func waitForFile(
        _ url: URL,
        timeout: Duration = .seconds(2),
        pollInterval: Duration = .milliseconds(20)
    ) async -> Bool {
        await waitUntil(timeout: timeout, pollInterval: pollInterval) {
            FileManager.default.fileExists(atPath: url.path)
        }
    }

    static func waitForProcessExit(
        _ pid: pid_t,
        timeout: Duration = .seconds(3),
        pollInterval: Duration = .milliseconds(20)
    ) async -> Bool {
        await waitUntil(timeout: timeout, pollInterval: pollInterval) {
            processHasExited(pid)
        }
    }

    static func assertEventually(
        _ message: @autoclosure () -> String,
        timeout: Duration = .seconds(5),
        pollInterval: Duration = .milliseconds(20),
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @escaping @Sendable () async -> Bool
    ) async {
        let satisfied = await waitUntil(timeout: timeout, pollInterval: pollInterval, condition: condition)
        if !satisfied {
            XCTFail(message(), file: file, line: line)
        }
    }

    private static func processHasExited(_ pid: pid_t) -> Bool {
        #if canImport(Darwin)
        if kill(pid, 0) != 0 && errno == ESRCH {
            return true
        }
        #endif
        return false
    }

    private static func sanitizedPathComponent(_ raw: String) -> String {
        let replaced = raw.replacingOccurrences(
            of: "[^A-Za-z0-9._-]+",
            with: "-",
            options: .regularExpression
        )
        let trimmed = replaced.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "fixture" : trimmed
    }
}

@MainActor
extension Workspace {
    var workingDirectoryURL: URL {
        identityState.workingDirectoryURL
    }

    var tasks: [RalphTask] {
        get { taskState.tasks }
        set { taskState.tasks = newValue }
    }

    var tasksLoading: Bool {
        get { taskState.tasksLoading }
        set { taskState.tasksLoading = newValue }
    }

    var tasksErrorMessage: String? {
        get { taskState.tasksErrorMessage }
        set { taskState.tasksErrorMessage = newValue }
    }

    var cliSpecErrorMessage: String? {
        get { commandState.cliSpecErrorMessage }
        set { commandState.cliSpecErrorMessage = newValue }
    }

    var cliSpecIsLoading: Bool {
        get { commandState.cliSpecIsLoading }
        set { commandState.cliSpecIsLoading = newValue }
    }

    var cliHealthStatus: CLIHealthStatus? {
        get { diagnosticsState.cliHealthStatus }
        set { diagnosticsState.cliHealthStatus = newValue }
    }

    var cachedTasks: [RalphTask] {
        get { diagnosticsState.cachedTasks }
        set { diagnosticsState.cachedTasks = newValue }
    }

    var operationalIssues: [WorkspaceOperationalIssue] {
        diagnosticsState.operationalIssues
    }

    var operationalSummary: WorkspaceOperationalSummary {
        diagnosticsState.operationalSummary
    }

    var showOfflineBanner: Bool {
        diagnosticsState.cliHealthStatus?.isAvailable == false
    }

    var isShowingCachedTasks: Bool {
        showOfflineBanner && !diagnosticsState.cachedTasks.isEmpty
    }

    var output: String {
        get { runState.output }
        set { runState.output = newValue }
    }

    var isRunning: Bool {
        get { runState.isRunning }
        set { runState.isRunning = newValue }
    }

    var lastExitStatus: RalphCLIExitStatus? {
        get { runState.lastExitStatus }
        set { runState.lastExitStatus = newValue }
    }

    var currentTaskID: String? {
        get { runState.currentTaskID }
        set { runState.currentTaskID = newValue }
    }

    var currentPhase: Workspace.ExecutionPhase? {
        get { runState.currentPhase }
        set { runState.currentPhase = newValue }
    }

    var executionHistory: [Workspace.ExecutionRecord] {
        get { runState.executionHistory }
        set { runState.executionHistory = newValue }
    }

    var attributedOutput: [Workspace.ANSISegment] {
        get { runState.attributedOutput }
        set { runState.attributedOutput = newValue }
    }

    var maxANSISegments: Int {
        get { runState.maxANSISegments }
        set { runState.maxANSISegments = newValue }
    }

    var runControlSelectedTaskID: String? {
        get { runState.runControlSelectedTaskID }
        set { runState.runControlSelectedTaskID = newValue }
    }

    var isLoopMode: Bool {
        get { runState.isLoopMode }
        set { runState.isLoopMode = newValue }
    }

    var stopAfterCurrent: Bool {
        get { runState.stopAfterCurrent }
        set { runState.stopAfterCurrent = newValue }
    }
}
