/**
 RalphCLIClient+Retry

 Responsibilities:
 - Add retry-aware collection APIs on top of `RalphCLIClient`.
 - Convert failed collected output into retryable error shapes.
 - Define timeout errors used by both retries and health checks.

 Does not handle:
 - Process spawning or pipe management.
 - Recovery UI categorization.
 - Health-check orchestration.

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
    var isRetryableFailure: Bool {
        guard status.code != 0 else { return false }

        let lowercasedStderr = stderr.lowercased()
        let retryablePatterns = [
            "resource temporarily unavailable",
            "operation would block",
            "device or resource busy",
            "resource busy",
            "file is locked",
            "io timeout",
            "eagain",
            "ewouldblock",
            "ebusy",
            "locked",
            "try again"
        ]

        return retryablePatterns.contains { lowercasedStderr.contains($0) }
    }

    func toError() -> any Error {
        RetryableError.processError(exitCode: status.code, stderr: stderr)
    }
}
