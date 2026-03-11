/**
 WorkspaceTaskCreationTests

 Responsibilities:
 - Verify Workspace task creation uses a deterministic direct-create path.
 - Exercise real CLI-backed task creation against an isolated temp workspace.
 - Guard against regressions where UI task creation invokes the long-running AI task builder.

 Does not handle:
 - UI automation flows (covered by RalphMacUITests).
 - Template-specific task creation behavior.

 Invariants/assumptions callers must respect:
 - A deterministic `ralph` binary is available via `RALPH_BIN_PATH` or the bundled app binary.
 - Tests run in an isolated temp directory initialized with `ralph init --non-interactive`.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceTaskCreationTests: XCTestCase {
    func test_createTask_importsStructuredTaskImmediately() async throws {
        let workspaceURL = try Self.makeTempDir(prefix: "ralph-workspace-create-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let client = try RalphCLIClient(executableURL: try Self.resolveRalphBinaryURL())
        try await Self.runChecked(
            client: client,
            arguments: ["--no-color", "init", "--non-interactive"],
            currentDirectoryURL: workspaceURL
        )

        let workspace = Workspace(workingDirectoryURL: workspaceURL, client: client)

        try await workspace.createTask(
            title: "UI-created task",
            description: "Created without invoking task builder",
            priority: .high,
            tags: ["ui", "direct-create"],
            scope: ["apps/RalphMac/RalphMac/TaskCreationView.swift"]
        )

        let loadedCreatedTask = await RalphCoreTestSupport.waitUntil(timeout: .seconds(5)) {
            await MainActor.run {
            workspace.taskState.tasks.count == 1
            }
        }
        XCTAssertTrue(loadedCreatedTask)

        let tasks = workspace.taskState.tasks
        XCTAssertEqual(tasks.count, 1)
        let task = try XCTUnwrap(tasks.first)
        XCTAssertEqual(task.title, "UI-created task")
        XCTAssertEqual(task.description, "Created without invoking task builder")
        XCTAssertEqual(task.priority, .high)
        XCTAssertEqual(task.tags, ["ui", "direct-create"])
        XCTAssertEqual(task.scope, ["apps/RalphMac/RalphMac/TaskCreationView.swift"])
        XCTAssertEqual(task.status, .todo)
    }

    func test_queueFileWatcher_rapidStartStopWithMutationsDoesNotCrash() async throws {
        for index in 0..<20 {
            let workspaceURL = try Self.makeTempDir(prefix: "ralph-workspace-watcher-")
            defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }
            let ralphURL = try Self.prepareWatcherFixture(at: workspaceURL)

            let watcher = QueueFileWatcher(workingDirectoryURL: workspaceURL)
            let notification = expectation(description: "watcher-notification-\(index)")
            notification.assertForOverFulfill = false
            let eventTask = Task {
                for await event in watcher.events {
                    if case .filesChanged(let batch) = event,
                       batch.fileNames.contains("queue.jsonc") {
                        notification.fulfill()
                        return
                    }
                }
            }

            await watcher.start()
            let queueURL = ralphURL.appendingPathComponent("queue.jsonc", isDirectory: false)
            try """
            [
              { "id": "RQ-\(String(format: "%04d", index))", "title": "iteration \(index)", "status": "todo" }
            ]
            """.write(to: queueURL, atomically: true, encoding: .utf8)

            await fulfillment(of: [notification], timeout: 5.0)
            eventTask.cancel()
            await watcher.stop()
        }
    }

    func test_queueFileWatcher_stopBeforeDebounceSuppressesNotification() async throws {
        let workspaceURL = try Self.makeTempDir(prefix: "ralph-workspace-watcher-stop-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let ralphURL = try Self.prepareWatcherFixture(at: workspaceURL)

        let watcher = QueueFileWatcher(workingDirectoryURL: workspaceURL)
        let invertedNotification = expectation(description: "watcher-stopped-before-debounce")
        invertedNotification.isInverted = true
        let eventTask = Task {
            for await event in watcher.events {
                if case .filesChanged = event {
                    invertedNotification.fulfill()
                    return
                }
            }
        }

        await watcher.start()
        try """
        [
          { "id": "RQ-STOP", "title": "stop before debounce", "status": "todo" }
        ]
        """.write(
            to: ralphURL.appendingPathComponent("queue.jsonc", isDirectory: false),
            atomically: true,
            encoding: .utf8
        )
        await watcher.stop()
        eventTask.cancel()

        await fulfillment(of: [invertedNotification], timeout: 1.0)
    }

    func test_queueFileWatcher_surfacesFailureAfterRetryExhaustion() async throws {
        let workspaceURL = try Self.makeTempDir(prefix: "ralph-workspace-watcher-fail-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let watcher = QueueFileWatcher(
            workingDirectoryURL: workspaceURL,
            configuration: QueueFileWatcher.Configuration(
                debounceInterval: .milliseconds(10),
                retryBaseDelay: .milliseconds(10),
                maxStartAttempts: 2,
                streamLatency: 0.01
            ),
            system: .init(
                create: { _, _, _, _, _ in nil },
                setDispatchQueue: { _, _ in },
                start: { _ in false },
                stop: { _ in },
                invalidate: { _ in }
            )
        )

        let failure = expectation(description: "watcher-failed")
        let eventTask = Task {
            for await event in watcher.events {
                if case .healthChanged(let health) = event,
                   case .failed(let reason, let attempts) = health.state {
                    XCTAssertEqual(reason, "Failed to create FSEvent stream")
                    XCTAssertEqual(attempts, 2)
                    failure.fulfill()
                    return
                }
            }
        }

        await watcher.start()
        await fulfillment(of: [failure], timeout: 2.0)
        eventTask.cancel()
        await watcher.stop()
    }

    private static func runChecked(
        client: RalphCLIClient,
        arguments: [String],
        currentDirectoryURL: URL
    ) async throws {
        let result = try await client.runAndCollect(
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL
        )
        XCTAssertEqual(result.status.code, 0, "Command failed: \(arguments.joined(separator: " "))\nstderr:\n\(result.stderr)")
    }

    private static func prepareWatcherFixture(at workspaceURL: URL) throws -> URL {
        let ralphURL = workspaceURL.appendingPathComponent(".ralph", isDirectory: true)
        try FileManager.default.createDirectory(at: ralphURL, withIntermediateDirectories: true)
        try "[]\n".write(
            to: ralphURL.appendingPathComponent("done.jsonc", isDirectory: false),
            atomically: true,
            encoding: .utf8
        )
        try "{}\n".write(
            to: ralphURL.appendingPathComponent("config.jsonc", isDirectory: false),
            atomically: true,
            encoding: .utf8
        )
        return ralphURL
    }

    private static func resolveRalphBinaryURL() throws -> URL {
        if let override = ProcessInfo.processInfo.environment["RALPH_BIN_PATH"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !override.isEmpty {
            let overrideURL = URL(fileURLWithPath: override)
            guard FileManager.default.isExecutableFile(atPath: overrideURL.path) else {
                throw NSError(
                    domain: "WorkspaceTaskCreationTests",
                    code: 2,
                    userInfo: [NSLocalizedDescriptionKey: "RALPH_BIN_PATH points to a non-executable path: \(overrideURL.path)"]
                )
            }
            return overrideURL
        }

        let bundledURL = Bundle(for: WorkspaceTaskCreationTests.self).bundleURL
            .deletingLastPathComponent()
            .appendingPathComponent("RalphMac.app", isDirectory: true)
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)
        if FileManager.default.isExecutableFile(atPath: bundledURL.path) {
            return bundledURL
        }

        throw NSError(
            domain: "WorkspaceTaskCreationTests",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: "Failed to locate a usable ralph binary for WorkspaceTaskCreationTests"]
        )
    }

    private static func makeTempDir(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }
}
