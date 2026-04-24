//! Workspace+RunnerState
//!
//! Purpose:
//! - Start, cancel, and loop Ralph CLI executions for a workspace.
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
//!
//! Usage:
//! - Used by the RalphMac app or RalphCore tests through its owning feature surface.
//! Invariants/assumptions callers must respect:
//! - Runner state remains window/workspace scoped and must not leak across workspaces.
//! - Only one active run may execute per workspace.
//! - Cancellation must target the active subprocess owned by this workspace.
//! - Runner configuration is resolved by the CLI itself, not reconstructed in-app.
//!
public import Foundation
public import Combine
public import SwiftUI

@MainActor
public final class WorkspaceRunState: ObservableObject {
    static let consoleRenderRefreshIntervalNanoseconds: UInt64 = 50_000_000

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
    @Published public var parallelStatusLoading = false
    @Published public var parallelStatusErrorMessage: String?
    @Published public var runControlSelectedTaskID: String?
    @Published public var runControlForceDirtyRepo = false
    @Published public var runControlParallelWorkersOverride: Int?
    @Published public var resumeState: Workspace.ResumeState? {
        didSet { refreshOperatorState() }
    }
    @Published public private(set) var blockingState: Workspace.BlockingState?
    @Published public var parallelStatus: Workspace.ParallelStatus? {
        didSet { refreshOperatorState() }
    }
    @Published public private(set) var runControlOperatorState: Workspace.RunControlOperatorState?
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
    private var liveBlockingState: Workspace.BlockingState?
    private var queueBlockingState: Workspace.BlockingState?
    private var pendingConsoleRenderRefreshTask: Task<Void, Never>?
    private var pendingConsoleText = ""

    var hasMeaningfulParallelStatus: Bool {
        parallelStatus?.isMeaningful == true
    }

    public var shouldShowRunControlParallelStatus: Bool {
        parallelStatusLoading
            || parallelStatusErrorMessage != nil
            || runControlParallelWorkersOverride != nil
            || currentRunnerConfig?.safety?.parallelConfigured == true
            || hasMeaningfulParallelStatus
    }

    public init(outputBuffer: ConsoleOutputBuffer) {
        self.outputBuffer = outputBuffer
    }

    func prepareForNewRun(preservingConsole: Bool = false) {
        cancelPendingConsoleRenderRefresh()
        pendingConsoleText.removeAll(keepingCapacity: false)
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
        clearQueueBlockingState()
        clearLiveBlockingState()
    }

    func setLiveBlockingState(_ state: Workspace.BlockingState?) {
        liveBlockingState = state
        refreshOperatorState()
    }

    func clearLiveBlockingState() {
        setLiveBlockingState(nil)
    }

    func setQueueBlockingState(_ state: Workspace.BlockingState?) {
        queueBlockingState = state
        refreshOperatorState()
    }

    func clearQueueBlockingState() {
        setQueueBlockingState(nil)
    }

    func clearRunControlOperatorState() {
        liveBlockingState = nil
        queueBlockingState = nil
        resumeState = nil
        blockingState = nil
        runControlOperatorState = nil
    }

    func clearParallelStatus() {
        parallelStatus = nil
        parallelStatusLoading = false
        parallelStatusErrorMessage = nil
    }

    func refreshOperatorStateForDisplay() {
        refreshOperatorState()
    }

    func scheduleConsoleRenderRefresh() {
        guard pendingConsoleRenderRefreshTask == nil else { return }
        pendingConsoleRenderRefreshTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(nanoseconds: Self.consoleRenderRefreshIntervalNanoseconds)
            } catch {
                return
            }
            guard let self, !Task.isCancelled else { return }
            pendingConsoleRenderRefreshTask = nil
            publishConsoleRenderState()
        }
    }

    func flushConsoleRenderState() {
        cancelPendingConsoleRenderRefresh()
        publishConsoleRenderState()
    }

    func ingestConsoleText(_ text: String) {
        pendingConsoleText.append(text)
    }

    func cancelPendingConsoleRenderRefresh() {
        pendingConsoleRenderRefreshTask?.cancel()
        pendingConsoleRenderRefreshTask = nil
    }

    private func publishConsoleRenderState() {
        if !pendingConsoleText.isEmpty {
            outputBuffer.append(pendingConsoleText)
            pendingConsoleText.removeAll(keepingCapacity: true)
        }
        output = outputBuffer.content
        attributedOutput = streamProcessor.displaySegments(maxSegments: maxANSISegments)
    }

    private func refreshOperatorState() {
        let operatorState = Workspace.RunControlOperatorState.build(
            liveBlockingState: liveBlockingState,
            parallelStatus: parallelStatus,
            resumeState: resumeState,
            queueBlockingState: queueBlockingState
        )
        runControlOperatorState = operatorState
        blockingState = operatorState?.blockingState
    }
}

public extension Workspace {
    struct RunControlOperatorStep: Equatable, Sendable {
        public let title: String
        public let detail: String

        public init(title: String, detail: String) {
            self.title = title
            self.detail = detail
        }
    }

    struct RunControlOperatorState: Equatable, Sendable {
        public enum Source: String, Equatable, Sendable {
            case liveRun
            case parallel
            case resumeRecovery
            case resumePreview
            case queueSnapshot
        }

        public let source: Source
        public let title: String
        public let detail: String
        public let blockingState: BlockingState?
        public let secondaryResumeState: ResumeState?
        public let nextSteps: [RunControlOperatorStep]
        public let observedAt: String?

        static func build(
            liveBlockingState: BlockingState?,
            parallelStatus: ParallelStatus?,
            resumeState: ResumeState?,
            queueBlockingState: BlockingState?
        ) -> Self? {
            let resumeBlockingState = resumeState?.asDerivedBlockingState()
            let secondaryResumeState: ResumeState? = {
                guard let resumeState else { return nil }
                guard let resumeBlockingState else { return resumeState }
                guard case let .runnerRecovery(scope, reason, taskID) = resumeBlockingState.reason else {
                    return resumeState
                }
                if liveBlockingState?.matchesRunnerRecovery(scope: scope, reason: reason, taskID: taskID) == true {
                    return nil
                }
                if parallelStatus?.blocking?.matchesRunnerRecovery(scope: scope, reason: reason, taskID: taskID) == true {
                    return nil
                }
                return resumeState
            }()

            if let liveBlockingState {
                return Self(
                    source: .liveRun,
                    title: liveBlockingState.message,
                    detail: liveBlockingState.detail,
                    blockingState: liveBlockingState,
                    secondaryResumeState: secondaryResumeState,
                    nextSteps: operatorSteps(for: liveBlockingState, resumeState: secondaryResumeState),
                    observedAt: liveBlockingState.observedAt
                )
            }

            if let parallelBlockingState = parallelStatus?.blocking {
                return Self(
                    source: .parallel,
                    title: parallelBlockingState.message,
                    detail: parallelBlockingState.detail,
                    blockingState: parallelBlockingState,
                    secondaryResumeState: secondaryResumeState,
                    nextSteps: operatorSteps(for: parallelBlockingState, resumeState: secondaryResumeState),
                    observedAt: parallelBlockingState.observedAt
                )
            }

            if let resumeBlockingState, let resumeState {
                return Self(
                    source: .resumeRecovery,
                    title: resumeBlockingState.message,
                    detail: resumeBlockingState.detail,
                    blockingState: resumeBlockingState,
                    secondaryResumeState: nil,
                    nextSteps: operatorSteps(for: resumeBlockingState, resumeState: resumeState),
                    observedAt: resumeBlockingState.observedAt
                )
            }

            if let queueBlockingState {
                return Self(
                    source: .queueSnapshot,
                    title: queueBlockingState.message,
                    detail: queueBlockingState.detail,
                    blockingState: queueBlockingState,
                    secondaryResumeState: secondaryResumeState,
                    nextSteps: operatorSteps(for: queueBlockingState, resumeState: secondaryResumeState),
                    observedAt: queueBlockingState.observedAt
                )
            }

            if let resumeState {
                return Self(
                    source: .resumePreview,
                    title: resumeState.message,
                    detail: resumeState.detail,
                    blockingState: nil,
                    secondaryResumeState: nil,
                    nextSteps: operatorSteps(for: resumeState),
                    observedAt: nil
                )
            }

            return nil
        }

        private static func operatorSteps(
            for blockingState: BlockingState,
            resumeState: ResumeState?
        ) -> [RunControlOperatorStep] {
            var steps: [RunControlOperatorStep]
            switch blockingState.reason {
            case .idle:
                steps = [
                    RunControlOperatorStep(
                        title: "Queue is waiting",
                        detail: "No runnable task is ready right now. Add work or wait for the queue state to change."
                    )
                ]
            case .dependencyBlocked(let blockedTasks):
                steps = [
                    RunControlOperatorStep(
                        title: "Resolve blockers",
                        detail: "\(blockedTasks) candidate task(s) are waiting on unfinished dependencies."
                    )
                ]
            case .scheduleBlocked(_, let nextRunnableAt, let secondsUntilNextRunnable):
                let detail: String
                if let nextRunnableAt, let secondsUntilNextRunnable {
                    detail = "The next scheduled task becomes runnable at \(nextRunnableAt) (\(secondsUntilNextRunnable)s remaining)."
                } else {
                    detail = "Wait for scheduled work to become runnable or reschedule the blocked task."
                }
                steps = [RunControlOperatorStep(title: "Scheduled work pending", detail: detail)]
            case .lockBlocked:
                steps = [
                    RunControlOperatorStep(
                        title: "Inspect the queue lock",
                        detail: "Check the lock owner and preview unlock status before clearing anything."
                    ),
                    RunControlOperatorStep(
                        title: "Only clear verified stale locks",
                        detail: "Live, indeterminate, or broken metadata locks should stay explicit until an operator confirms recovery."
                    )
                ]
            case .ciBlocked(let pattern, let exitCode):
                let summary = [pattern, exitCode.map { "exit \($0)" }]
                    .compactMap { $0 }
                    .joined(separator: " · ")
                steps = [
                    RunControlOperatorStep(
                        title: "Repair CI gate",
                        detail: summary.isEmpty
                            ? "Resolve the failing CI requirement before continuing."
                            : "Resolve the CI requirement before continuing (\(summary))."
                    )
                ]
            case .runnerRecovery:
                steps = [
                    RunControlOperatorStep(
                        title: "Review the saved session",
                        detail: "Ralph needs an explicit recovery decision before continuing this run."
                    )
                ]
            case .operatorRecovery(_, _, let suggestedCommand):
                steps = [
                    RunControlOperatorStep(
                        title: "Follow the recovery path",
                        detail: suggestedCommand.map { "Suggested command: \($0)" }
                            ?? "Use the CLI recovery command suggested by Ralph before retrying."
                    )
                ]
            case .mixedQueue(let dependencyBlocked, let scheduleBlocked, let statusFiltered):
                steps = [
                    RunControlOperatorStep(
                        title: "Queue has mixed blockers",
                        detail: "Dependencies: \(dependencyBlocked) · Schedule: \(scheduleBlocked) · Filtered: \(statusFiltered)."
                    )
                ]
            }

            if let resumeState,
               resumeState.status != .refusingToResume,
               steps.count < 2 {
                steps.append(contentsOf: operatorSteps(for: resumeState))
            }
            return Array(steps.prefix(2))
        }

        private static func operatorSteps(for resumeState: ResumeState) -> [RunControlOperatorStep] {
            switch resumeState.status {
            case .resumingSameSession:
                return [
                    RunControlOperatorStep(
                        title: "Resume is ready",
                        detail: "The next run will continue the saved session instead of starting a fresh invocation."
                    )
                ]
            case .fallingBackToFreshInvocation:
                return [
                    RunControlOperatorStep(
                        title: "Fresh invocation selected",
                        detail: "Ralph will continue by starting a new invocation for the next run."
                    )
                ]
            case .refusingToResume:
                return [
                    RunControlOperatorStep(
                        title: "Operator confirmation required",
                        detail: "Resolve the recovery issue before retrying the run."
                    )
                ]
            }
        }
    }

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

private extension Workspace.ResumeState {
    func asDerivedBlockingState() -> Workspace.BlockingState? {
        guard status == .refusingToResume else {
            return nil
        }
        return Workspace.BlockingState(
            status: .stalled,
            reason: .runnerRecovery(scope: scope, reason: reason, taskID: taskID),
            taskID: taskID,
            message: message,
            detail: detail,
            observedAt: nil
        )
    }
}

private extension Workspace.BlockingState {
    func matchesRunnerRecovery(scope: String, reason: String, taskID: String?) -> Bool {
        guard case let .runnerRecovery(currentScope, currentReason, currentTaskID) = self.reason else {
            return false
        }
        return currentScope == scope && currentReason == reason && currentTaskID == taskID
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

    func startLoop(forceDirtyRepo: Bool? = nil, parallelWorkers: Int? = nil) {
        runnerController.startLoop(forceDirtyRepo: forceDirtyRepo, parallelWorkers: parallelWorkers)
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
