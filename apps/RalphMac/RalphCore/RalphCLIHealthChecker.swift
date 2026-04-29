/**
 RalphCLIHealthChecker

 Purpose:
 - Probe whether the Ralph CLI is available for a workspace.

 Responsibilities:
 - Probe whether the Ralph CLI is available for a workspace.
 - Cache health results and coalesce duplicate in-flight checks.
 - Translate health failures into availability reasons for offline-mode UI.

 Does not handle:
 - Process retry loops outside health probes.
 - Workspace error recovery presentation.
 - Parsing command output beyond simple success/failure checks.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Health checks may cancel and replace an older in-flight check for the same workspace.
 - Health probing uses the machine system-info contract.
 - Cached status is keyed by `Workspace.id`.
 */

public import Foundation

public struct CLIHealthStatus: Sendable, Equatable {
    public struct Diagnostics: Sendable, Equatable {
        public let attempts: Int
        public let maxAttempts: Int
        public let finalMessage: String?

        public init(attempts: Int, maxAttempts: Int, finalMessage: String? = nil) {
            self.attempts = attempts
            self.maxAttempts = maxAttempts
            self.finalMessage = finalMessage
        }
    }

    public enum Availability: Sendable, Equatable {
        case available
        case unavailable(reason: UnavailabilityReason)
        case unknown
    }

    public enum UnavailabilityReason: Sendable, Equatable {
        case cliNotFound
        case cliNotExecutable
        case workspaceInaccessible
        case timeout
        case permissionDenied
        case unknown(String)

        public var errorCategory: ErrorCategory {
            switch self {
            case .cliNotFound, .cliNotExecutable:
                return .cliUnavailable
            case .workspaceInaccessible, .permissionDenied:
                return .permissionDenied
            case .timeout:
                return .networkError
            case .unknown:
                return .unknown
            }
        }
    }

    public let availability: Availability
    public let lastChecked: Date
    public let workspaceURL: URL
    public let diagnostics: Diagnostics?

    public init(
        availability: Availability,
        lastChecked: Date,
        workspaceURL: URL,
        diagnostics: Diagnostics? = nil
    ) {
        self.availability = availability
        self.lastChecked = lastChecked
        self.workspaceURL = workspaceURL
        self.diagnostics = diagnostics
    }

    public var isAvailable: Bool {
        if case .available = availability { return true }
        return false
    }
}

public actor CLIHealthChecker {
    private var cachedStatus: [UUID: CLIHealthStatus] = [:]
    private var checkTasks: [UUID: Task<CLIHealthStatus, Never>] = [:]
    private var checkTaskTokens: [UUID: UUID] = [:]

    public static let defaultTimeout: TimeInterval = 30
    public static let defaultHealthRetryConfiguration = RetryConfiguration(
        maxRetries: 3,
        baseDelay: 0.2,
        maxDelay: 1.0,
        jitterRange: 0.02...0.05
    )

    public func checkHealth(
        workspaceID: UUID,
        workspaceURL: URL,
        timeout: TimeInterval = defaultTimeout,
        executableURL: URL? = nil,
        retryConfiguration: RetryConfiguration = defaultHealthRetryConfiguration
    ) async -> CLIHealthStatus {
        checkTasks[workspaceID]?.cancel()

        let token = UUID()
        let task = Task { [workspaceID] in
            await performHealthCheck(
                workspaceID: workspaceID,
                workspaceURL: workspaceURL,
                timeout: timeout,
                executableURL: executableURL,
                retryConfiguration: retryConfiguration
            )
        }

        checkTasks[workspaceID] = task
        checkTaskTokens[workspaceID] = token
        let status = await withTaskCancellationHandler {
            await task.value
        } onCancel: {
            task.cancel()
        }

        if Task.isCancelled {
            if checkTaskTokens[workspaceID] == token {
                checkTasks[workspaceID] = nil
                checkTaskTokens[workspaceID] = nil
            }
            return CLIHealthStatus(
                availability: .unknown,
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        }

        if checkTaskTokens[workspaceID] == token {
            checkTasks[workspaceID] = nil
            checkTaskTokens[workspaceID] = nil
            cachedStatus[workspaceID] = status
        }
        return status
    }

    public func cachedHealth(for workspaceID: UUID) -> CLIHealthStatus? {
        cachedStatus[workspaceID]
    }

    public func invalidateCache(for workspaceID: UUID) {
        cachedStatus.removeValue(forKey: workspaceID)
        checkTasks[workspaceID]?.cancel()
        checkTasks.removeValue(forKey: workspaceID)
        checkTaskTokens.removeValue(forKey: workspaceID)
    }

    public static func isCLIUnavailableError(_ error: any Error) -> Bool {
        if let cliError = error as? RalphCLIClientError {
            switch cliError {
            case .executableNotFound, .executableNotExecutable:
                return true
            }
        }

        let description = error.localizedDescription.lowercased()
        return description.contains("executable")
            || description.contains("not found")
            || description.contains("permission denied")
    }

    private func performHealthCheck(
        workspaceID _: UUID,
        workspaceURL: URL,
        timeout: TimeInterval,
        executableURL: URL?,
        retryConfiguration: RetryConfiguration
    ) async -> CLIHealthStatus {
        guard !Task.isCancelled else {
            return CLIHealthStatus(
                availability: .unknown,
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        }

        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(
            atPath: workspaceURL.path,
            isDirectory: &isDir
        )

        guard exists && isDir.boolValue else {
            return CLIHealthStatus(
                availability: .unavailable(reason: .workspaceInaccessible),
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        }

        do {
            _ = try FileManager.default.contentsOfDirectory(
                at: workspaceURL,
                includingPropertiesForKeys: nil
            )
        } catch {
            return CLIHealthStatus(
                availability: .unavailable(reason: .permissionDenied),
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        }

        do {
            let client = if let executableURL {
                try RalphCLIClient(executableURL: executableURL)
            } else {
                try RalphCLIClient.bundled()
            }

            let healthRetryConfiguration = retryConfiguration.normalizedForHealthChecks()
            let retryHelper = RetryHelper(configuration: healthRetryConfiguration)
            try await retryHelper.execute(
                operation: { [self, client, timeout] in
                    try await self.checkVersionCommandHealth(client: client, timeout: timeout)
                },
                shouldRetry: { error in
                    guard let probeError = error as? CLIHealthProbeError else {
                        return RetryHelper.defaultShouldRetry(error)
                    }
                    return probeError.isRetryable
                }
            )

            return CLIHealthStatus(
                availability: .available,
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        } catch is CancellationError {
            return CLIHealthStatus(
                availability: .unknown,
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        } catch let probeError as CLIHealthProbeError {
            return CLIHealthStatus(
                availability: .unavailable(reason: probeError.unavailabilityReason),
                lastChecked: Date(),
                workspaceURL: workspaceURL,
                diagnostics: CLIHealthStatus.Diagnostics(
                    attempts: probeError.attempts(maxAttempts: retryConfiguration.normalizedHealthAttemptCount),
                    maxAttempts: retryConfiguration.normalizedHealthAttemptCount,
                    finalMessage: probeError.diagnosticMessage
                )
            )
        } catch RalphCLIClientError.executableNotFound {
            return CLIHealthStatus(
                availability: .unavailable(reason: .cliNotFound),
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        } catch RalphCLIClientError.executableNotExecutable {
            return CLIHealthStatus(
                availability: .unavailable(reason: .cliNotExecutable),
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        } catch {
            return CLIHealthStatus(
                availability: .unavailable(reason: .unknown(error.localizedDescription)),
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        }
    }

    private func checkVersionCommandHealth(
        client: RalphCLIClient,
        timeout: TimeInterval
    ) async throws {
        do {
            let systemInfoResult = try await runHealthCommand(
                client: client,
                arguments: ["--no-color", "machine", "system", "info"],
                timeout: timeout
            )
            guard systemInfoResult.status.code == 0 else {
                let processError = RetryableError.processError(
                    exitCode: systemInfoResult.status.code,
                    stderr: systemInfoResult.stderr
                )
                if RetryHelper.defaultShouldRetry(processError) {
                    throw CLIHealthProbeError.transientProcess(
                        exitCode: systemInfoResult.status.code,
                        stderr: systemInfoResult.stderr
                    )
                }
                throw CLIHealthProbeError.nonRetryableProcess(
                    exitCode: systemInfoResult.status.code,
                    stderr: systemInfoResult.stderr
                )
            }
        } catch is TimeoutError {
            throw CLIHealthProbeError.timeout
        } catch let probeError as CLIHealthProbeError {
            throw probeError
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            if RetryHelper.defaultShouldRetry(error) {
                throw CLIHealthProbeError.underlying(error)
            }
            throw error
        }
    }

    private func runHealthCommand(
        client: RalphCLIClient,
        arguments: [String],
        timeout: TimeInterval
    ) async throws -> RalphCLIClient.CollectedOutput {
        try await client.runAndCollect(
            arguments: arguments,
            timeoutConfiguration: TimeoutConfiguration(timeout: timeout)
        )
    }
}

private enum CLIHealthProbeError: Error, Sendable {
    case timeout
    case transientProcess(exitCode: Int32, stderr: String)
    case nonRetryableProcess(exitCode: Int32, stderr: String)
    case underlying(any Error)

    var unavailabilityReason: CLIHealthStatus.UnavailabilityReason {
        switch self {
        case .timeout:
            return .timeout
        case .transientProcess, .nonRetryableProcess, .underlying:
            return .unknown(diagnosticMessage)
        }
    }

    var diagnosticMessage: String {
        switch self {
        case .timeout:
            return "CLI health check timed out"
        case .transientProcess(let exitCode, let stderr), .nonRetryableProcess(let exitCode, let stderr):
            let trimmed = Self.trim(stderr)
            if trimmed.isEmpty {
                return "CLI health command failed with exit code \(exitCode)"
            }
            return "CLI health command failed with exit code \(exitCode): \(trimmed)"
        case .underlying(let error):
            return error.localizedDescription
        }
    }

    var isRetryable: Bool {
        switch self {
        case .timeout, .transientProcess:
            return true
        case .underlying(let error):
            return RetryHelper.defaultShouldRetry(error)
        case .nonRetryableProcess:
            return false
        }
    }

    func attempts(maxAttempts: Int) -> Int {
        switch self {
        case .timeout, .transientProcess, .underlying:
            return maxAttempts
        case .nonRetryableProcess:
            return 1
        }
    }

    private static func trim(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > 300 else { return trimmed }
        return String(trimmed.prefix(300)) + "…"
    }
}

private extension RetryConfiguration {
    var normalizedHealthAttemptCount: Int {
        max(1, maxRetries)
    }

    func normalizedForHealthChecks() -> RetryConfiguration {
        RetryConfiguration(
            maxRetries: normalizedHealthAttemptCount,
            baseDelay: baseDelay,
            maxDelay: maxDelay,
            jitterRange: jitterRange
        )
    }
}
