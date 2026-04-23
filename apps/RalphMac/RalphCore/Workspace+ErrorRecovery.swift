/**
 Workspace+ErrorRecovery

 Responsibilities:
 - Define workspace-scoped mutation/conflict error cases.
 - Convert raw operation failures into recovery UI state.
 - Execute queue continuation validation, repair, and restore commands for app recovery flows.
 - Reset recovery UI state once the user dismisses it.

 Does not handle:
 - CLI health checking internals.
 - Conflict diff rendering.
 - Task mutation request construction.

 Invariants/assumptions callers must respect:
 - Recovery state is mutated on the main actor with the rest of `Workspace`.
 - `taskConflict` always carries the latest task snapshot loaded from disk.
 - Queue repair and restore writes refresh task state after success.
 */

public import Foundation

extension Workspace {
    public enum WorkspaceError: Error, LocalizedError {
        case cliClientUnavailable
        case cliError(String)
        case taskConflict(RalphTask)

        public var errorDescription: String? {
            switch self {
            case .cliClientUnavailable:
                return "Ralph cannot continue because the CLI client is unavailable."
            case .cliError(let message):
                return message
            case .taskConflict:
                return "Ralph is blocked from continuing this edit because the task changed elsewhere. Review the conflict, choose which values to keep, then save again."
            }
        }
    }

    /// Report an error with recovery context.
    func reportError(_ error: any Error, operation: String) {
        let recoveryError = RecoveryError.classify(
            error: error,
            operation: operation,
            workspaceURL: identityState.workingDirectoryURL
        )
        diagnosticsState.lastRecoveryError = recoveryError
        diagnosticsState.showErrorRecovery = true

        RalphLogger.shared.error(
            "Operation '\(operation)' failed: \(recoveryError.message)",
            category: .workspace
        )
    }

    /// Clear error recovery state.
    public func clearErrorRecovery() {
        diagnosticsState.lastRecoveryError = nil
        diagnosticsState.showErrorRecovery = false
        diagnosticsState.retryState = nil
    }

    func validateQueueContinuation() async throws -> MachineQueueValidateDocument {
        try await runRecoveryCommand(
            arguments: ["--no-color", "machine", "queue", "validate"],
            operation: "validate queue continuation"
        )
    }

    func repairQueueContinuation(dryRun: Bool = true) async throws -> MachineQueueRepairDocument {
        var arguments = ["--no-color", "machine", "queue", "repair"]
        if dryRun {
            arguments.append("--dry-run")
        }

        let document: MachineQueueRepairDocument = try await runRecoveryCommand(
            arguments: arguments,
            operation: dryRun ? "preview queue repair" : "apply queue repair"
        )

        if !dryRun {
            await loadTasks()
        }
        return document
    }

    func restoreQueueContinuation(
        snapshotID: String? = nil,
        dryRun: Bool = true
    ) async throws -> MachineQueueUndoDocument {
        var arguments = ["--no-color", "machine", "queue", "undo"]
        if dryRun {
            arguments.append("--dry-run")
        }
        if let snapshotID, !snapshotID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            arguments.append(contentsOf: ["--id", snapshotID])
        }

        let document: MachineQueueUndoDocument = try await runRecoveryCommand(
            arguments: arguments,
            operation: dryRun ? "preview queue restore" : "restore queue continuation"
        )

        if !dryRun {
            await loadTasks()
        }
        return document
    }
}

private extension Workspace {
    func runRecoveryCommand<T: Decodable & VersionedMachineDocument & Sendable>(
        arguments: [String],
        operation: String
    ) async throws -> T {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        let collected = try await helper.execute(
            operation: { [self] in
                try await client.runAndCollect(
                    arguments: arguments,
                    currentDirectoryURL: identityState.workingDirectoryURL,
                    timeoutConfiguration: .longRunning
                )
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    self?.runState.errorMessage = "Retrying \(operation) (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(
                collected.failureMessage(fallback: "Failed to \(operation) (exit \(collected.status.code))")
            )
        }

        do {
            return try RalphMachineContract.decode(T.self, from: Data(collected.stdout.utf8), operation: operation)
        } catch {
            throw WorkspaceError.cliError(
                "Failed to decode \(operation) JSON output: \(error.localizedDescription)"
            )
        }
    }
}
