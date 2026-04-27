/**
 WorkspaceTaskCreationTestSupport

 Purpose:
 - Centralize CLI/bootstrap and queue-document helpers for workspace task-creation and watcher integration tests.

 Responsibilities:
 - Centralize CLI/bootstrap and queue-document helpers for workspace task-creation and watcher integration tests.

 Does not handle:
 - Defining task-creation or watcher assertions.
 - UI automation flows.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - A deterministic `ralph` binary is available via `RALPH_BIN_PATH` or the bundled app binary.
 */

import Foundation
import XCTest

@testable import RalphCore

enum WorkspaceTaskCreationTestSupport {
    static func runChecked(
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

    static func prepareWatcherFixture(at workspaceURL: URL) throws -> URL {
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

    static func writeQueueDocument(to url: URL, tasks: [RalphTask]) throws {
        let document = RalphTaskQueueDocument(tasks: tasks)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(document)
        try data.write(to: url, options: .atomic)
    }

    static func removeItemIfExists(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        try FileManager.default.removeItem(at: url)
    }

    static func resolveRalphBinaryURL() throws -> URL {
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

        let bundledURL = Bundle(for: RalphCoreTestCase.self).bundleURL
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

    static func makeTempDir(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }
}

extension WorkspaceQueueRefreshTests {
    static func workspaceOverviewCapabilitySpecDocument(
        supportsWorkspaceOverview: Bool
    ) -> MachineCLISpecDocument {
        let queueCommand = commandSpec(
            name: "queue",
            path: ["ralph", "machine", "queue"]
        )
        let workspaceOverviewCommand = commandSpec(
            name: "overview",
            path: ["ralph", "machine", "workspace", "overview"]
        )
        let workspaceCommand = commandSpec(
            name: "workspace",
            path: ["ralph", "machine", "workspace"],
            subcommands: supportsWorkspaceOverview ? [workspaceOverviewCommand] : []
        )
        let machineSubcommands = supportsWorkspaceOverview
            ? [queueCommand, workspaceCommand]
            : [queueCommand]

        return MachineCLISpecDocument(
            version: RalphMachineContract.cliSpecVersion,
            spec: RalphCLISpecDocument(
                version: RalphCLISpecDocument.expectedVersion,
                root: commandSpec(
                    name: "ralph",
                    path: ["ralph"],
                    subcommands: [
                        commandSpec(
                            name: "machine",
                            path: ["ralph", "machine"],
                            subcommands: machineSubcommands
                        )
                    ]
                )
            )
        )
    }

    static func commandSpec(
        name: String,
        path: [String],
        subcommands: [RalphCLICommandSpec] = []
    ) -> RalphCLICommandSpec {
        RalphCLICommandSpec(
            name: name,
            path: path,
            about: nil,
            longAbout: nil,
            afterLongHelp: nil,
            hidden: false,
            args: [],
            subcommands: subcommands
        )
    }
}
