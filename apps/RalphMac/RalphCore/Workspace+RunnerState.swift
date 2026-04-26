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
    @Published public var isPreparingRun = false
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
                attributedOutput = streamProcessor.displaySegments(
                    maxSegments: maxANSISegments,
                    maxCharacters: outputBuffer.maxCharacters
                )
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

    public var isExecutionActive: Bool {
        isPreparingRun || isRunning
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
        isPreparingRun = false
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
        if pendingConsoleText.count > outputBuffer.maxCharacters {
            outputBuffer.append(pendingConsoleText)
            pendingConsoleText.removeAll(keepingCapacity: true)
        }
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
        attributedOutput = streamProcessor.displaySegments(
            maxSegments: maxANSISegments,
            maxCharacters: outputBuffer.maxCharacters
        )
    }

    private func refreshOperatorState() {
        let operatorState = Workspace.RunControlOperatorState.build(
            liveBlockingState: liveBlockingState,
            parallelStatus: parallelStatus,
            resumeState: resumeState,
            queueBlockingState: queueBlockingState,
            isLoopMode: isLoopMode,
            stopAfterCurrent: stopAfterCurrent
        )
        runControlOperatorState = operatorState
        blockingState = operatorState?.blockingState
    }
}

public extension Workspace {
    struct RunControlOperatorAction: Identifiable, Equatable, Sendable {
        public enum NativeAction: String, Equatable, Sendable {
            case refreshRunControlStatus
            case refreshQueueStatus
            case refreshParallelStatus
            case validateQueue
            case previewQueueRepair
            case previewQueueUndo
            case stopAfterCurrent
            case inspectQueueLock
            case previewQueueUnlock
            case clearStaleQueueLock
        }

        public enum Disposition: Equatable, Sendable {
            case native(NativeAction)
            case copyCommand(String)
            case unsupported(reason: String, command: String?)
        }

        public let id: String
        public let title: String
        public let detail: String
        public let command: String?
        public let disposition: Disposition

        public init(
            id: String,
            title: String,
            detail: String,
            command: String? = nil,
            disposition: Disposition
        ) {
            self.id = id
            self.title = title
            self.detail = detail
            self.command = command
            self.disposition = disposition
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
        public let actions: [RunControlOperatorAction]
        public let observedAt: String?

        static func build(
            liveBlockingState: BlockingState?,
            parallelStatus: ParallelStatus?,
            resumeState: ResumeState?,
            queueBlockingState: BlockingState?,
            isLoopMode: Bool,
            stopAfterCurrent: Bool
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
                    actions: operatorActions(
                        for: liveBlockingState,
                        source: .liveRun,
                        continuationSteps: [],
                        isLoopMode: isLoopMode,
                        stopAfterCurrent: stopAfterCurrent
                    ),
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
                    actions: operatorActions(
                        for: parallelBlockingState,
                        source: .parallel,
                        continuationSteps: parallelStatus?.nextSteps ?? [],
                        isLoopMode: isLoopMode,
                        stopAfterCurrent: stopAfterCurrent
                    ),
                    observedAt: parallelBlockingState.observedAt
                )
            }

            if let resumeBlockingState {
                return Self(
                    source: .resumeRecovery,
                    title: resumeBlockingState.message,
                    detail: resumeBlockingState.detail,
                    blockingState: resumeBlockingState,
                    secondaryResumeState: nil,
                    actions: operatorActions(
                        for: resumeBlockingState,
                        source: .resumeRecovery,
                        continuationSteps: [],
                        isLoopMode: isLoopMode,
                        stopAfterCurrent: stopAfterCurrent
                    ),
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
                    actions: operatorActions(
                        for: queueBlockingState,
                        source: .queueSnapshot,
                        continuationSteps: [],
                        isLoopMode: isLoopMode,
                        stopAfterCurrent: stopAfterCurrent
                    ),
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
                    actions: operatorActions(
                        for: resumeState,
                        source: .resumePreview,
                        isLoopMode: isLoopMode,
                        stopAfterCurrent: stopAfterCurrent
                    ),
                    observedAt: nil
                )
            }

            return nil
        }

        public static func classifyParallelStatusActions(
            _ steps: [ParallelStatusStep],
            isLoopMode: Bool = false,
            stopAfterCurrent: Bool = false
        ) -> [RunControlOperatorAction] {
            let actions = steps.map {
                classifyContinuationAction(title: $0.title, command: $0.command, detail: $0.detail)
            }
            return deduplicatedActions(
                actions + trailingOperatorActions(
                    source: .parallel,
                    isLoopMode: isLoopMode,
                    stopAfterCurrent: stopAfterCurrent
                )
            )
        }

        private static func operatorActions(
            for blockingState: BlockingState,
            source: Source,
            continuationSteps: [ParallelStatusStep],
            isLoopMode: Bool,
            stopAfterCurrent: Bool
        ) -> [RunControlOperatorAction] {
            var actions = actions(for: blockingState)
            actions.append(contentsOf: continuationSteps.map {
                classifyContinuationAction(title: $0.title, command: $0.command, detail: $0.detail)
            })
            actions.append(contentsOf: trailingOperatorActions(
                source: source,
                isLoopMode: isLoopMode,
                stopAfterCurrent: stopAfterCurrent
            ))
            return deduplicatedActions(actions)
        }

        private static func operatorActions(
            for resumeState: ResumeState,
            source: Source,
            isLoopMode: Bool,
            stopAfterCurrent: Bool
        ) -> [RunControlOperatorAction] {
            let actions = actions(for: resumeState) + trailingOperatorActions(
                source: source,
                isLoopMode: isLoopMode,
                stopAfterCurrent: stopAfterCurrent
            )
            return deduplicatedActions(actions)
        }

        private static func actions(for blockingState: BlockingState) -> [RunControlOperatorAction] {
            switch blockingState.reason {
            case .idle:
                return [
                    nativeAction(
                        .refreshQueueStatus,
                        title: "Refresh Queue Status",
                        detail: "Reload runnability and runner state after queue conditions change."
                    )
                ]
            case .dependencyBlocked(let blockedTasks):
                return [
                    nativeAction(
                        .refreshQueueStatus,
                        title: "Refresh Dependency Status",
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
                return [
                    nativeAction(
                        .refreshQueueStatus,
                        title: "Refresh Scheduled Status",
                        detail: detail
                    )
                ]
            case .lockBlocked:
                return [
                    nativeAction(
                        .inspectQueueLock,
                        title: "Inspect Queue Lock",
                        detail: "Check the lock owner and preview unlock status before clearing anything."
                    ),
                    nativeAction(
                        .previewQueueUnlock,
                        title: "Preview Queue Unlock",
                        detail: "Review the unlock report before attempting recovery."
                    ),
                    nativeAction(
                        .clearStaleQueueLock,
                        title: "Clear Stale Queue Lock",
                        detail: "Only clear verified stale locks. Live or indeterminate locks should remain explicit."
                    )
                ]
            case .ciBlocked(let pattern, let exitCode):
                let summary = [pattern, exitCode.map { "exit \($0)" }]
                    .compactMap { $0 }
                    .joined(separator: " · ")
                return [
                    nativeAction(
                        .refreshRunControlStatus,
                        title: "Refresh CI Gate Status",
                        detail: summary.isEmpty
                            ? "Resolve the failing CI requirement before continuing."
                            : "Resolve the CI requirement before continuing (\(summary))."
                    )
                ]
            case .runnerRecovery:
                return [
                    nativeAction(
                        .refreshRunControlStatus,
                        title: "Refresh Resume Status",
                        detail: "Reload the current recovery decision before retrying the run."
                    )
                ]
            case .operatorRecovery(let scope, _, let suggestedCommand):
                var actions = queueRecoveryActionsIfNeeded(scope: scope)
                if let suggestedCommand {
                    actions.append(classifyContinuationAction(
                        title: "Follow Recovery Path",
                        command: suggestedCommand,
                        detail: "Use the suggested continuation when you are ready to retry."
                    ))
                } else {
                    actions.append(
                        unsupportedAction(
                            title: "Follow Recovery Path",
                            detail: "Ralph requires explicit operator recovery before the run can continue.",
                            command: nil,
                            reason: "This recovery flow does not expose a native app action yet."
                        )
                    )
                }
                return actions
            case .mixedQueue(let dependencyBlocked, let scheduleBlocked, let statusFiltered):
                return [
                    nativeAction(
                        .validateQueue,
                        title: "Validate Queue",
                        detail: "Review queue health before making recovery changes."
                    ),
                    nativeAction(
                        .previewQueueRepair,
                        title: "Preview Queue Repair",
                        detail: "Inspect undo-backed normalization before applying any queue repair."
                    ),
                    nativeAction(
                        .previewQueueUndo,
                        title: "Preview Queue Restore",
                        detail: "Inspect the current rollback path before additional queue changes."
                    ),
                    nativeAction(
                        .refreshQueueStatus,
                        title: "Refresh Mixed Queue Status",
                        detail: "Dependencies: \(dependencyBlocked) · Schedule: \(scheduleBlocked) · Filtered: \(statusFiltered)."
                    )
                ]
            }
        }

        private static func actions(for resumeState: ResumeState) -> [RunControlOperatorAction] {
            switch resumeState.status {
            case .resumingSameSession:
                return [
                    nativeAction(
                        .refreshRunControlStatus,
                        title: "Refresh Resume Decision",
                        detail: "The next run will continue the saved session instead of starting a fresh invocation."
                    )
                ]
            case .fallingBackToFreshInvocation:
                return [
                    nativeAction(
                        .refreshRunControlStatus,
                        title: "Refresh Resume Decision",
                        detail: "Ralph will continue by starting a new invocation for the next run."
                    )
                ]
            case .refusingToResume:
                return [
                    nativeAction(
                        .refreshRunControlStatus,
                        title: "Refresh Resume Decision",
                        detail: "Resolve the recovery issue before retrying the run."
                    )
                ]
            }
        }

        private static func queueRecoveryActionsIfNeeded(scope: String) -> [RunControlOperatorAction] {
            let normalizedScope = scope.lowercased()
            guard normalizedScope == "queue"
                || normalizedScope.hasPrefix("queue_") else {
                return []
            }

            return [
                nativeAction(
                    .validateQueue,
                    title: "Validate Queue",
                    detail: "Re-run the machine queue validation report before applying recovery changes."
                ),
                nativeAction(
                    .previewQueueRepair,
                    title: "Preview Queue Repair",
                    detail: "Inspect undo-backed repair output before changing queue files."
                ),
                nativeAction(
                    .previewQueueUndo,
                    title: "Preview Queue Restore",
                    detail: "Inspect the current rollback path before applying more queue changes."
                ),
            ]
        }

        private static func classifyContinuationAction(
            title: String?,
            command: String,
            detail: String
        ) -> RunControlOperatorAction {
            let normalizedCommand = normalizedCommand(command)
            switch normalizedCommand {
            case "ralph machine queue validate":
                return nativeAction(
                    .validateQueue,
                    title: title ?? "Validate Queue",
                    detail: detail,
                    command: command
                )
            case "ralph machine queue repair --dry-run":
                return nativeAction(
                    .previewQueueRepair,
                    title: title ?? "Preview Queue Repair",
                    detail: detail,
                    command: command
                )
            case "ralph machine queue undo --dry-run":
                return nativeAction(
                    .previewQueueUndo,
                    title: title ?? "Preview Queue Restore",
                    detail: detail,
                    command: command
                )
            case "ralph machine run parallel-status":
                return nativeAction(
                    .refreshParallelStatus,
                    title: title ?? "Refresh Parallel Status",
                    detail: detail,
                    command: command
                )
            case "ralph machine run stop":
                return nativeAction(
                    .stopAfterCurrent,
                    title: title ?? "Stop After Current",
                    detail: detail,
                    command: command
                )
            case "ralph machine run stop --dry-run":
                return copyAction(
                    title: title ?? "Preview Stop Request",
                    detail: detail,
                    command: command
                )
            case "ralph machine run one --resume":
                return unsupportedAction(
                    title: title ?? "Continue Work",
                    detail: detail,
                    command: command,
                    reason: "Run Control uses the existing Run Next Task and Run Selected Task buttons for this flow."
                )
            default:
                if normalizedCommand.hasPrefix("ralph machine run loop --resume --max-tasks 0 --parallel") {
                    return unsupportedAction(
                        title: title ?? "Start Parallel Execution",
                        detail: detail,
                        command: command,
                        reason: "Use the native Start Loop controls above to choose the worker count."
                    )
                }
                if normalizedCommand.hasPrefix("ralph run parallel retry --task") {
                    return unsupportedAction(
                        title: title ?? "Retry Worker Integration",
                        detail: detail,
                        command: command,
                        reason: "Retained worker retry is not exposed as a native RalphMac action yet."
                    )
                }
                return copyAction(
                    title: title ?? command,
                    detail: detail,
                    command: command
                )
            }
        }

        private static func trailingOperatorActions(
            source: Source,
            isLoopMode: Bool,
            stopAfterCurrent: Bool
        ) -> [RunControlOperatorAction] {
            var actions: [RunControlOperatorAction] = []
            if isLoopMode && !stopAfterCurrent {
                actions.append(
                    nativeAction(
                        .stopAfterCurrent,
                        title: "Stop After Current",
                        detail: "Record a stop request and let the active task finish cleanly."
                    )
                )
            }
            actions.append(
                nativeAction(
                    source == .parallel ? .refreshParallelStatus : .refreshRunControlStatus,
                    title: source == .parallel ? "Refresh Parallel Status" : "Refresh Run Control Status",
                    detail: source == .parallel
                        ? "Reload shared parallel worker state from the machine contract."
                        : "Reload config, resume, blocking, and queue-derived Run Control state."
                )
            )
            return actions
        }

        private static func nativeAction(
            _ nativeAction: RunControlOperatorAction.NativeAction,
            title: String,
            detail: String,
            command: String? = nil
        ) -> RunControlOperatorAction {
            RunControlOperatorAction(
                id: "native:\(nativeAction.rawValue)",
                title: title,
                detail: detail,
                command: command,
                disposition: .native(nativeAction)
            )
        }

        private static func copyAction(
            title: String,
            detail: String,
            command: String
        ) -> RunControlOperatorAction {
            RunControlOperatorAction(
                id: "copy:\(normalizedCommand(command))",
                title: title,
                detail: detail,
                command: command,
                disposition: .copyCommand(command)
            )
        }

        private static func unsupportedAction(
            title: String,
            detail: String,
            command: String?,
            reason: String
        ) -> RunControlOperatorAction {
            RunControlOperatorAction(
                id: "unsupported:\(command.map(normalizedCommand) ?? normalizedCommand(title))",
                title: title,
                detail: detail,
                command: command,
                disposition: .unsupported(reason: reason, command: command)
            )
        }

        private static func normalizedCommand(_ command: String) -> String {
            command
                .split(whereSeparator: \.isWhitespace)
                .joined(separator: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }

        private static func deduplicatedActions(
            _ actions: [RunControlOperatorAction]
        ) -> [RunControlOperatorAction] {
            var seen = Set<String>()
            return actions.filter { seen.insert($0.id).inserted }
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
        public let runner: String?
        public let model: String?
        public let reasoningEffort: String?
        public let phases: Int?
        public let maxIterations: Int?
        public let executionControls: MachineExecutionControls?
        public let safety: RunnerSafetySummary?

        public init(
            runner: String? = nil,
            model: String? = nil,
            reasoningEffort: String? = nil,
            phases: Int? = nil,
            maxIterations: Int? = nil,
            executionControls: MachineExecutionControls? = nil,
            safety: RunnerSafetySummary? = nil
        ) {
            self.runner = runner
            self.model = model
            self.reasoningEffort = reasoningEffort
            self.phases = phases
            self.maxIterations = maxIterations
            self.executionControls = executionControls
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
