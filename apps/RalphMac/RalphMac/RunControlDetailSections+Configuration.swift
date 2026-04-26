/**
 RunControlDetailSections+Configuration

 Purpose:
 - Render runner-configuration and execution-control cards for Run Control.

 Responsibilities:
 - Render runner-configuration and execution-control cards for Run Control.
 - Keep execution action orchestration scoped to a thin owner view.
 - Surface resume-state decisions from machine config preview and live run events.

 Scope:
 - Runner-configuration rows and execution-controls orchestration only.

 Does not handle:
 - Task-summary cards.
 - Queue-preview selection or phase-progress rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
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

                RunControlConfigRow(icon: "bolt.circle", label: "Runner", value: workspace.runState.currentRunnerConfig?.runner ?? "Default")
                RunControlConfigRow(icon: "cpu", label: "Model", value: workspace.runState.currentRunnerConfig?.model ?? "Default")
                RunControlConfigRow(icon: "speedometer", label: "Reasoning Effort", value: workspace.runState.currentRunnerConfig?.reasoningEffort ?? "Default")
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
                RunControlOperatorStatusGroup(
                    workspace: workspace,
                    queueLockSnapshot: queueLockSnapshot,
                    performAction: performOperatorAction,
                    isActionDisabled: isOperatorActionDisabled,
                    isProminentAction: isProminentOperatorAction
                )

                RunControlLoopWorkersControls(workspace: workspace)

                RunControlExecutionActionBar(workspace: workspace)
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

    private func performOperatorAction(_ action: Workspace.RunControlOperatorAction) {
        switch action.disposition {
        case .native(let nativeAction):
            switch nativeAction {
            case .refreshRunControlStatus:
                Task { @MainActor in
                    await workspace.refreshRunControlStatusData()
                    await refreshQueueLockSnapshotIfNeeded()
                }
            case .refreshQueueStatus:
                Task { @MainActor in
                    await workspace.refreshRunControlQueueStatusData()
                    await refreshQueueLockSnapshotIfNeeded()
                }
            case .refreshParallelStatus:
                Task { @MainActor in
                    await workspace.loadParallelStatus(retryConfiguration: .minimal)
                    await refreshQueueLockSnapshotIfNeeded()
                }
            case .validateQueue:
                runDiagnosticsSheet(
                    title: "Queue Validation",
                    loadingTitle: "Validating queue..."
                ) {
                    await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
                }
            case .previewQueueRepair:
                runDiagnosticsSheet(
                    title: "Queue Repair Preview",
                    loadingTitle: "Previewing queue repair..."
                ) {
                    await WorkspaceDiagnosticsService.queueRepairPreviewOutput(for: workspace)
                }
            case .previewQueueUndo:
                runDiagnosticsSheet(
                    title: "Queue Restore Preview",
                    loadingTitle: "Previewing queue restore..."
                ) {
                    await WorkspaceDiagnosticsService.queueRestorePreviewOutput(for: workspace)
                }
            case .stopAfterCurrent:
                workspace.stopLoop()
            case .inspectQueueLock:
                runDiagnosticsSheet(
                    title: "Queue Lock Inspection",
                    loadingTitle: "Inspecting queue lock..."
                ) {
                    let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                    queueLockSnapshot = snapshot
                    return await WorkspaceDiagnosticsService.queueLockInspectionOutput(for: workspace)
                }
            case .previewQueueUnlock:
                runDiagnosticsSheet(
                    title: "Queue Unlock Preview",
                    loadingTitle: "Previewing queue unlock..."
                ) {
                    let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                    queueLockSnapshot = snapshot
                    return snapshot.unlockPreview
                }
            case .clearStaleQueueLock:
                runDiagnosticsSheet(
                    title: "Clear Stale Queue Lock",
                    loadingTitle: "Clearing stale queue lock..."
                ) {
                    let result = await WorkspaceDiagnosticsService.clearStaleQueueLock(for: workspace)
                    queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
                    return result
                }
            }
        case .copyCommand(let command):
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(command, forType: .string)
        case .unsupported:
            break
        }
    }

    private func isOperatorActionDisabled(_ action: Workspace.RunControlOperatorAction) -> Bool {
        guard case .native(let nativeAction) = action.disposition else {
            return false
        }

        switch nativeAction {
        case .inspectQueueLock, .previewQueueUnlock, .validateQueue, .previewQueueRepair, .previewQueueUndo:
            return isRunningDiagnostics
        case .clearStaleQueueLock:
            return queueLockSnapshot?.canClearStaleLock != true || isRunningDiagnostics
        case .stopAfterCurrent:
            return workspace.runState.stopAfterCurrent
        case .refreshRunControlStatus, .refreshQueueStatus, .refreshParallelStatus:
            return false
        }
    }

    private func isProminentOperatorAction(_ action: Workspace.RunControlOperatorAction) -> Bool {
        if case .native(.inspectQueueLock) = action.disposition {
            return true
        }
        return false
    }

    private func refreshQueueLockSnapshotIfNeeded() async {
        guard queueLockInspectionToken != nil else {
            queueLockSnapshot = nil
            return
        }
        queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
    }
}
