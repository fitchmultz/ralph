/**
 Workspace+ErrorRecovery

 Responsibilities:
 - Define workspace-scoped mutation/conflict error cases.
 - Convert raw operation failures into recovery UI state.
 - Reset recovery UI state once the user dismisses it.

 Does not handle:
 - CLI health checking internals.
 - Conflict diff rendering.
 - Task mutation request construction.

 Invariants/assumptions callers must respect:
 - Recovery state is mutated on the main actor with the rest of `Workspace`.
 - `taskConflict` always carries the latest task snapshot loaded from disk.
 */

public import Foundation

public extension Workspace {
    enum WorkspaceError: Error, LocalizedError {
        case cliClientUnavailable
        case cliError(String)
        case taskConflict(RalphTask)

        public var errorDescription: String? {
            switch self {
            case .cliClientUnavailable:
                return "CLI client is not available."
            case .cliError(let message):
                return message
            case .taskConflict:
                return "Task has been modified externally. Please resolve the conflict before saving."
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
    func clearErrorRecovery() {
        diagnosticsState.lastRecoveryError = nil
        diagnosticsState.showErrorRecovery = false
        diagnosticsState.retryState = nil
    }
}
