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

/// Actor-isolated type for managing a CLI run.
/// All mutable state is protected by actor isolation, eliminating data race risks.
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
            Task {
                let isRunning = await self.process.isRunning
                guard isRunning else { return }
                _ = kill(pid, SIGKILL)
            }
        }
        #endif
    }

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
    public func runAndCollect(
        arguments: [String],
        currentDirectoryURL: URL? = nil,
        environment: [String: String] = [:],
        maxOutputSize: Int? = nil
    ) async throws -> CollectedOutput {
        let run = try start(
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            environment: environment
        )

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
                if let processError = error as? RalphCLIClientError {
                    // Client errors (executable not found, etc.) are not retryable
                    return false
                }
                return RetryHelper.defaultShouldRetry(error)
            },
            onProgress: onRetry
        )
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
