//! Workspace+RunnerState
//!
//! Responsibilities:
//! - Start, cancel, and loop Ralph CLI executions for a workspace.
//! - Track per-workspace runner lifecycle fields such as active run, phase, history, and resume state.
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
//! - Only one active run may execute per workspace.
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
    @Published public var parallelStatus: Workspace.ParallelStatus?
    @Published public var parallelStatusLoading = false
    @Published public var parallelStatusErrorMessage: String?
    @Published public var runControlSelectedTaskID: String?
    @Published public var runControlForceDirtyRepo = false
    @Published public var resumeState: Workspace.ResumeState?
    @Published public var blockingState: Workspace.BlockingState?
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

    var hasMeaningfulParallelStatus: Bool {
        parallelStatus?.isMeaningful == true
    }

    public var shouldShowRunControlParallelStatus: Bool {
        parallelStatusLoading
            || parallelStatusErrorMessage != nil
            || currentRunnerConfig?.safety?.parallelConfigured == true
            || hasMeaningfulParallelStatus
    }

    public var runControlDisplayBlockingState: Workspace.BlockingState? {
        guard let blockingState else {
            return nil
        }
        if parallelStatus?.blocking == blockingState {
            return nil
        }
        guard case let .runnerRecovery(scope, reason, taskID) = blockingState.reason,
              let resumeState,
              resumeState.scope == scope,
              resumeState.reason == reason,
              resumeState.taskID == taskID else {
            return blockingState
        }
        return nil
    }

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
        resumeState = nil
        blockingState = nil
    }

    func setBlockingState(_ state: Workspace.BlockingState?) {
        blockingState = state
    }

    func clearParallelStatus() {
        parallelStatus = nil
        parallelStatusLoading = false
        parallelStatusErrorMessage = nil
    }
}

public extension Workspace {
    struct ParallelStatus: Sendable, Equatable {
        public let headline: String
        public let detail: String
        public let blocking: BlockingState?
        public let nextSteps: [ParallelStatusStep]
        public let snapshot: ParallelStatusSnapshot

        public var isMeaningful: Bool {
            blocking != nil || !snapshot.workers.isEmpty
        }
    }

    enum BlockingStatus: String, Codable, Sendable {
        case waiting
        case blocked
        case stalled
    }

    enum BlockingReason: Equatable, Sendable {
        case idle(includeDraft: Bool)
        case dependencyBlocked(blockedTasks: Int)
        case scheduleBlocked(blockedTasks: Int, nextRunnableAt: String?, secondsUntilNextRunnable: Int?)
        case lockBlocked(lockPath: String?, owner: String?, ownerPID: Int?)
        case ciBlocked(pattern: String?, exitCode: Int?)
        case runnerRecovery(scope: String, reason: String, taskID: String?)
        case operatorRecovery(scope: String, reason: String, suggestedCommand: String?)
        case mixedQueue(dependencyBlocked: Int, scheduleBlocked: Int, statusFiltered: Int)
    }

    struct BlockingState: Equatable, Sendable {
        public let status: BlockingStatus
        public let reason: BlockingReason
        public let taskID: String?
        public let message: String
        public let detail: String
        /// RFC3339 UTC instant when this blocking snapshot was produced (CLI/machine contract).
        public let observedAt: String?

        public init(
            status: BlockingStatus,
            reason: BlockingReason,
            taskID: String?,
            message: String,
            detail: String,
            observedAt: String? = nil
        ) {
            self.status = status
            self.reason = reason
            self.taskID = taskID
            self.message = message
            self.detail = detail
            self.observedAt = observedAt
        }
    }

    struct ResumeState: Equatable, Sendable {
        public enum Status: String, Codable, Sendable {
            case resumingSameSession = "resuming_same_session"
            case fallingBackToFreshInvocation = "falling_back_to_fresh_invocation"
            case refusingToResume = "refusing_to_resume"
        }

        public let status: Status
        public let scope: String
        public let reason: String
        public let taskID: String?
        public let message: String
        public let detail: String

        public init(
            status: Status,
            scope: String,
            reason: String,
            taskID: String?,
            message: String,
            detail: String
        ) {
            self.status = status
            self.scope = scope
            self.reason = reason
            self.taskID = taskID
            self.message = message
            self.detail = detail
        }
    }

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
        public let safety: RunnerSafetySummary?

        public init(
            model: String? = nil,
            phases: Int? = nil,
            maxIterations: Int? = nil,
            safety: RunnerSafetySummary? = nil
        ) {
            self.model = model
            self.phases = phases
            self.maxIterations = maxIterations
            self.safety = safety
        }
    }

    struct RunnerSafetySummary: Sendable, Equatable {
        public let repoTrusted: Bool
        public let dirtyRepo: Bool
        public let gitPublishMode: String
        public let approvalMode: String?
        public let ciGateEnabled: Bool
        public let gitRevertMode: String
        public let parallelConfigured: Bool
        public let executionInteractivity: String
        public let interactiveApprovalSupported: Bool

        public init(
            repoTrusted: Bool,
            dirtyRepo: Bool,
            gitPublishMode: String,
            approvalMode: String?,
            ciGateEnabled: Bool,
            gitRevertMode: String,
            parallelConfigured: Bool,
            executionInteractivity: String,
            interactiveApprovalSupported: Bool
        ) {
            self.repoTrusted = repoTrusted
            self.dirtyRepo = dirtyRepo
            self.gitPublishMode = gitPublishMode
            self.approvalMode = approvalMode
            self.ciGateEnabled = ciGateEnabled
            self.gitRevertMode = gitRevertMode
            self.parallelConfigured = parallelConfigured
            self.executionInteractivity = executionInteractivity
            self.interactiveApprovalSupported = interactiveApprovalSupported
        }
    }
}

public extension Workspace {
    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        await runnerController.loadRunnerConfiguration(retryConfiguration: retryConfiguration)
    }

    func loadParallelStatus(retryConfiguration: RetryConfiguration = .minimal) async {
        await runnerController.loadParallelStatus(retryConfiguration: retryConfiguration)
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
        runState.blockingState = nil
        resetStreamProcessingState()
    }

    func addToHistory(_ record: ExecutionRecord) {
        runState.executionHistory.insert(record, at: 0)
        if runState.executionHistory.count > 50 {
            runState.executionHistory = Array(runState.executionHistory.prefix(50))
        }
    }
}
