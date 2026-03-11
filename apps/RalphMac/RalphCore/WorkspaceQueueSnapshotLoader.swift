//! WorkspaceQueueSnapshotLoader
//!
//! Responsibilities:
//! - Decode Ralph machine queue-read documents away from the main actor.
//! - Centralize queue decoding so watcher and manual refresh paths stay consistent.
//!
//! Does not handle:
//! - Publishing decoded tasks into SwiftUI state.
//! - Starting file watchers or deciding when refreshes should run.
//! - Any task filtering or sorting decisions.
//!
//! Invariants/assumptions callers must respect:
//! - Queue snapshots must match `MachineQueueReadDocument`.
//! - Results are returned to callers for main-actor publication.

import Foundation

enum WorkspaceQueueSnapshotLoader {
    static func decodeQueueSnapshot(from data: Data) async throws -> MachineQueueReadDocument {
        try await Task.detached(priority: .userInitiated) {
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return try decoder.decode(MachineQueueReadDocument.self, from: data)
        }.value
    }
}
