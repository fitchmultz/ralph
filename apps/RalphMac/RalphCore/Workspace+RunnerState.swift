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

    func prepareForNewRun(preservingConsole: Bool = false) {
        if preservingConsole {
            if !outputBuffer.content.hasSuffix("\n"), !outputBuffer.content.isEmpty {
                outputBuffer.append("\n")
            }
            output = outputBuffer.content
        } else {
            output = ""
            outputBuffer.clear()
            attributedOutput = []
            streamProcessor.reset()
        }
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true
        executionStartTime = Date()
        currentPhase = nil
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
        await runnerController.loadRunnerConfiguration(retryConfiguration: retryConfiguration)
    }

    func run(arguments: [String], preservingConsole: Bool = false) {
        runnerController.run(arguments: arguments, preservingConsole: preservingConsole)
    }

    func cancel() {
        runnerController.cancel()
    }

    func runNextTask(
        taskIDOverride: String? = nil,
        forceDirtyRepo: Bool = false,
        preservingConsole: Bool = false
    ) {
        runnerController.runNextTask(
            taskIDOverride: taskIDOverride,
            forceDirtyRepo: forceDirtyRepo,
            preservingConsole: preservingConsole
        )
    }

    func startLoop(forceDirtyRepo: Bool? = nil) {
        runnerController.startLoop(forceDirtyRepo: forceDirtyRepo)
    }

    func stopLoop() {
        runnerController.stopLoop()
    }
}

extension Workspace {
    func resetExecutionState() {
        runState.currentPhase = nil
        runState.executionStartTime = nil
        runState.currentTaskID = nil
        resetStreamProcessingState()
    }

    func addToHistory(_ record: ExecutionRecord) {
        runState.executionHistory.insert(record, at: 0)
        if runState.executionHistory.count > 50 {
            runState.executionHistory = Array(runState.executionHistory.prefix(50))
        }
    }
}
