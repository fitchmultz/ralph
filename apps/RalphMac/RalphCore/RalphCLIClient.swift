/**
 RalphCLIClient

 Responsibilities:
 - Spawn the bundled `ralph` CLI (or an injected executable) using `Process`.
 - Stream `stdout` and `stderr` as they arrive.
 - Support cooperative cancellation by terminating the subprocess.
 - Expose a reliable exit status (code + termination reason).

 Does not handle:
 - Parsing Ralph command output into domain models (see `RalphModels.swift`).
 - Building the `ralph` binary or placing it in the app bundle (handled by the Xcode build phase).
 - Rich TTY UX (colors, progress spinners, cursor control). The client assumes non-interactive IO.

 Invariants/assumptions callers must respect:
 - `executableURL` must point to an on-disk, executable file.
 - If `currentDirectoryURL` is provided, it must exist and be a directory.
 - Output is treated as UTF-8 when converted to text; callers requiring exact bytes should use `data`.
 */

public import Foundation
import OSLog

#if canImport(Darwin)
import Darwin
#endif

public struct RalphCLIEvent: Sendable, Equatable {
    public enum Stream: String, Sendable, Equatable {
        case stdout
        case stderr
    }

    public let stream: Stream
    public let data: Data

    public init(stream: Stream, data: Data) {
        self.stream = stream
        self.data = data
    }

    /// Best-effort UTF-8 decoding for UI display.
    public var text: String {
        String(decoding: data, as: UTF8.self)
    }
}

public struct RalphCLIExitStatus: Sendable, Equatable {
    public enum TerminationReason: String, Sendable, Equatable, Codable {
        case exit
        case uncaughtSignal
    }

    public let code: Int32
    public let reason: TerminationReason

    public init(code: Int32, reason: TerminationReason) {
        self.code = code
        self.reason = reason
    }
}

public enum RalphCLIClientError: Error, Equatable {
    case executableNotFound(URL)
    case executableNotExecutable(URL)
}

// MARK: - Error Recovery Types

/// Categories of errors for tailored recovery UI
public enum ErrorCategory: String, CaseIterable, Sendable {
    /// Ralph CLI binary not found or not executable
    case cliUnavailable
    /// File permission errors
    case permissionDenied
    /// JSON parsing, data format errors
    case parseError
    /// Network-related failures
    case networkError
    /// Queue file corruption
    case queueCorrupted
    /// File locked, resource temporarily unavailable
    case resourceBusy
    /// CLI version incompatible with app
    case versionMismatch
    /// Uncategorized errors
    case unknown

    public var displayName: String {
        switch self {
        case .cliUnavailable: return "CLI Not Available"
        case .permissionDenied: return "Permission Denied"
        case .parseError: return "Data Parse Error"
        case .networkError: return "Network Error"
        case .queueCorrupted: return "Queue Corrupted"
        case .resourceBusy: return "Resource Busy"
        case .versionMismatch: return "Version Mismatch"
        case .unknown: return "Unknown Error"
        }
    }

    public var icon: String {
        switch self {
        case .cliUnavailable: return "terminal.fill"
        case .permissionDenied: return "lock.fill"
        case .parseError: return "doc.text.magnifyingglass"
        case .networkError: return "wifi.exclamationmark"
        case .queueCorrupted: return "exclamationmark.triangle.fill"
        case .resourceBusy: return "clock.badge.exclamationmark.fill"
        case .versionMismatch: return "number.circle.fill"
        case .unknown: return "questionmark.circle.fill"
        }
    }

}

/// Available recovery actions for error recovery UI
public enum RecoveryAction: String, CaseIterable, Sendable {
    /// Retry the failed operation
    case retry
    /// Run diagnostic commands
    case diagnose
    /// Copy full error to clipboard
    case copyErrorDetails
    /// Open Ralph logs
    case openLogs
    /// Dismiss the error
    case dismiss
    /// Guide user to check permissions
    case checkPermissions
    /// Guide for CLI reinstallation
    case reinstallCLI
    /// Run queue validation
    case validateQueue
}

/// Suggested recovery actions per error category
extension ErrorCategory {
    public var suggestedActions: [RecoveryAction] {
        switch self {
        case .cliUnavailable:
            return [.retry, .checkPermissions, .reinstallCLI, .openLogs, .copyErrorDetails, .dismiss]
        case .permissionDenied:
            return [.retry, .checkPermissions, .openLogs, .copyErrorDetails, .dismiss]
        case .parseError:
            return [.retry, .validateQueue, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .queueCorrupted:
            return [.validateQueue, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .resourceBusy:
            return [.retry, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        case .networkError, .versionMismatch, .unknown:
            return [.retry, .diagnose, .openLogs, .copyErrorDetails, .dismiss]
        }
    }

    public var guidanceMessage: String? {
        switch self {
        case .cliUnavailable:
            return "The Ralph CLI could not be found or is not executable. This may indicate an incomplete installation."
        case .permissionDenied:
            return "Ralph doesn't have permission to access required files. Check that you have read/write access to the workspace directory."
        case .parseError:
            return "The CLI returned data that couldn't be parsed. The queue file may be corrupted or incompatible."
        case .queueCorrupted:
            return "The queue file appears to be corrupted. Try validating or restoring from backup."
        case .resourceBusy:
            return "A required resource is temporarily unavailable. This usually resolves on retry."
        case .networkError:
            return "A network operation failed. Check your connection and try again."
        case .versionMismatch:
            return "The Ralph CLI version is incompatible with this app. Please reinstall to ensure versions match."
        case .unknown:
            return "An unexpected error occurred. Check the logs for more details."
        }
    }
}

/// Rich error type with categorization and recovery context
public struct RecoveryError: Error, Sendable {
    public let category: ErrorCategory
    public let message: String
    public let underlyingError: String?
    public let operation: String
    public let timestamp: Date
    public let suggestions: [String]
    public let workspaceURL: URL?

    public init(
        category: ErrorCategory,
        message: String,
        underlyingError: String? = nil,
        operation: String,
        suggestions: [String] = [],
        workspaceURL: URL? = nil
    ) {
        self.category = category
        self.message = message
        self.underlyingError = underlyingError
        self.operation = operation
        self.timestamp = Date()
        self.suggestions = suggestions
        self.workspaceURL = workspaceURL
    }

    /// Full error details for copying to clipboard
    public var fullErrorDetails: String {
        var lines: [String] = []
        lines.append("=== Ralph Error Report ===")
        lines.append("Timestamp: \(timestamp.formatted(.iso8601))")
        lines.append("Category: \(category.displayName)")
        lines.append("Operation: \(operation)")
        lines.append("Message: \(message)")
        if let underlying = underlyingError {
            lines.append("Details: \(underlying)")
        }
        if !suggestions.isEmpty {
            lines.append("")
            lines.append("Suggestions:")
            for suggestion in suggestions {
                lines.append("  - \(suggestion)")
            }
        }
        lines.append("==========================")
        return lines.joined(separator: "\n")
    }
}

/// Extension to classify errors from various sources
extension RecoveryError {
    public static func classify(error: any Error, operation: String, workspaceURL: URL? = nil) -> RecoveryError {
        if let retryable = error as? RetryableError {
            switch retryable {
            case .fileLocked, .resourceBusy, .resourceTemporarilyUnavailable:
                return RecoveryError(
                    category: .resourceBusy,
                    message: "Resource temporarily unavailable",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Wait a moment and retry",
                        "Check if another process is using Ralph",
                        "Close other Ralph windows that may be using the same workspace"
                    ],
                    workspaceURL: workspaceURL
                )
            case .ioTimeout:
                return RecoveryError(
                    category: .networkError,
                    message: "Operation timed out",
                    underlyingError: retryable.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Try the operation again",
                        "Check system load and available resources",
                        "If this persists, inspect logs for blocked operations"
                    ],
                    workspaceURL: workspaceURL
                )
            case .underlying(let underlying):
                return classify(error: underlying, operation: operation, workspaceURL: workspaceURL)
            case .processError(let exitCode, let stderr):
                let trimmed = stderr.trimmingCharacters(in: .whitespacesAndNewlines)
                let description = trimmed.isEmpty
                    ? "CLI command failed with exit code \(exitCode)"
                    : trimmed
                let wrappedError = NSError(
                    domain: "RalphCore.CLIProcess",
                    code: Int(exitCode),
                    userInfo: [NSLocalizedDescriptionKey: description]
                )
                let wrappedClassified = classify(error: wrappedError, operation: operation, workspaceURL: workspaceURL)
                if wrappedClassified.category == .unknown {
                    return RecoveryError(
                        category: .unknown,
                        message: description,
                        underlyingError: "CLI exit code: \(exitCode)",
                        operation: operation,
                        suggestions: [
                            "Check the logs for more details",
                            "Try the operation again",
                            "If the problem persists, consider reporting the issue"
                        ],
                        workspaceURL: workspaceURL
                    )
                }
                return wrappedClassified
            }
        }

        let errorString = error.localizedDescription.lowercased()

        // Missing queue file should be explicit and actionable, not unknown.
        if operation == "loadTasks" &&
            errorString.contains("queue") &&
            errorString.contains("no such file") {
            return RecoveryError(
                category: .queueCorrupted,
                message: "No Ralph queue file found in this workspace",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Run `ralph init --non-interactive` in this workspace",
                    "Switch to a directory that contains `.ralph/queue.jsonc`",
                    "Use Queue > Refresh after initializing"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Strong signal for parse/format failures even when localizedDescription is vague.
        if error is DecodingError || error is EncodingError {
            return RecoveryError(
                category: .parseError,
                message: "Failed to parse data",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Validate the queue file format",
                    "Check for manual edits to queue files",
                    "Run 'ralph queue validate' to check for corruption"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for CLI availability issues
        if let cliError = error as? RalphCLIClientError {
            switch cliError {
            case .executableNotFound, .executableNotExecutable:
                return RecoveryError(
                    category: .cliUnavailable,
                    message: "Ralph CLI is not available",
                    underlyingError: error.localizedDescription,
                    operation: operation,
                    suggestions: [
                        "Check that Ralph is properly installed",
                        "Verify file permissions on the CLI binary",
                        "Try reinstalling Ralph from the official source"
                    ],
                    workspaceURL: workspaceURL
                )
            }
        }

        // Check for version mismatch (from VersionValidator)
        if errorString.contains("version") &&
           (errorString.contains("too old") || errorString.contains("newer than") || errorString.contains("incompatible")) {
            return RecoveryError(
                category: .versionMismatch,
                message: "Ralph CLI version mismatch",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Reinstall Ralph to ensure CLI and app versions match",
                    "Check the bundled CLI in RalphMac.app/Contents/MacOS/ralph"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for permission errors
        if errorString.contains("permission") ||
           errorString.contains("eacces") ||
           errorString.contains("not permitted") ||
           errorString.contains("access denied") {
            return RecoveryError(
                category: .permissionDenied,
                message: "Permission denied",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Check file permissions in the workspace",
                    "Run with appropriate user privileges",
                    "Use Finder to verify read/write access to the workspace"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for parse errors
        if errorString.contains("parse") ||
           errorString.contains("decode") ||
           errorString.contains("json") ||
           errorString.contains("serialization") ||
           errorString.contains("decoding") {
            return RecoveryError(
                category: .parseError,
                message: "Failed to parse data",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Validate the queue file format",
                    "Check for manual edits to queue files",
                    "Run 'ralph queue validate' to check for corruption"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for resource busy/locked
        if errorString.contains("resource busy") ||
           errorString.contains("file locked") ||
           errorString.contains("resource temporarily unavailable") ||
           errorString.contains("eagain") ||
           errorString.contains("ewouldblock") ||
           errorString.contains("ebusy") ||
           errorString.contains("operation would block") ||
           errorString.contains("device or resource busy") {
            return RecoveryError(
                category: .resourceBusy,
                message: "Resource temporarily unavailable",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Wait a moment and retry",
                    "Check if another process is using Ralph",
                    "Close other Ralph windows that may be using the same workspace"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for queue corruption indicators
        if errorString.contains("corrupt") ||
           errorString.contains("invalid") ||
           errorString.contains("malformed") ||
           (errorString.contains("queue") && errorString.contains("error")) {
            return RecoveryError(
                category: .queueCorrupted,
                message: "Queue data appears corrupted",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Run queue validation to diagnose the issue",
                    "Restore from backup if available",
                    "Check for manual edits to queue files"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Check for network errors
        if errorString.contains("network") ||
           errorString.contains("connection") ||
           errorString.contains("timeout") ||
           errorString.contains("unreachable") ||
           errorString.contains("host not found") {
            return RecoveryError(
                category: .networkError,
                message: "Network operation failed",
                underlyingError: error.localizedDescription,
                operation: operation,
                suggestions: [
                    "Check your network connection",
                    "Verify that required services are available",
                    "Retry the operation"
                ],
                workspaceURL: workspaceURL
            )
        }

        // Default to unknown
        return RecoveryError(
            category: .unknown,
            message: error.localizedDescription,
            underlyingError: nil,
            operation: operation,
            suggestions: [
                "Check the logs for more details",
                "Try the operation again",
                "If the problem persists, consider reporting the issue"
            ],
            workspaceURL: workspaceURL
        )
    }
}

/// Tracks the state of retry attempts for UI feedback
public struct RetryState: Sendable {
    public let isRetrying: Bool
    public let attempt: Int
    public let maxAttempts: Int
    public let isExhausted: Bool

    public init(isRetrying: Bool, attempt: Int, maxAttempts: Int) {
        self.isRetrying = isRetrying
        self.attempt = attempt
        self.maxAttempts = maxAttempts
        self.isExhausted = attempt >= maxAttempts && !isRetrying
    }

    public var canRetryManually: Bool {
        isExhausted && !isRetrying
    }
}

/// Actor-isolated type for managing a CLI run.
///
/// Note: This is not @MainActor-isolated to allow use from any context.
/// All state is immutable (Sendable) and operations are executed on the caller's context.
public actor RalphCLIRun {
    public let events: AsyncStream<RalphCLIEvent>

    private let ioQueue: DispatchQueue
    private let process: Process
    private let stdoutHandle: FileHandle
    private let stderrHandle: FileHandle

    private var eventsContinuation: AsyncStream<RalphCLIEvent>.Continuation?

    // Actor-isolated mutable state (protected by actor isolation)
    private var didRequestCancel = false
    private var didFinishEvents = false
    private var didTerminateProcess = false
    private var stdoutClosed = false
    private var stderrClosed = false
    private var exitStatus: RalphCLIExitStatus?
    private var exitWaiters: [CheckedContinuation<RalphCLIExitStatus, Never>] = []

    internal init(
        ioQueue: DispatchQueue,
        process: Process,
        stdoutHandle: FileHandle,
        stderrHandle: FileHandle
    ) {
        self.ioQueue = ioQueue
        self.process = process
        self.stdoutHandle = stdoutHandle
        self.stderrHandle = stderrHandle

        var continuation: AsyncStream<RalphCLIEvent>.Continuation?
        let stream = AsyncStream<RalphCLIEvent> { cont in
            continuation = cont
        }
        self.events = stream
        self.eventsContinuation = continuation
        self.eventsContinuation?.onTermination = { @Sendable [weak self] _ in
            Task { [weak self] in
                await self?.cancel()
            }
        }

        // Set up IO handlers synchronously since init cannot await
        // The handlers use Task to bridge to actor-isolated methods
        setupIOHandlers()
    }

    deinit {
        // Best-effort cleanup. We cannot await in deinit, so we dispatch to the queue.
        // The cancel() method is actor-isolated, so we use Task to bridge.
        Task { [weak self] in
            await self?.cancel()
        }
    }

    public func cancel() {
        guard !didRequestCancel else { return }
        didRequestCancel = true

        guard process.isRunning else { return }

        process.terminate()

        #if canImport(Darwin)
        let pid = process.processIdentifier
        ioQueue.asyncAfter(deadline: .now() + 2.0) { [weak self] in
            guard let self else { return }
            Task { [weak self] in
                await self?.killIfStillRunning(pid: pid)
            }
        }
        #endif
    }

    #if canImport(Darwin)
    private func killIfStillRunning(pid: pid_t) {
        guard process.isRunning else { return }
        _ = kill(pid, SIGKILL)
    }
    #endif

    public func waitUntilExit() async -> RalphCLIExitStatus {
        if let existing = exitStatus {
            return existing
        }

        return await withCheckedContinuation { cont in
            if let existing = exitStatus {
                cont.resume(returning: existing)
                return
            }
            exitWaiters.append(cont)
        }
    }

    /// Sets up IO handlers synchronously. Must be nonisolated since it's called from init.
    /// The handlers dispatch to actor-isolated methods via Task.
    private nonisolated func setupIOHandlers() {
        stdoutHandle.readabilityHandler = { [weak self] handle in
            guard let self else { return }
            Task {
                await self.handleReadable(stream: .stdout, handle: handle)
            }
        }

        stderrHandle.readabilityHandler = { [weak self] handle in
            guard let self else { return }
            Task {
                await self.handleReadable(stream: .stderr, handle: handle)
            }
        }

        process.terminationHandler = { [weak self] process in
            guard let self else { return }
            Task {
                await self.handleTermination(process: process)
            }
        }
    }

    private func handleReadable(stream: RalphCLIEvent.Stream, handle: FileHandle) {
        let data = handle.availableData
        if data.isEmpty {
            // EOF.
            handle.readabilityHandler = nil

            switch stream {
            case .stdout:
                stdoutClosed = true
            case .stderr:
                stderrClosed = true
            }

            finishIfComplete()
            return
        }

        eventsContinuation?.yield(RalphCLIEvent(stream: stream, data: data))
    }

    private func handleTermination(process: Process) {
        didTerminateProcess = true
        RalphLogger.shared.debug("CLI process terminated with status: \(process.terminationStatus)", category: .cli)

        let reason: RalphCLIExitStatus.TerminationReason
        switch process.terminationReason {
        case .exit:
            reason = .exit
        case .uncaughtSignal:
            reason = .uncaughtSignal
        @unknown default:
            reason = .exit
        }

        let status = RalphCLIExitStatus(code: process.terminationStatus, reason: reason)

        // Stop incremental streaming and flush any remaining bytes.
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil

        let remainingStdout = stdoutHandle.readDataToEndOfFile()
        if !remainingStdout.isEmpty {
            eventsContinuation?.yield(RalphCLIEvent(stream: .stdout, data: remainingStdout))
        }

        let remainingStderr = stderrHandle.readDataToEndOfFile()
        if !remainingStderr.isEmpty {
            eventsContinuation?.yield(RalphCLIEvent(stream: .stderr, data: remainingStderr))
        }

        stdoutClosed = true
        stderrClosed = true

        if exitStatus == nil {
            exitStatus = status
            let waiters = exitWaiters
            exitWaiters.removeAll(keepingCapacity: false)
            for w in waiters {
                w.resume(returning: status)
            }
        }

        finishIfComplete()
    }

    private func finishIfComplete() {
        if !didTerminateProcess || !stdoutClosed || !stderrClosed {
            return
        }

        if didFinishEvents {
            return
        }
        didFinishEvents = true

        eventsContinuation?.finish()
        eventsContinuation = nil
    }
}

// MARK: - Timeout Configuration

public struct TimeoutConfiguration: Sendable {
    public let timeout: TimeInterval
    public let terminationGracePeriod: TimeInterval
    
    public init(
        timeout: TimeInterval = 30,
        terminationGracePeriod: TimeInterval = 2
    ) {
        self.timeout = timeout
        self.terminationGracePeriod = terminationGracePeriod
    }
    
    public static let `default` = TimeoutConfiguration()
    public static let longRunning = TimeoutConfiguration(timeout: 300) // 5 minutes
}

public struct RalphCLIClient: Sendable {
    public let executableURL: URL

    public init(executableURL: URL) throws {
        self.executableURL = executableURL

        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: executableURL.path, isDirectory: &isDir) else {
            throw RalphCLIClientError.executableNotFound(executableURL)
        }
        guard !isDir.boolValue else {
            throw RalphCLIClientError.executableNotExecutable(executableURL)
        }
        guard FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw RalphCLIClientError.executableNotExecutable(executableURL)
        }
    }

    /// Convenience for `Ralph` GUI usage.
    public static func bundled(bundle: Bundle = .main) throws -> RalphCLIClient {
        try RalphCLIClient(executableURL: RalphCLIExecutableLocator.bundledRalphExecutableURL(bundle: bundle))
    }

    /// Start a subprocess and stream output until termination.
    ///
    /// - Note: Non-zero exit codes are surfaced in `waitUntilExit()`; they do not throw.
    public func start(
        arguments: [String],
        currentDirectoryURL: URL? = nil,
        environment: [String: String] = [:]
    ) throws -> RalphCLIRun {
        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments

        if let currentDirectoryURL {
            process.currentDirectoryURL = currentDirectoryURL
        }

        if !environment.isEmpty {
            process.environment = ProcessInfo.processInfo.environment.merging(environment, uniquingKeysWith: { _, new in new })
        }

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        let ioQueue = DispatchQueue(label: "com.mitchfultz.ralph.cli-io.\(UUID().uuidString)")
        let run = RalphCLIRun(
            ioQueue: ioQueue,
            process: process,
            stdoutHandle: stdoutPipe.fileHandleForReading,
            stderrHandle: stderrPipe.fileHandleForReading
        )

        try process.run()
        let commandString = arguments.joined(separator: " ")
        RalphLogger.shared.debug("Started CLI process: \(commandString)", category: .cli)
        return run
    }

    public struct CollectedOutput: Sendable, Equatable {
        public let status: RalphCLIExitStatus
        public let stdout: String
        public let stderr: String

        public init(status: RalphCLIExitStatus, stdout: String, stderr: String) {
            self.status = status
            self.stdout = stdout
            self.stderr = stderr
        }
    }

    /// Run a subprocess and collect stdout/stderr into strings.
    ///
    /// This is intended for small machine-readable outputs (like JSON) where streaming is not
    /// required. For long-running commands, prefer `start(...)` and consume `events`.
    ///
    /// - Note: Non-zero exit codes are returned in `CollectedOutput.status`; they do not throw.
    /// - Parameters:
    ///   - arguments: Command-line arguments to pass to the executable
    ///   - currentDirectoryURL: Working directory for the subprocess
    ///   - environment: Additional environment variables
    ///   - maxOutputSize: Optional maximum size in characters before truncation (default: nil = unlimited)
    ///   - timeoutConfiguration: Timeout configuration for the operation (default: 30s)
    public func runAndCollect(
        arguments: [String],
        currentDirectoryURL: URL? = nil,
        environment: [String: String] = [:],
        maxOutputSize: Int? = nil,
        timeoutConfiguration: TimeoutConfiguration = .default
    ) async throws -> CollectedOutput {
        let run = try start(
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            environment: environment
        )

        // Use withTimeout to enforce timeout
        return try await withTimeout(
            configuration: timeoutConfiguration,
            run: run
        ) {
            var stdout = ""
            var stderr = ""
            var isTruncated = false

            for await event in run.events {
                // Check if we've exceeded the max size
                if let maxSize = maxOutputSize, !isTruncated {
                    let currentSize = stdout.count + stderr.count
                    if currentSize >= maxSize {
                        isTruncated = true
                        // Continue consuming events but don't accumulate more
                        continue
                    }
                }

                switch event.stream {
                case .stdout:
                    stdout.append(event.text)
                case .stderr:
                    stderr.append(event.text)
                }
            }

            let status = await run.waitUntilExit()

            // If truncated, add indicator to stderr
            if isTruncated {
                stderr = "\n[warning: output exceeded maximum size and was truncated]\n" + stderr
            }

            return CollectedOutput(status: status, stdout: stdout, stderr: stderr)
        }
    }
    
    // MARK: - Timeout Support
    
    private func withTimeout<T: Sendable>(
        configuration: TimeoutConfiguration,
        run: RalphCLIRun,
        operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            // Add the operation task
            group.addTask {
                try await operation()
            }
            
            // Add timeout task
            group.addTask {
                try await Task.sleep(nanoseconds: UInt64(configuration.timeout * 1_000_000_000))
                // Cancel the CLI run before throwing
                await run.cancel()
                try await Task.sleep(nanoseconds: UInt64(configuration.terminationGracePeriod * 1_000_000_000))
                throw TimeoutError()
            }
            
            // Return first to complete, cancel the other
            guard let result = try await group.next() else {
                group.cancelAll()
                throw CancellationError()
            }
            group.cancelAll()
            return result
        }
    }
}

// MARK: - Retry Support

public extension RalphCLIClient {
    /// Run and collect with automatic retry for transient failures
    ///
    /// - Parameters:
    ///   - arguments: Command-line arguments
    ///   - currentDirectoryURL: Working directory for the subprocess
    ///   - environment: Additional environment variables
    ///   - maxOutputSize: Optional maximum output size
    ///   - retryConfiguration: Retry behavior configuration
    ///   - onRetry: Optional callback invoked on retry attempts
    /// - Returns: Collected output from the CLI
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
                // Also check stderr for retryable patterns in process errors
                if error is RalphCLIClientError {
                    // Client errors (executable not found, etc.) are not retryable
                    return false
                }
                return RetryHelper.defaultShouldRetry(error)
            },
            onProgress: onRetry
        )
    }
}

/// Error thrown when an operation times out
public struct TimeoutError: Error, Sendable {}

// MARK: - CLI Health Checking

/// Represents the health status of the CLI for a workspace
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

/// Actor that manages CLI health checking
public actor CLIHealthChecker {
    private var cachedStatus: [UUID: CLIHealthStatus] = [:]
    private var checkTasks: [UUID: Task<CLIHealthStatus, Never>] = [:]
    
    /// Default timeout for health checks (30 seconds)
    public static let defaultTimeout: TimeInterval = 30
    
    /// Perform a health check for a workspace
    /// - Parameters:
    ///   - workspaceID: The unique identifier for the workspace
    ///   - workspaceURL: The working directory URL of the workspace
    ///   - timeout: Maximum time to wait for check (default: 30s)
    /// - Returns: The health status
    public func checkHealth(
        workspaceID: UUID,
        workspaceURL: URL,
        timeout: TimeInterval = defaultTimeout,
        executableURL: URL? = nil
    ) async -> CLIHealthStatus {
        // Cancel any in-flight check for this workspace
        checkTasks[workspaceID]?.cancel()
        
        let task = Task {
            return await performHealthCheck(
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
    
    /// Get cached health status without performing a new check
    public func cachedHealth(for workspaceID: UUID) -> CLIHealthStatus? {
        cachedStatus[workspaceID]
    }
    
    /// Invalidate cached status for a workspace
    public func invalidateCache(for workspaceID: UUID) {
        cachedStatus.removeValue(forKey: workspaceID)
    }
    
    /// Check if a specific error represents a CLI unavailability issue
    public static func isCLIUnavailableError(_ error: any Error) -> Bool {
        if let cliError = error as? RalphCLIClientError {
            switch cliError {
            case .executableNotFound, .executableNotExecutable:
                return true
            }
        }
        
        let description = error.localizedDescription.lowercased()
        return description.contains("executable") ||
               description.contains("not found") ||
               description.contains("permission denied")
    }
    
    // MARK: - Private
    
    private func performHealthCheck(
        workspaceID: UUID,
        workspaceURL: URL,
        timeout: TimeInterval,
        executableURL: URL?
    ) async -> CLIHealthStatus {
        // Check 1: Workspace directory exists and is accessible
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
        
        // Check 2: Can list directory contents (permission check)
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
        
        // Check 3: CLI is available and executable (with timeout)
        do {
            let client: RalphCLIClient
            if let executableURL {
                client = try RalphCLIClient(executableURL: executableURL)
            } else {
                client = try RalphCLIClient.bundled()
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
        let dashVersionResult = try await runHealthCommand(
            client: client,
            arguments: ["--version"],
            timeout: timeout
        )
        if dashVersionResult.status.code == 0 {
            return true
        }

        let subcommandResult = try await runHealthCommand(
            client: client,
            arguments: ["version"],
            timeout: timeout
        )
        return subcommandResult.status.code == 0
    }

    private func runHealthCommand(
        client: RalphCLIClient,
        arguments: [String],
        timeout: TimeInterval
    ) async throws -> RalphCLIClient.CollectedOutput {
        try await withThrowingTaskGroup(of: RalphCLIClient.CollectedOutput.self) { group in
            group.addTask {
                try await client.runAndCollect(arguments: arguments)
            }

            group.addTask {
                try await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                throw TimeoutError()
            }

            guard let result = try await group.next() else {
                group.cancelAll()
                throw CancellationError()
            }
            group.cancelAll()
            return result
        }
    }
}

/// Extension to check if a CollectedOutput indicates a retryable failure
public extension RalphCLIClient.CollectedOutput {
    /// Check if this output represents a retryable failure
    var isRetryableFailure: Bool {
        guard status.code != 0 else { return false }

        // Check stderr for retryable patterns
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

    /// Convert to an error if this is a failure
    func toError() -> any Error {
        return RetryableError.processError(exitCode: status.code, stderr: stderr)
    }
}
