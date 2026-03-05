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
        defer { try? FileManager.default.removeItem(at: workspaceURL) }

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

        let tasks = workspace.tasks
        XCTAssertEqual(tasks.count, 1)
        let task = try XCTUnwrap(tasks.first)
        XCTAssertEqual(task.title, "UI-created task")
        XCTAssertEqual(task.description, "Created without invoking task builder")
        XCTAssertEqual(task.priority, .high)
        XCTAssertEqual(task.tags, ["ui", "direct-create"])
        XCTAssertEqual(task.scope, ["apps/RalphMac/RalphMac/TaskCreationView.swift"])
        XCTAssertEqual(task.status, .todo)
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
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(prefix + UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }
}
