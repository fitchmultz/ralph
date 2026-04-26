/**
 Workspace+RunnerStatusModels

 Purpose:
 - Define runner status and blocking/resume models shared by workspace run control.

 Responsibilities:
 - Represent parallel status snapshots, blocking reasons, and resume-state outcomes.
 - Provide derived blocking-state and runner-recovery match helpers.

 Scope:
 - In scope: status/resume/blocking models and private bridge helpers.
 - Out of scope: command dispatch, operator-state precedence orchestration, and console buffering.

 Usage:
 - Consumed by `WorkspaceRunState`, runner controller outputs, and machine-contract adapters.

 Invariants/assumptions callers must respect:
 - Blocking and resume semantics align with machine-contract vocabulary.
 - Derived resume blocking applies only to refusing-to-resume outcomes.
 */
import Foundation

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
}

extension Workspace.ResumeState {
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

extension Workspace.BlockingState {
    func matchesRunnerRecovery(scope: String, reason: String, taskID: String?) -> Bool {
        guard case let .runnerRecovery(currentScope, currentReason, currentTaskID) = self.reason else {
            return false
        }
        return currentScope == scope && currentReason == reason && currentTaskID == taskID
    }
}
