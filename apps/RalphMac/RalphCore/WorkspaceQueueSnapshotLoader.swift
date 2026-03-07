//! WorkspaceQueueSnapshotLoader
//!
//! Responsibilities:
//! - Decode Ralph queue documents away from the main actor.
//! - Support both file-backed queue refreshes and CLI JSON output refreshes.
//! - Centralize queue decoding so watcher and manual refresh paths stay consistent.
//!
//! Does not handle:
//! - Publishing decoded tasks into SwiftUI state.
//! - Starting file watchers or deciding when refreshes should run.
//! - Any task filtering or sorting decisions.
//!
//! Invariants/assumptions callers must respect:
//! - Queue payloads must match `RalphTaskQueueDocument`.
//! - File URLs must point to the queue file to decode.
//! - Results are returned to callers for main-actor publication.

import Foundation

enum WorkspaceQueueSnapshotLoader {
    static func decodeQueueTasks(from data: Data) async throws -> [RalphTask] {
        try await Task.detached(priority: .userInitiated) {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let document = try decoder.decode(RalphTaskQueueDocument.self, from: data)
            return document.tasks
        }.value
    }

    static func decodeQueueTasks(fromCLIOutput output: String) async throws -> [RalphTask] {
        try await decodeQueueTasks(from: Data(output.utf8))
    }

    static func loadQueueTasks(from queueURL: URL) async throws -> [RalphTask] {
        try await Task.detached(priority: .userInitiated) {
            guard FileManager.default.fileExists(atPath: queueURL.path) else {
                throw URLError(.fileDoesNotExist)
            }

            let data = try Data(contentsOf: queueURL)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let document = try decoder.decode(RalphTaskQueueDocument.self, from: data)
            return document.tasks
        }.value
    }
}
