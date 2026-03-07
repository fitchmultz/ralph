//! Workspace+GraphData
//!
//! Responsibilities:
//! - Load dependency graph data from the Ralph CLI.
//!
//! Does not handle:
//! - Graph layout or visualization.
//! - Queue task loading or mutations.
//! - Analytics loading.
//!
//! Invariants/assumptions callers must respect:
//! - Graph payloads must conform to `RalphGraphDocument`.
//! - Errors are surfaced through the workspace recovery state.

import Foundation

public extension Workspace {
    func loadGraphData(retryConfiguration: RetryConfiguration = .default) async {
        guard let client else {
            graphDataErrorMessage = "CLI client not available."
            return
        }

        graphDataLoading = true
        graphDataErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "graph", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.graphDataErrorMessage = "Retrying load graph (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            guard collected.status.code == 0 else {
                graphDataErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load graph data (exit \(collected.status.code))."
                    : collected.stderr
                graphDataLoading = false
                return
            }

            graphData = try JSONDecoder().decode(RalphGraphDocument.self, from: Data(collected.stdout.utf8))
        } catch {
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadGraphData",
                workspaceURL: workingDirectoryURL
            )
            graphDataErrorMessage = recoveryError.message
            lastRecoveryError = recoveryError
            showErrorRecovery = true
        }

        graphDataLoading = false
    }
}
