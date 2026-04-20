/**
 RunControlDetailSections+Configuration

 Responsibilities:
 - Render runner-configuration and execution-control cards for Run Control.
 - Keep execution actions and status presentation out of progress/history/safety sections.
 - Surface resume-state decisions from machine config preview and live run events.

 Does not handle:
 - Task-summary cards.
 - Queue-preview selection or phase-progress rendering.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlRunnerConfigurationSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Runner Configuration") {
            VStack(alignment: .leading, spacing: 8) {
                if workspace.runState.runnerConfigLoading {
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                        Text("Loading resolved config...")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                RunControlConfigRow(icon: "cpu", label: "Model", value: workspace.runState.currentRunnerConfig?.model ?? "Default")
                RunControlConfigRow(icon: "square.split.2x1", label: "Phases", value: workspace.runState.currentRunnerConfig?.phases.map(String.init) ?? "Auto")
                RunControlConfigRow(icon: "number", label: "Max Iterations", value: workspace.runState.currentRunnerConfig?.maxIterations.map(String.init) ?? "Auto")

                if let configError = workspace.runState.runnerConfigErrorMessage {
                    Text(configError)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

@MainActor
struct RunControlExecutionControlsSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Controls") {
            VStack(spacing: 12) {
                if let blockingState = workspace.runState.runControlDisplayBlockingState {
                    blockingStateView(blockingState)
                }

                if let resumeState = workspace.runState.resumeState {
                    resumeStateView(resumeState)
                }

                if workspace.runState.shouldShowRunControlParallelStatus {
                    parallelStatusView
                }

                let previewTask = workspace.runControlPreviewTask
                let hasSelectedTask = workspace.selectedRunControlTask != nil

                HStack(spacing: 12) {
                    if workspace.runState.isRunning {
                        Button(action: { workspace.cancel() }) {
                            Label("Stop", systemImage: "stop.circle.fill")
                                .foregroundStyle(.red)
                        }
                        .buttonStyle(GlassButtonStyle())
                        .accessibilityLabel("Stop execution")
                        .accessibilityHint("Cancel the current task execution")

                        if workspace.runState.isLoopMode {
                            Button(action: { workspace.stopLoop() }) {
                                Label("Stop After Current", systemImage: "pause.circle")
                                    .foregroundStyle(.orange)
                            }
                            .buttonStyle(GlassButtonStyle())
                        }
                    } else {
                        Button(action: {
                            workspace.runNextTask(
                                taskIDOverride: workspace.runState.runControlSelectedTaskID,
                                forceDirtyRepo: workspace.runState.runControlForceDirtyRepo
                            )
                        }) {
                            Label(hasSelectedTask ? "Run Selected Task" : "Run Next Task", systemImage: "play.circle.fill")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .disabled(previewTask == nil)
                        .accessibilityLabel("Run next task")
                        .accessibilityHint("Starts execution of the selected task or next task in the queue")

                        Button(action: { workspace.startLoop(forceDirtyRepo: workspace.runState.runControlForceDirtyRepo) }) {
                            Label("Start CLI Loop", systemImage: "repeat.circle")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .accessibilityLabel("Start CLI loop")
                        .accessibilityHint("Runs the CLI loop with max tasks set to zero, then streams progress until the loop completes or is stopped")
                    }

                    Spacer()
                }

                if workspace.runState.isLoopMode {
                    HStack {
                        Image(systemName: "repeat.circle.fill")
                            .foregroundStyle(.blue)
                        Text("CLI Loop Active")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        if workspace.runState.stopAfterCurrent {
                            Text("(Stopping after current)")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }

                        Spacer()
                    }
                }

                if let status = workspace.runState.lastExitStatus, !workspace.runState.isRunning {
                    HStack {
                        Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Text("Exit: \(status.code)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Spacer()
                    }
                }
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
    private func blockingStateView(_ state: Workspace.BlockingState) -> some View {
        RunControlTintedStatusCard(
            icon: blockingIcon(for: state.status),
            tint: blockingColor(for: state.status)
        ) {
            RunControlStatusText(title: state.message, detail: state.detail)
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

                    if !parallelStatus.nextSteps.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Next")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                            ForEach(parallelStatus.nextSteps.prefix(2), id: \.command) { step in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(step.command)
                                        .font(.system(.caption, design: .monospaced))
                                    Text(step.detail)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
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
