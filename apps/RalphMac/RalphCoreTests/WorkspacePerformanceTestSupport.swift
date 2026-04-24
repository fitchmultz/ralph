/**
 WorkspacePerformanceTestSupport

 Purpose:
 - Keep expensive fixture-generation and wait helpers centralized across workspace-focused test files.

 Responsibilities:
 - Keep expensive fixture-generation and wait helpers centralized across workspace-focused test files.

 Does not handle:
 - Owning a per-test workspace instance.
 - Defining regression assertions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Helpers remain thin delegates over the canonical RalphCore test-support utilities.
 */

import Foundation

@testable import RalphCore

enum WorkspacePerformanceTestSupport {
    static func makeTempDir(prefix: String) throws -> URL {
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }

    static func makeExecutableScript(in directory: URL, name: String, body: String) throws -> URL {
        try RalphMockCLITestSupport.makeExecutableScript(in: directory, name: name, body: body)
    }

    static func makeVersionAwareMockCLI(in directory: URL, name: String) throws -> URL {
        try RalphMockCLITestSupport.makeVersionAwareMockCLI(in: directory, name: name)
    }

    static func writeEmptyQueueFile(in workspaceDir: URL) throws {
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceDir, tasks: [])
    }

    static func writeQueueFile(in workspaceDir: URL, tasksJSON: String) throws {
        let data = Data(tasksJSON.utf8)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let tasks = try decoder.decode([RalphTask].self, from: data)
        try RalphMockCLITestSupport.writeQueueFile(in: workspaceDir, tasks: tasks)
    }

    static func waitFor(
        timeout: TimeInterval,
        pollInterval: Duration = .milliseconds(50),
        condition: @escaping @MainActor () -> Bool
    ) async -> Bool {
        await RalphCoreTestSupport.waitUntil(
            timeout: .seconds(timeout),
            pollInterval: pollInterval
        ) {
            await MainActor.run { condition() }
        }
    }
}
