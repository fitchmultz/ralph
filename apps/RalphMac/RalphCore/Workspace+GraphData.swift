//! Workspace+GraphData
//!
//! Purpose:
//! - Load dependency graph data from the Ralph CLI.
//!
//! Responsibilities:
//! - Load dependency graph data from the Ralph CLI.
//!
//! Does not handle:
//! - Graph layout or visualization.
//! - Queue task loading or mutations.
//! - Analytics loading.
//!
//!
//! Usage:
//! - Used by the RalphMac app or RalphCore tests through its owning feature surface.
//! Invariants/assumptions callers must respect:
//! - Graph payloads must conform to `MachineGraphReadDocument`.
//! - Errors are surfaced through the workspace recovery state.
//!
import Foundation

public extension Workspace {
    func loadGraphData(retryConfiguration: RetryConfiguration = .default) async {
        await performRepositoryLoad(
            operation: "loadGraphData",
            retryConfiguration: retryConfiguration,
            setLoading: { [insightsState] in insightsState.graphDataLoading = $0 },
            clearFailure: { [insightsState] in
                insightsState.graphDataErrorMessage = nil
            },
            handleMissingClient: { [insightsState] in
                insightsState.graphDataErrorMessage = "CLI client not available."
            },
            retryMessage: { attempt, maxAttempts in
                "Retrying load graph (attempt \(attempt)/\(maxAttempts))..."
            },
            load: { [self] client, workingDirectoryURL, retryConfiguration, onRetry in
                try await self.decodeMachineRepositoryJSON(
                    MachineGraphReadDocument.self,
                    client: client,
                    machineArguments: ["queue", "graph"],
                    currentDirectoryURL: workingDirectoryURL,
                    retryConfiguration: retryConfiguration,
                    onRetry: onRetry
                )
            },
            apply: { [insightsState] document in
                insightsState.graphData = document.graph
            },
            handleRetryMessage: { [insightsState] in
                insightsState.graphDataErrorMessage = $0
            },
            handleFailure: { [insightsState] recoveryError in
                insightsState.graphDataErrorMessage = recoveryError.message
            }
        )
    }
}
