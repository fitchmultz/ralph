//! Workspace+RunnerState
//!
//! Responsibilities:
//! - Start, cancel, and loop Ralph CLI executions for a workspace.
//! - Track per-workspace runner lifecycle fields such as active run, phase, and history.
//! - Resolve the next runnable task, runner configuration, and execution phase state.
//! - Apply incremental stream parsing to runner output instead of reparsing the full buffer.
//!
//! Does not handle:
//! - Queue file decoding or file-watcher orchestration.
//! - Task filtering, grouping, or other presentation work.
//! - Task mutation and task creation flows.
//!
//! Invariants/assumptions callers must respect:
//! - Runner state remains window/workspace scoped and must not leak across workspaces.
//! - Only one active run may execute per workspace at a time.
//! - Cancellation must target the active subprocess owned by this workspace.
//! - Runner configuration is resolved by the CLI itself, not reconstructed in-app.

public import Foundation
public import Combine
public import SwiftUI

@MainActor
public final class WorkspaceRunState: ObservableObject {
    @Published public var output = ""
    @Published public var isRunning = false
    @Published public var lastExitStatus: RalphCLIExitStatus?
    @Published public var errorMessage: String?
    @Published public var currentTaskID: String?
    @Published public var currentPhase: Workspace.ExecutionPhase?
    @Published public var executionStartTime: Date?
    @Published public var isLoopMode = false
    @Published public var stopAfterCurrent = false
    @Published public var executionHistory: [Workspace.ExecutionRecord] = []
    @Published public var currentRunnerConfig: Workspace.RunnerConfig?
    @Published public var runnerConfigLoading = false
    @Published public var runnerConfigErrorMessage: String?
    @Published public var runControlSelectedTaskID: String?
    @Published public var runControlForceDirtyRepo = false
    @Published public var attributedOutput: [Workspace.ANSISegment] = []
    @Published public var outputBuffer: ConsoleOutputBuffer
    @Published public var maxANSISegments = 1_000 {
        didSet {
            if maxANSISegments != oldValue {
                attributedOutput = streamProcessor.displaySegments(maxSegments: maxANSISegments)
            }
        }
    }

    let streamProcessor = WorkspaceStreamProcessor()

    public init(outputBuffer: ConsoleOutputBuffer) {
        self.outputBuffer = outputBuffer
    }

    func prepareForNewRun() {
        output = ""
        outputBuffer.clear()
        attributedOutput = []
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true
        executionStartTime = Date()
        currentPhase = nil
        streamProcessor.reset()
    }
}

public extension Workspace {
    enum ExecutionPhase: Int, CaseIterable, Sendable {
        case plan = 1
        case implement = 2
        case review = 3

        public var displayName: String {
            switch self {
            case .plan: return "Plan"
            case .implement: return "Implement"
            case .review: return "Review"
            }
        }

        public var icon: String {
            switch self {
            case .plan: return "doc.text.magnifyingglass"
            case .implement: return "hammer.fill"
            case .review: return "checkmark.shield.fill"
            }
        }

        public var progressFraction: Double {
            switch self {
            case .plan: return 0.17
            case .implement: return 0.5
            case .review: return 0.83
            }
        }

        public var color: SwiftUI.Color {
            switch self {
            case .plan: return .blue
            case .implement: return .orange
            case .review: return .green
            }
        }
    }

    struct ExecutionRecord: Identifiable, Codable, Sendable {
        public let id: UUID
        public let taskID: String?
        public let startTime: Date
        public let endTime: Date?
        public let exitCode: Int?
        public let wasCancelled: Bool

        public init(id: UUID = UUID(), taskID: String?, startTime: Date, endTime: Date?, exitCode: Int?, wasCancelled: Bool) {
            self.id = id
            self.taskID = taskID
            self.startTime = startTime
            self.endTime = endTime
            self.exitCode = exitCode
            self.wasCancelled = wasCancelled
        }

        public var duration: TimeInterval? {
            guard let endTime else { return nil }
            return endTime.timeIntervalSince(startTime)
        }

        public var success: Bool {
            exitCode == 0 && !wasCancelled
        }
    }

    struct RunnerConfig: Sendable {
        public let model: String?
        public let phases: Int?
        public let maxIterations: Int?

        public init(model: String? = nil, phases: Int? = nil, maxIterations: Int? = nil) {
            self.model = model
            self.phases = phases
            self.maxIterations = maxIterations
        }
    }
}

public extension Workspace {
    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        runnerConfigLoading = true
        runnerConfigErrorMessage = nil

        guard let client else {
            currentRunnerConfig = nil
            runnerConfigErrorMessage = "CLI client not available."
            runnerConfigLoading = false
            return
        }

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "config", "show", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            let data = Data(collected.stdout.utf8)
            let decoded = try JSONDecoder().decode(ResolvedRunnerConfigDocument.self, from: data)
            currentRunnerConfig = RunnerConfig(
                model: decoded.agent?.model,
                phases: decoded.agent?.phases,
                maxIterations: decoded.agent?.iterations
            )
            runnerConfigErrorMessage = nil
        } catch {
            currentRunnerConfig = nil
            runnerConfigErrorMessage = "Failed to load resolved runner configuration."
            RalphLogger.shared.error(
                "Failed to load runner configuration: \(error)",
                category: .workspace
            )
        }

        runnerConfigLoading = false
    }

    func run(arguments: [String]) {
        guard let client else {
            errorMessage = "CLI client not available."
            return
        }
        guard !isRunning else { return }

        runState.prepareForNewRun()
        cancelRequested = false

        Task { @MainActor [weak self] in
            guard let self else { return }

            do {
                let run = try client.start(
                    arguments: arguments,
                    currentDirectoryURL: workingDirectoryURL
                )
                activeRun = run

                for await event in run.events {
                    outputBuffer.append(event.text)
                    output = outputBuffer.content
                    consumeStreamTextChunk(event.text)
                }

                let status = await run.waitUntilExit()

                lastExitStatus = status
                isRunning = false

                if let startTime = executionStartTime {
                    let record = ExecutionRecord(
                        id: UUID(),
                        taskID: currentTaskID,
                        startTime: startTime,
                        endTime: Date(),
                        exitCode: cancelRequested ? nil : Int(status.code),
                        wasCancelled: cancelRequested
                    )
                    addToHistory(record)
                }

                if isLoopMode && !stopAfterCurrent && !cancelRequested && status.code == 0 {
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                    if isLoopMode && !stopAfterCurrent && !cancelRequested {
                        activeRun = nil
                        cancelRequested = false
                        runNextTask(forceDirtyRepo: runControlForceDirtyRepo)
                        return
                    }
                }

                if status.code != 0 {
                    isLoopMode = false
                }

                activeRun = nil
                cancelRequested = false
                resetExecutionState()
            } catch {
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "run",
                    workspaceURL: workingDirectoryURL
                )
                errorMessage = recoveryError.message
                lastRecoveryError = recoveryError
                showErrorRecovery = true
                isRunning = false
                activeRun = nil
                cancelRequested = false
                resetExecutionState()
            }
        }
    }

    func cancel() {
        guard isRunning else {
            isLoopMode = false
            stopAfterCurrent = true
            return
        }

        cancelRequested = true
        isLoopMode = false
        stopAfterCurrent = true

        guard let run = activeRun else { return }
        Task {
            await run.cancel()
        }
    }

    func runNextTask(taskIDOverride: String? = nil, forceDirtyRepo: Bool = false) {
        guard !isRunning else { return }

        Task { @MainActor [weak self] in
            guard let self else { return }

            resetExecutionState()

            let requestedTaskID = taskIDOverride?.trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedTaskID = requestedTaskID.flatMap { $0.isEmpty ? nil : $0 }
            let resolvedTaskID: String?
            if let selectedTaskID {
                resolvedTaskID = selectedTaskID
            } else {
                resolvedTaskID = await resolveNextRunnableTaskID()
            }
            currentTaskID = resolvedTaskID

            var arguments = ["--no-color", "run", "one"]
            if forceDirtyRepo {
                arguments.append("--force")
            }
            if let resolvedTaskID {
                arguments.append(contentsOf: ["--id", resolvedTaskID])
            }

            run(arguments: arguments)
        }
    }

    func startLoop(forceDirtyRepo: Bool? = nil) {
        isLoopMode = true
        stopAfterCurrent = false
        let shouldForceDirtyRepo = forceDirtyRepo ?? runControlForceDirtyRepo
        runNextTask(forceDirtyRepo: shouldForceDirtyRepo)
    }

    func stopLoop() {
        isLoopMode = false
        stopAfterCurrent = true
    }
}

private extension Workspace {
    struct ResolvedRunnerConfigDocument: Decodable, Sendable {
        let agent: AgentConfig?

        struct AgentConfig: Decodable, Sendable {
            let model: String?
            let phases: Int?
            let iterations: Int?
        }
    }

    static func extractTaskID(from text: String) -> String? {
        for token in text.split(whereSeparator: { $0.isWhitespace || $0 == "(" || $0 == ")" || $0 == ":" || $0 == "," }) {
            let candidate = String(token)
            if candidate.hasPrefix("RQ-") {
                return candidate
            }
        }
        return nil
    }

    func resolveNextRunnableTaskID() async -> String? {
        guard let client else { return nextTask()?.id }

        do {
            let dryRun = try await client.runAndCollect(
                arguments: ["--no-color", "run", "one", "--dry-run", "--non-interactive"],
                currentDirectoryURL: workingDirectoryURL
            )
            let combined = dryRun.stdout + "\n" + dryRun.stderr
            if let id = Self.extractTaskID(from: combined) {
                return id
            }
        } catch {
            RalphLogger.shared.debug("Failed to resolve runnable task ID: \(error)", category: .workspace)
        }

        return nextTask()?.id
    }

    func resetExecutionState() {
        currentPhase = nil
        executionStartTime = nil
        currentTaskID = nil
        resetStreamProcessingState()
    }

    func addToHistory(_ record: ExecutionRecord) {
        executionHistory.insert(record, at: 0)
        if executionHistory.count > 50 {
            executionHistory = Array(executionHistory.prefix(50))
        }
    }
}
