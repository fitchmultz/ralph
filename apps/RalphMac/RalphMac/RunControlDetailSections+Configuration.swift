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
    @State private var showingDiagnosticsSheet = false
    @State private var diagnosticsSheetTitle = ""
    @State private var diagnosticsSheetText = ""
    @State private var diagnosticsSheetLoadingTitle = "Loading..."
    @State private var isRunningDiagnostics = false
    @State private var queueLockSnapshot: QueueLockDiagnosticSnapshot?

    private var queueLockInspectionToken: String? {
        guard let blockingState = workspace.runState.runControlOperatorState?.blockingState,
              case .lockBlocked = blockingState.reason else {
            return nil
        }
        return [
            workspace.runState.runControlOperatorState?.title,
            workspace.runState.runControlOperatorState?.detail,
            workspace.runState.runControlOperatorState?.observedAt,
        ]
        .compactMap { $0 }
        .joined(separator: "|")
    }

    var body: some View {
        RunControlGlassSection("Controls") {
            VStack(spacing: 12) {
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
        .sheet(isPresented: $showingDiagnosticsSheet) {
            RunControlDiagnosticsTextSheet(
                title: diagnosticsSheetTitle,
                text: diagnosticsSheetText,
                isLoading: isRunningDiagnostics,
                loadingTitle: diagnosticsSheetLoadingTitle
            )
        }
        .task(id: queueLockInspectionToken) {
            await refreshQueueLockSnapshotIfNeeded()
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

            if !state.nextSteps.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Next")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    ForEach(Array(state.nextSteps.enumerated()), id: \.offset) { _, step in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(step.title)
                                .font(.caption.weight(.medium))
                            Text(step.detail)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            if let blockingState = state.blockingState,
               case .lockBlocked = blockingState.reason {
                lockRecoveryActionsView
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

    @ViewBuilder
    private var lockRecoveryActionsView: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Lock Recovery")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)

            HStack(spacing: 8) {
                Button("Inspect Lock") {
                    runDiagnosticsSheet(
                        title: "Queue Lock Inspection",
                        loadingTitle: "Inspecting queue lock..."
                    ) {
                        let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                        queueLockSnapshot = snapshot
                        return await WorkspaceDiagnosticsService.queueLockInspectionOutput(for: workspace)
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Button("Preview Unlock") {
                    runDiagnosticsSheet(
                        title: "Queue Unlock Preview",
                        loadingTitle: "Previewing queue unlock..."
                    ) {
                        let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                        queueLockSnapshot = snapshot
                        return snapshot.unlockPreview
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                Button("Clear Stale Lock") {
                    runDiagnosticsSheet(
                        title: "Clear Stale Queue Lock",
                        loadingTitle: "Clearing stale queue lock..."
                    ) {
                        let result = await WorkspaceDiagnosticsService.clearStaleQueueLock(for: workspace)
                        queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                        return result
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(queueLockSnapshot?.canClearStaleLock != true || isRunningDiagnostics)
            }

            if let queueLockSnapshot {
                Text("Lock status: \(queueLockSnapshot.condition.displayName)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
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

    private func runDiagnosticsSheet(
        title: String,
        loadingTitle: String,
        action: @escaping @MainActor () async -> String
    ) {
        diagnosticsSheetTitle = title
        diagnosticsSheetLoadingTitle = loadingTitle
        diagnosticsSheetText = ""
        showingDiagnosticsSheet = true
        isRunningDiagnostics = true

        Task { @MainActor in
            diagnosticsSheetText = await action()
            isRunningDiagnostics = false
        }
    }

    private func refreshQueueLockSnapshotIfNeeded() async {
        guard queueLockInspectionToken != nil else {
            queueLockSnapshot = nil
            return
        }
        queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
    }
}

@MainActor
private struct RunControlDiagnosticsTextSheet: View {
    let title: String
    let text: String
    let isLoading: Bool
    let loadingTitle: String

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack {
                if isLoading {
                    ProgressView(loadingTitle)
                        .padding()
                } else {
                    ScrollView {
                        Text(text)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                    }
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .navigationTitle(title)
        }
        .frame(minWidth: 600, minHeight: 400)
    }
}
