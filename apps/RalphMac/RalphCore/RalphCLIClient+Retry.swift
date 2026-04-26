/**
 RalphCLIClient+Retry

 Purpose:
 - Add retry-aware collection APIs on top of `RalphCLIClient`.

 Responsibilities:
 - Add retry-aware collection APIs on top of `RalphCLIClient`.
 - Convert failed collected output into retryable error shapes.
 - Define timeout errors used by both retries and health checks.

 Does not handle:
 - Process spawning or pipe management.
 - Recovery UI categorization.
 - Health-check orchestration.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Retries are intended for transient failures only.
 - Non-zero exit codes are converted into errors before retry decisions run.
 */

public import Foundation

public extension RalphCLIClient {
    func runAndCollectWithRetry(
        arguments: [String],
        currentDirectoryURL: URL? = nil,
        environment: [String: String] = [:],
        maxOutputSize: Int? = nil,
        retryConfiguration: RetryConfiguration = .default,
        onRetry: RetryProgressHandler? = nil
    ) async throws -> CollectedOutput {
        let helper = RetryHelper(configuration: retryConfiguration)

        return try await helper.execute(
            operation: { [self] in
                let result = try await self.runAndCollect(
                    arguments: arguments,
                    currentDirectoryURL: currentDirectoryURL,
                    environment: environment,
                    maxOutputSize: maxOutputSize
                )
                if result.status.code != 0 {
                    throw result.toError()
                }
                return result
            },
            shouldRetry: { error in
                if error is RalphCLIClientError {
                    return false
                }
                return RetryHelper.defaultShouldRetry(error)
            },
            onProgress: onRetry
        )
    }
}

/// Error thrown when an operation times out.
public struct TimeoutError: Error, Sendable {}

public extension RalphCLIClient.CollectedOutput {
    func machineError(operation: String) throws -> MachineErrorDocument? {
        try MachineErrorDocument.decodeIfPresent(from: stderr, operation: operation)
    }

    func failureMessage(
        operation: String = "format process failure",
        fallback fallbackMessage: @autoclosure () -> String
    ) -> String {
        do {
            if let machineError = try machineError(operation: operation) {
                return machineError.userFacingDescription
            }
        } catch let recovery as RecoveryError {
            return recovery.message
        } catch {
            return error.localizedDescription
        }

        let trimmedStderr = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmedStderr.isEmpty {
            return trimmedStderr
        }

        return fallbackMessage()
    }

    var isRetryableFailure: Bool {
        guard status.code != 0 else { return false }

        do {
            if let machineError = try machineError(operation: "classify retryable process failure") {
                return machineError.retryable
            }
        } catch {
            return false
        }
        return RalphCLITransientErrorPolicy.isRetryableProcessError(
            exitCode: status.code,
            stderr: stderr
        )
    }

    func toError() -> any Error {
        RetryableError.processError(exitCode: status.code, stderr: stderr)
    }
}
