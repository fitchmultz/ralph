/**
 Workspace+RunnerExecutionModels

 Purpose:
 - Define execution-history and runner-configuration models used by workspace run flows.

 Responsibilities:
 - Represent execution phase metadata for run-control presentation.
 - Capture execution history records and success/duration helpers.
 - Represent resolved runner configuration and safety summary snapshots.

 Scope:
 - In scope: execution phase/history/configuration value models.
 - Out of scope: model loading, run invocation orchestration, and operator-state decisions.

 Usage:
 - Consumed by `WorkspaceRunState`, runner controller configuration loads, and run-control UI.

 Invariants/assumptions callers must respect:
 - `ExecutionPhase` raw values match phase numbers surfaced by CLI workflows.
 - `ExecutionRecord.success` requires zero exit code and no cancellation.
 */
public import Foundation
public import SwiftUI

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
