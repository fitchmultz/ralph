/**
 RalphCoreTestSupport+Filesystem

 Purpose:
 - Centralize deterministic temp-directory, workspace-path, and cleanup helpers for RalphCore tests.

 Responsibilities:
 - Centralize deterministic temp-directory, workspace-path, and cleanup helpers for RalphCore tests.
 - Provide one portable source of filesystem fixtures so tests do not hardcode `/tmp` or hide cleanup failures.

 Does not handle:
 - Async wait utilities.
 - UI automation helpers.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - All temp fixtures live under `FileManager.default.temporaryDirectory`.
 - Cleanup helpers treat missing paths as success so repeated teardown remains deterministic.
 */

import Foundation
import XCTest

@testable import RalphCore

extension RalphCoreTestSupport {
    private static var tempRootName: String { "ralph-core-tests" }

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
