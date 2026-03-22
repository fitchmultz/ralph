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

    private var displayBlockingState: Workspace.BlockingState? {
        guard let blockingState = workspace.runState.blockingState else {
            return nil
        }
        guard case let .runnerRecovery(scope, reason, taskID) = blockingState.reason,
              let resumeState = workspace.runState.resumeState,
              resumeState.scope == scope,
              resumeState.reason == reason,
              resumeState.taskID == taskID else {
            return blockingState
        }
        return nil
    }

    var body: some View {
        RunControlGlassSection("Controls") {
            VStack(spacing: 12) {
                if let blockingState = displayBlockingState {
                    blockingStateView(blockingState)
                }

                if let resumeState = workspace.runState.resumeState {
                    resumeStateView(resumeState)
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
                            Label("Start Loop", systemImage: "repeat.circle")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .disabled(workspace.nextTask() == nil)
                        .accessibilityLabel("Start task loop")
                        .accessibilityHint("Continuously run tasks until stopped")
                    }

                    Spacer()
                }

                if workspace.runState.isLoopMode {
                    HStack {
                        Image(systemName: "repeat.circle.fill")
                            .foregroundStyle(.blue)
                        Text("Loop Mode Active")
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
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: resumeIcon(for: state.status))
                .foregroundStyle(resumeColor(for: state.status))
                .font(.headline)
                .padding(.top, 1)

            VStack(alignment: .leading, spacing: 4) {
                Text(state.message)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                Text(state.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 0)
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(resumeColor(for: state.status).opacity(0.09))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(resumeColor(for: state.status).opacity(0.2), lineWidth: 1)
        )
    }

    @ViewBuilder
    private func blockingStateView(_ state: Workspace.BlockingState) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: blockingIcon(for: state.status))
                .foregroundStyle(blockingColor(for: state.status))
                .font(.headline)
                .padding(.top, 1)

            VStack(alignment: .leading, spacing: 4) {
                Text(state.message)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                Text(state.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 0)
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(blockingColor(for: state.status).opacity(0.09))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(blockingColor(for: state.status).opacity(0.2), lineWidth: 1)
        )
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
