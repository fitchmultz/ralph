/**
 RalphCLIHealthChecker

 Responsibilities:
 - Probe whether the Ralph CLI is available for a workspace.
 - Cache health results and coalesce duplicate in-flight checks.
 - Translate health failures into availability reasons for offline-mode UI.

 Does not handle:
 - Process retry loops outside health probes.
 - Workspace error recovery presentation.
 - Parsing command output beyond simple success/failure checks.

 Invariants/assumptions callers must respect:
 - Health checks may cancel and replace an older in-flight check for the same workspace.
 - Health probing uses the machine system-info contract.
 - Cached status is keyed by `Workspace.id`.
 */

public import Foundation

public struct CLIHealthStatus: Sendable, Equatable {
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

    public var isAvailable: Bool {
        if case .available = availability { return true }
        return false
    }
}

public actor CLIHealthChecker {
    private var cachedStatus: [UUID: CLIHealthStatus] = [:]
    private var checkTasks: [UUID: Task<CLIHealthStatus, Never>] = [:]

    public static let defaultTimeout: TimeInterval = 30

    public func checkHealth(
        workspaceID: UUID,
        workspaceURL: URL,
        timeout: TimeInterval = defaultTimeout,
        executableURL: URL? = nil
    ) async -> CLIHealthStatus {
        checkTasks[workspaceID]?.cancel()

        let task = Task {
            await performHealthCheck(
                workspaceID: workspaceID,
                workspaceURL: workspaceURL,
                timeout: timeout,
                executableURL: executableURL
            )
        }

        checkTasks[workspaceID] = task
        let status = await task.value
        checkTasks[workspaceID] = nil
        cachedStatus[workspaceID] = status
        return status
    }

    public func cachedHealth(for workspaceID: UUID) -> CLIHealthStatus? {
        cachedStatus[workspaceID]
    }

    public func invalidateCache(for workspaceID: UUID) {
        cachedStatus.removeValue(forKey: workspaceID)
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
        executableURL: URL?
    ) async -> CLIHealthStatus {
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

            let supportsVersion = try await checkVersionCommandHealth(client: client, timeout: timeout)
            guard supportsVersion else {
                return CLIHealthStatus(
                    availability: .unavailable(reason: .cliNotExecutable),
                    lastChecked: Date(),
                    workspaceURL: workspaceURL
                )
            }

            return CLIHealthStatus(
                availability: .available,
                lastChecked: Date(),
                workspaceURL: workspaceURL
            )
        } catch is TimeoutError {
            return CLIHealthStatus(
                availability: .unavailable(reason: .timeout),
                lastChecked: Date(),
                workspaceURL: workspaceURL
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
    ) async throws -> Bool {
        let systemInfoResult = try await runHealthCommand(
            client: client,
            arguments: ["--no-color", "machine", "system", "info"],
            timeout: timeout
        )
        return systemInfoResult.status.code == 0
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
