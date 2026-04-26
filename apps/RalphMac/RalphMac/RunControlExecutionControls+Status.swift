/**
 RunControlExecutionControls+Status

 Purpose:
 - Render operator, resume, and parallel status cards for Run Control controls.

 Responsibilities:
 - Render operator and resume status cards with consistent icon/tint mapping.
 - Render shared parallel status details and action suggestions when appropriate.
 - Surface queue-lock condition copy when operator state indicates lock blocking.

 Scope:
 - Status rendering and visual helper mapping only.

 Does not handle:
 - Diagnostics-sheet lifecycle management.
 - Native operator-action side effects.

 Usage:
 - Embedded by `RunControlExecutionControlsSection` as the status group block.

 Invariants/Assumptions:
 - Operator-state and parallel-state classification come from `Workspace`/RalphCore.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlOperatorStatusGroup: View {
    @ObservedObject var workspace: Workspace
    let queueLockSnapshot: QueueLockDiagnosticSnapshot?
    let performAction: (Workspace.RunControlOperatorAction) -> Void
    let isActionDisabled: (Workspace.RunControlOperatorAction) -> Bool
    let isProminentAction: (Workspace.RunControlOperatorAction) -> Bool

    var body: some View {
        Group {
            if let operatorState = workspace.runState.runControlOperatorState {
                operatorStateView(operatorState)
            }

            if let resumeState = workspace.runState.runControlOperatorState?.secondaryResumeState {
                resumeStateView(resumeState)
            }

            if workspace.runState.shouldShowRunControlParallelStatus,
               workspace.runState.runControlOperatorState?.source != .parallel,
               workspace.runState.parallelStatus?.blocking != workspace.runState.runControlOperatorState?.blockingState {
                parallelStatusView
            }
        }
    }

    @ViewBuilder
    private func resumeStateView(_ state: Workspace.ResumeState) -> some View {
        RunControlTintedStatusCard(
            icon: resumeIcon(for: state.status),
            tint: resumeColor(for: state.status)
        ) {
            RunControlStatusText(title: state.message, detail: state.detail)
        }
    }

    @ViewBuilder
    private func operatorStateView(_ state: Workspace.RunControlOperatorState) -> some View {
        RunControlTintedStatusCard(
            icon: operatorStateIcon(for: state),
            tint: operatorStateColor(for: state)
        ) {
            RunControlStatusText(title: state.title, detail: state.detail)

            if state.source == .parallel, let parallelStatus = workspace.runState.parallelStatus {
                if let targetBranch = parallelStatus.snapshot.targetBranch, !targetBranch.isEmpty {
                    RunControlConfigRow(icon: "arrow.triangle.branch", label: "Target Branch", value: targetBranch)
                }

                if parallelStatus.snapshot.lifecycleCounts.total > 0 {
                    RunControlConfigRow(
                        icon: "square.stack.3d.up",
                        label: "Workers",
                        value: parallelCountSummary(for: parallelStatus.snapshot.lifecycleCounts)
                    )
                }
            }

            if !state.actions.isEmpty {
                RunControlOperatorActionsList(
                    actions: state.actions,
                    performAction: performAction,
                    isActionDisabled: isActionDisabled,
                    isProminentAction: isProminentAction
                )
            }

            if let blockingState = state.blockingState,
               case .lockBlocked = blockingState.reason,
               let queueLockSnapshot {
                Text("Lock status: \(queueLockSnapshot.condition.displayName)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if let observed = state.observedAt, !observed.isEmpty {
                Text("Blocking snapshot: \(observed)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    @ViewBuilder
    private var parallelStatusView: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Shared Parallel Status", systemImage: "square.stack.3d.up.fill")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)

            if workspace.runState.parallelStatusLoading {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Loading shared worker status...")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else if let error = workspace.runState.parallelStatusErrorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if let parallelStatus = workspace.runState.parallelStatus {
                RunControlTintedStatusCard(
                    icon: parallelStatusIcon(for: parallelStatus),
                    tint: parallelStatusColor(for: parallelStatus)
                ) {
                    RunControlStatusText(title: parallelStatus.headline, detail: parallelStatus.detail)

                    if let targetBranch = parallelStatus.snapshot.targetBranch, !targetBranch.isEmpty {
                        RunControlConfigRow(icon: "arrow.triangle.branch", label: "Target Branch", value: targetBranch)
                    }

                    if parallelStatus.snapshot.lifecycleCounts.total > 0 {
                        RunControlConfigRow(
                            icon: "square.stack.3d.up",
                            label: "Workers",
                            value: parallelCountSummary(for: parallelStatus.snapshot.lifecycleCounts)
                        )
                    }

                    let actions = Workspace.RunControlOperatorState.classifyParallelStatusActions(
                        parallelStatus.nextSteps,
                        isLoopMode: workspace.runState.isLoopMode,
                        stopAfterCurrent: workspace.runState.stopAfterCurrent
                    )
                    if !actions.isEmpty {
                        RunControlOperatorActionsList(
                            actions: actions,
                            performAction: performAction,
                            isActionDisabled: isActionDisabled,
                            isProminentAction: isProminentAction
                        )
                    }
                }
            } else {
                Text("Load shared worker status to inspect the current parallel operator state.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func parallelStatusIcon(for status: Workspace.ParallelStatus) -> String {
        if let blocking = status.blocking {
            return blockingIcon(for: blocking.status)
        }
        if status.snapshot.lifecycleCounts.hasActive {
            return "bolt.horizontal.circle.fill"
        }
        if status.snapshot.lifecycleCounts.failed > 0 {
            return "xmark.circle.fill"
        }
        return "checkmark.circle.fill"
    }

    private func parallelStatusColor(for status: Workspace.ParallelStatus) -> Color {
        if let blocking = status.blocking {
            return blockingColor(for: blocking.status)
        }
        if status.snapshot.lifecycleCounts.hasActive {
            return .blue
        }
        if status.snapshot.lifecycleCounts.failed > 0 {
            return .red
        }
        return .green
    }

    private func parallelCountSummary(for counts: ParallelLifecycleCounts) -> String {
        [
            counts.running > 0 ? "R \(counts.running)" : nil,
            counts.integrating > 0 ? "I \(counts.integrating)" : nil,
            counts.completed > 0 ? "C \(counts.completed)" : nil,
            counts.failed > 0 ? "F \(counts.failed)" : nil,
            counts.blocked > 0 ? "B \(counts.blocked)" : nil,
        ]
        .compactMap { $0 }
        .joined(separator: " · ")
    }

    private func blockingIcon(for status: Workspace.BlockingStatus) -> String {
        switch status {
        case .waiting:
            return "hourglass"
        case .blocked:
            return "pause.circle.fill"
        case .stalled:
            return "exclamationmark.triangle.fill"
        }
    }

    private func operatorStateIcon(for state: Workspace.RunControlOperatorState) -> String {
        if let blockingState = state.blockingState {
            return blockingIcon(for: blockingState.status)
        }
        if let resumeState = state.secondaryResumeState {
            return resumeIcon(for: resumeState.status)
        }
        switch state.source {
        case .resumePreview:
            return resumeIcon(for: workspace.runState.resumeState?.status ?? .fallingBackToFreshInvocation)
        case .parallel:
            return "square.stack.3d.up.fill"
        case .liveRun:
            return "bolt.horizontal.circle.fill"
        case .resumeRecovery:
            return "exclamationmark.octagon.fill"
        case .queueSnapshot:
            return "hourglass"
        }
    }

    private func operatorStateColor(for state: Workspace.RunControlOperatorState) -> Color {
        if let blockingState = state.blockingState {
            return blockingColor(for: blockingState.status)
        }
        switch workspace.runState.resumeState?.status {
        case .resumingSameSession:
            return .blue
        case .fallingBackToFreshInvocation:
            return .orange
        case .refusingToResume:
            return .red
        case .none:
            return .secondary
        }
    }

    private func blockingColor(for status: Workspace.BlockingStatus) -> Color {
        switch status {
        case .waiting:
            return .blue
        case .blocked:
            return .orange
        case .stalled:
            return .red
        }
    }

    private func resumeIcon(for status: Workspace.ResumeState.Status) -> String {
        switch status {
        case .resumingSameSession:
            return "arrow.clockwise.circle.fill"
        case .fallingBackToFreshInvocation:
            return "arrow.trianglehead.clockwise"
        case .refusingToResume:
            return "exclamationmark.octagon.fill"
        }
    }

    private func resumeColor(for status: Workspace.ResumeState.Status) -> Color {
        switch status {
        case .resumingSameSession:
            return .blue
        case .fallingBackToFreshInvocation:
            return .orange
        case .refusingToResume:
            return .red
        }
    }
}
