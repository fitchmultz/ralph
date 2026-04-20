/**
 ErrorRecoveryView

 Responsibilities:
 - Render recovery guidance and category-specific actions for workspace failures.
 - Trigger diagnostics, queue-recovery previews, and log loading through shared services.
 - Present diagnostic and recovery sheets for operators without coupling to task-conflict UI.

 Does not handle:
 - Error classification.
 - Workspace mutation retry implementations.
 - Conflict-resolution flows.

 Invariants/assumptions callers must respect:
 - The supplied `RecoveryError` is already classified.
 - The surrounding view owns retry/dismiss callbacks.
 - Recovery operations target the provided workspace when one is available.
 */

import AppKit
import RalphCore
import SwiftUI

private extension ErrorCategory {
    var offlineGuidance: String? {
        switch self {
        case .cliUnavailable:
            return """
            The Ralph CLI is not available. This can happen when:
            - The app bundle is damaged or incomplete
            - The ralph binary was moved or deleted
            - Security software quarantined the binary

            Try reinstalling Ralph or checking your security software.
            """
        case .permissionDenied:
            return """
            Ralph cannot access the workspace directory. This can happen when:
            - The directory was moved or deleted
            - File permissions changed
            - The workspace is on a disconnected drive

            Check that the workspace path is still valid and accessible.
            """
        case .configIncompatible:
            return """
            The selected workspace is using an older or unsupported Ralph config contract.

            Run `ralph migrate --apply` in the repository, then retry the action.
            """
        case .networkError:
            return """
            A network-related operation timed out. This can happen when:
            - The CLI took too long to respond
            - The system is under heavy load
            - There is a resource deadlock

            Try again in a moment.
            """
        case .queueLock:
            return """
            Ralph found queue-lock contention or a broken queue-lock record.

            Inspect the current lock owner and preview unlock state before clearing anything. The app only enables stale-lock clearing when the lock is confirmed dead-PID stale.
            """
        default:
            return guidanceMessage
        }
    }

    var recoveryTint: Color {
        switch self {
        case .cliUnavailable: return .orange
        case .permissionDenied: return .red
        case .configIncompatible: return .yellow
        case .parseError: return .yellow
        case .networkError: return .blue
        case .queueCorrupted: return .red
        case .queueLock: return .orange
        case .resourceBusy: return .orange
        case .versionMismatch: return .purple
        case .unknown: return .gray
        }
    }
}

@MainActor
struct ErrorRecoveryView: View {
    let error: RecoveryError
    let workspace: Workspace?
    let onRetry: () -> Void
    let onDismiss: () -> Void

    @State private var showingActionSheet = false
    @State private var actionSheetTitle = "Diagnostic Results"
    @State private var actionSheetOutput = ""
    @State private var actionSheetLoadingTitle = "Running diagnostics..."
    @State private var isRunningAction = false
    @State private var showingLogsSheet = false
    @State private var logsContent = ""
    @State private var isLoadingLogs = false
    @State private var queueLockSnapshot: QueueLockDiagnosticSnapshot?

    var body: some View {
        VStack(spacing: 20) {
            VStack(spacing: 12) {
                Image(systemName: error.category.icon)
                    .font(.system(size: 48))
                    .foregroundStyle(error.category.recoveryTint)
                    .accessibilityLabel("Error: \(error.category.displayName)")

                Text(error.category.displayName)
                    .font(.headline)
                    .foregroundStyle(error.category.recoveryTint)

                Text(error.message)
                    .font(.body)
                    .multilineTextAlignment(.center)
            }

            if let guidance = error.category.offlineGuidance {
                Text(guidance)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)
            }

            VStack(spacing: 8) {
                ForEach(error.category.suggestedActions, id: \.self) { action in
                    recoveryButton(for: action)
                }
            }
            .padding(.top, 8)
        }
        .padding(24)
        .frame(maxWidth: 400)
        .background(.ultraThinMaterial)
        .clipShape(.rect(cornerRadius: 12))
        .sheet(isPresented: $showingActionSheet) {
            DiagnosticsTextSheet(
                title: actionSheetTitle,
                text: actionSheetOutput,
                isLoading: isRunningAction,
                loadingTitle: actionSheetLoadingTitle
            )
        }
        .sheet(isPresented: $showingLogsSheet) {
            DiagnosticsTextSheet(
                title: "Ralph Logs",
                text: logsContent,
                isLoading: isLoadingLogs,
                loadingTitle: "Loading logs..."
            )
            .frame(minWidth: 600, minHeight: 400)
        }
        .task(id: error.category) {
            await refreshQueueLockSnapshotIfNeeded()
        }
    }

    @ViewBuilder
    private func recoveryButton(for action: RecoveryAction) -> some View {
        switch action {
        case .retry:
            Button(action: onRetry) {
                Label("Retry", systemImage: "arrow.clockwise")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)

        case .diagnose:
            Button(action: performDiagnosis) {
                Label("Diagnose", systemImage: "stethoscope")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .validateQueue:
            Button(action: performQueueValidation) {
                Label("Validate Queue", systemImage: "checkmark.shield")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .repairQueue:
            Button(action: performQueueRepairPreview) {
                Label("Preview Queue Repair", systemImage: "wrench.and.screwdriver")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .restoreLastCheckpoint:
            Button(action: performRestorePreview) {
                Label("Preview Restore", systemImage: "clock.arrow.circlepath")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .copyErrorDetails:
            Button(action: copyErrorDetails) {
                Label("Copy Error Details", systemImage: "doc.on.doc")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)

        case .openLogs:
            Button(action: openLogs) {
                Label("View Logs", systemImage: "doc.text.magnifyingglass")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)

        case .dismiss:
            Button(action: onDismiss) {
                Label("Dismiss", systemImage: "xmark")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderless)

        case .checkPermissions:
            Button(action: checkPermissions) {
                Label("Check Permissions", systemImage: "folder.badge.gearshape")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)

        case .reinstallCLI:
            Button(action: openReinstallationHelp) {
                Label("Reinstallation Help", systemImage: "arrow.down.circle")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)

        case .inspectQueueLock:
            Button(action: inspectQueueLock) {
                Label("Inspect Queue Lock", systemImage: "lock.doc")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .previewQueueUnlock:
            Button(action: previewQueueUnlock) {
                Label("Preview Queue Unlock", systemImage: "eye")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction)

        case .clearStaleQueueLock:
            Button(action: clearStaleQueueLock) {
                Label("Clear Stale Queue Lock", systemImage: "lock.open.trianglebadge.exclamationmark")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isRunningAction || queueLockSnapshot?.canClearStaleLock != true)
        }
    }

    private func copyErrorDetails() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(error.fullErrorDetails, forType: .string)
    }

    private func performDiagnosis() {
        runActionSheet(
            title: "Diagnostic Results",
            loadingTitle: "Running diagnostics..."
        ) {
            guard let workspace else {
                return "No workspace is available for diagnostics."
            }
            if error.category == .queueLock {
                return await WorkspaceDiagnosticsService.queueLockInspectionOutput(for: workspace)
            }
            return await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        }
    }

    private func performQueueValidation() {
        runActionSheet(
            title: "Queue Validation",
            loadingTitle: "Validating queue..."
        ) {
            guard let workspace else {
                return "No workspace is available for queue validation."
            }
            return await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
        }
    }

    private func performQueueRepairPreview() {
        runActionSheet(
            title: "Queue Repair Preview",
            loadingTitle: "Previewing queue repair..."
        ) {
            guard let workspace else {
                return "No workspace is available for queue repair preview."
            }
            return await WorkspaceDiagnosticsService.queueRepairPreviewOutput(for: workspace)
        }
    }

    private func performRestorePreview() {
        runActionSheet(
            title: "Restore Preview",
            loadingTitle: "Previewing continuation restore..."
        ) {
            guard let workspace else {
                return "No workspace is available for restore preview."
            }
            return await WorkspaceDiagnosticsService.queueRestorePreviewOutput(for: workspace)
        }
    }

    private func runActionSheet(
        title: String,
        loadingTitle: String,
        action: @escaping @MainActor () async -> String
    ) {
        actionSheetTitle = title
        actionSheetLoadingTitle = loadingTitle
        actionSheetOutput = ""
        showingActionSheet = true
        isRunningAction = true

        Task { @MainActor in
            actionSheetOutput = await action()
            isRunningAction = false
        }
    }

    private func openLogs() {
        showingLogsSheet = true
        isLoadingLogs = true

        Task { @MainActor in
            logsContent = await WorkspaceDiagnosticsService.recentLogs(hours: 2)
            isLoadingLogs = false
        }
    }

    private func checkPermissions() {
        if let url = error.workspaceURL ?? workspace?.identityState.workingDirectoryURL {
            NSWorkspace.shared.open(url)
        }
    }

    private func openReinstallationHelp() {
        guard let url = URL(string: "https://github.com/fitchmultz/ralph#installation") else { return }
        NSWorkspace.shared.open(url)
    }

    private func inspectQueueLock() {
        runActionSheet(
            title: "Queue Lock Inspection",
            loadingTitle: "Inspecting queue lock..."
        ) {
            guard let workspace else {
                return "No workspace is available for queue-lock inspection."
            }
            let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
            queueLockSnapshot = snapshot
            return await WorkspaceDiagnosticsService.queueLockInspectionOutput(for: workspace)
        }
    }

    private func previewQueueUnlock() {
        runActionSheet(
            title: "Queue Unlock Preview",
            loadingTitle: "Previewing queue unlock..."
        ) {
            guard let workspace else {
                return "No workspace is available for queue-unlock preview."
            }
            let snapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
            queueLockSnapshot = snapshot
            return snapshot.unlockPreview
        }
    }

    private func clearStaleQueueLock() {
        runActionSheet(
            title: "Clear Stale Queue Lock",
            loadingTitle: "Clearing stale queue lock..."
        ) {
            guard let workspace else {
                return "No workspace is available for stale queue-lock recovery."
            }
            let result = await WorkspaceDiagnosticsService.clearStaleQueueLock(for: workspace)
            queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
            return result
        }
    }

    private func refreshQueueLockSnapshotIfNeeded() async {
        guard error.category == .queueLock, let workspace else {
            queueLockSnapshot = nil
            return
        }
        queueLockSnapshot = await WorkspaceDiagnosticsService.queueLockDiagnosticSnapshot(for: workspace)
    }
}

@MainActor
private struct DiagnosticsTextSheet: View {
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
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                            .textSelection(.enabled)
                    }
                }
            }
            .navigationTitle(title)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(text, forType: .string)
                    }
                    .disabled(isLoading)
                }
            }
        }
        .frame(minWidth: 500, minHeight: 300)
    }
}

@MainActor
struct ErrorRecoverySheet: View {
    let error: RecoveryError
    let workspace: Workspace?
    let onRetry: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack {
            ErrorRecoveryView(
                error: error,
                workspace: workspace,
                onRetry: onRetry,
                onDismiss: onDismiss
            )
        }
        .padding()
        .frame(minWidth: 450, minHeight: 420)
    }
}

#Preview("Error Recovery") {
    ErrorRecoveryView(
        error: RecoveryError(
            category: .queueCorrupted,
            message: "Queue data appears corrupted",
            underlyingError: "queue validation failed: duplicate id RQ-0001",
            operation: "loadTasks",
            suggestions: ["Run `ralph queue validate`", "Preview `ralph queue repair --dry-run`"]
        ),
        workspace: nil,
        onRetry: {},
        onDismiss: {}
    )
}
