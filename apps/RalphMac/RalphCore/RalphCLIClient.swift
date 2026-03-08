/**
 RalphCLIClient

 Responsibilities:
 - Spawn the bundled `ralph` CLI (or an injected executable) using `Process`.
 - Expose the core streaming and collected-output APIs shared by the app.
 - Enforce subprocess timeout behavior for bounded machine-readable commands.

 Does not handle:
 - Recovery classification and retry UI state.
 - Health-check orchestration.
 - Detailed process-lifecycle ownership beyond the shared `RalphCLIRun` actor.

 Invariants/assumptions callers must respect:
 - `executableURL` must point to an on-disk, executable file.
 - If `currentDirectoryURL` is provided, it must exist and be a directory.
 - Output is treated as UTF-8 when converted to text; callers requiring exact bytes should use `data`.
 */

public import Foundation

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
    public static let longRunning = TimeoutConfiguration(timeout: 300)
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

    public static func bundled(bundle: Bundle = .main) throws -> RalphCLIClient {
        try RalphCLIClient(executableURL: RalphCLIExecutableLocator.bundledRalphExecutableURL(bundle: bundle))
    }

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

        return try await withTimeout(
            configuration: timeoutConfiguration,
            run: run
        ) {
            var stdout = ""
            var stderr = ""
            var isTruncated = false

            for await event in run.events {
                if let maxSize = maxOutputSize, !isTruncated {
                    let currentSize = stdout.count + stderr.count
                    if currentSize >= maxSize {
                        isTruncated = true
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

            if isTruncated {
                stderr = "\n[warning: output exceeded maximum size and was truncated]\n" + stderr
            }

            return CollectedOutput(status: status, stdout: stdout, stderr: stderr)
        }
    }

    private func withTimeout<T: Sendable>(
        configuration: TimeoutConfiguration,
        run: RalphCLIRun,
        operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask {
                try await operation()
            }

            group.addTask {
                try await Task.sleep(nanoseconds: UInt64(configuration.timeout * 1_000_000_000))
                await run.cancel()
                try await Task.sleep(nanoseconds: UInt64(configuration.terminationGracePeriod * 1_000_000_000))
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
