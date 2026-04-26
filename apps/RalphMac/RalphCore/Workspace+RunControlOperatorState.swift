/**
 Workspace+RunControlOperatorState

 Purpose:
 - Define run-control operator action/state models and precedence-aware classification logic.

 Responsibilities:
 - Build a single operator-state snapshot from live, parallel, resume, and queue sources.
 - Classify continuation and recovery actions into native/copy/unsupported dispositions.
 - Deduplicate run-control actions while preserving source-specific trailing controls.

 Scope:
 - In scope: `RunControlOperatorAction`, `RunControlOperatorState`, and helper classification routines.
 - Out of scope: workspace storage mutation, console rendering, and command execution side effects.

 Usage:
 - Consumed by `WorkspaceRunState` synthesis updates and run-control presentation layers.

 Invariants/assumptions callers must respect:
 - Source precedence is live -> parallel -> resume recovery -> queue -> resume preview.
 - Recovery action IDs remain stable for deduplication and UI identity.
 */
import Foundation

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
                    ),
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
                    ),
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
}
