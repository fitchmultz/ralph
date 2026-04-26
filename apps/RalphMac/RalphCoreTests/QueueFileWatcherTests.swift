/**
 QueueFileWatcherTests

 Purpose:
 - Validate queue file watcher notification, debounce, replacement, and retry-exhaustion behavior.

 Responsibilities:
 - Validate queue file watcher notification, debounce, replacement, and retry-exhaustion behavior.

 Does not handle:
 - Higher-level workspace task creation flows.
 - Queue graph/analytics refresh assertions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests exercise the low-level watcher directly against isolated `.ralph` fixtures.
 */

import XCTest

@testable import RalphCore

@MainActor
final class QueueFileWatcherTests: RalphCoreTestCase {
    func test_queueFileWatcher_rapidStartStopWithMutationsDoesNotCrash() async throws {
        for index in 0..<20 {
            let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-")
            defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }
            let ralphURL = try WorkspaceTaskCreationTestSupport.prepareWatcherFixture(at: workspaceURL)

            let watcher = QueueFileWatcher(
                workingDirectoryURL: workspaceURL,
                configuration: Self.fastWatcherConfiguration
            )
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

            await fulfillment(of: [notification], timeout: 1.0)
            eventTask.cancel()
            await watcher.stop()
        }
    }

    func test_queueFileWatcher_stopBeforeDebounceSuppressesNotification() async throws {
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-stop-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let ralphURL = try WorkspaceTaskCreationTestSupport.prepareWatcherFixture(at: workspaceURL)

        let watcher = QueueFileWatcher(
            workingDirectoryURL: workspaceURL,
            configuration: Self.fastWatcherConfiguration
        )
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

        await fulfillment(of: [invertedNotification], timeout: 0.2)
    }

    func test_queueFileWatcher_replaceItemAtQueueFileEmitsNotification() async throws {
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-replace-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let ralphURL = try WorkspaceTaskCreationTestSupport.prepareWatcherFixture(at: workspaceURL)
        let queueURL = ralphURL.appendingPathComponent("queue.jsonc", isDirectory: false)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: queueURL,
            tasks: [
                RalphTask(id: "RQ-INITIAL", status: .todo, title: "Initial", priority: .medium)
            ]
        )

        let watcher = QueueFileWatcher(
            workingDirectoryURL: workspaceURL,
            configuration: Self.fastWatcherConfiguration
        )
        let notification = expectation(description: "watcher-replacement-notification")
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

        let replacementURL = workspaceURL.appendingPathComponent("queue-replacement.jsonc", isDirectory: false)
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: replacementURL,
            tasks: [
                RalphTask(id: "RQ-REPLACED", status: .todo, title: "Replacement", priority: .high)
            ]
        )
        defer { XCTAssertNoThrow(try WorkspaceTaskCreationTestSupport.removeItemIfExists(replacementURL)) }

        _ = try FileManager.default.replaceItemAt(queueURL, withItemAt: replacementURL)

        await fulfillment(of: [notification], timeout: 1.0)
        eventTask.cancel()
        await watcher.stop()
    }

    func test_queueFileWatcher_customTargetsEmitNotificationForConfiguredQueuePath() async throws {
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-custom-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let stateURL = workspaceURL.appendingPathComponent("state", isDirectory: true)
        try FileManager.default.createDirectory(at: stateURL, withIntermediateDirectories: true)

        let queueURL = stateURL.appendingPathComponent("queue.jsonc", isDirectory: false)
        let doneURL = stateURL.appendingPathComponent("done.jsonc", isDirectory: false)
        let configURL = workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
        try FileManager.default.createDirectory(
            at: configURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: queueURL, tasks: [])
        try "[]\n".write(to: doneURL, atomically: true, encoding: .utf8)
        try "{}\n".write(to: configURL, atomically: true, encoding: .utf8)

        let watcher = QueueFileWatcher(
            targets: QueueFileWatcher.WatchTargets(
                workingDirectoryURL: workspaceURL,
                queueFileURL: queueURL,
                doneFileURL: doneURL,
                projectConfigFileURL: configURL
            ),
            configuration: Self.fastWatcherConfiguration
        )
        let notification = expectation(description: "watcher-custom-target-notification")
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
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: queueURL,
            tasks: [
                RalphTask(id: "RQ-CUSTOM", status: .todo, title: "Configured target", priority: .medium)
            ]
        )

        await fulfillment(of: [notification], timeout: 1.0)
        eventTask.cancel()
        await watcher.stop()
    }

    func test_queueFileWatcher_customQueueFileNameStillMarksQueueSnapshotChanges() async throws {
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-custom-name-")
        defer { RalphCoreTestSupport.assertRemoved(workspaceURL) }

        let stateURL = workspaceURL.appendingPathComponent("state", isDirectory: true)
        try FileManager.default.createDirectory(at: stateURL, withIntermediateDirectories: true)

        let queueURL = stateURL.appendingPathComponent("tasks.jsonc", isDirectory: false)
        let doneURL = stateURL.appendingPathComponent("archive.jsonc", isDirectory: false)
        let configURL = workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
        try FileManager.default.createDirectory(
            at: configURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(to: queueURL, tasks: [])
        try "[]\n".write(to: doneURL, atomically: true, encoding: .utf8)
        try "{}\n".write(to: configURL, atomically: true, encoding: .utf8)

        let watcher = QueueFileWatcher(
            targets: QueueFileWatcher.WatchTargets(
                workingDirectoryURL: workspaceURL,
                queueFileURL: queueURL,
                doneFileURL: doneURL,
                projectConfigFileURL: configURL
            ),
            configuration: Self.fastWatcherConfiguration
        )
        let notification = expectation(description: "watcher-custom-queue-name-batch")
        let eventTask = Task {
            for await event in watcher.events {
                if case .filesChanged(let batch) = event, batch.affectsQueueSnapshot {
                    notification.fulfill()
                    return
                }
            }
        }

        await watcher.start()
        try WorkspaceTaskCreationTestSupport.writeQueueDocument(
            to: queueURL,
            tasks: [RalphTask(id: "RQ-CUSTOM-NAME", status: .todo, title: "Custom queue file", priority: .medium)]
        )

        await fulfillment(of: [notification], timeout: 1.0)
        eventTask.cancel()
        await watcher.stop()
    }

    func test_queueFileWatcher_surfacesFailureAfterRetryExhaustion() async throws {
        let workspaceURL = try WorkspaceTaskCreationTestSupport.makeTempDir(prefix: "ralph-workspace-watcher-fail-")
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

    private static let fastWatcherConfiguration = QueueFileWatcher.Configuration(
        debounceInterval: .milliseconds(10),
        retryBaseDelay: .milliseconds(10),
        maxStartAttempts: 2,
        streamLatency: 0.01
    )
}
