/**
 ErrorRecoveryView

 Responsibilities:
 - Render recovery guidance and category-specific actions for workspace failures.
 - Trigger diagnostics/log loading via shared services instead of inline view logic.
 - Present diagnostic/log sheets for operators without coupling to task-conflict UI.

 Does not handle:
 - Error classification.
 - Workspace mutation retry implementations.
 - Conflict-resolution flows.

 Invariants/assumptions callers must respect:
 - The supplied `RecoveryError` is already classified.
 - The surrounding view owns retry/dismiss callbacks.
 - Diagnostic operations target the provided workspace when one is available.
 */

import AppKit
import SwiftUI
import RalphCore

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

    @State private var showingDiagnoseSheet = false
    @State private var diagnoseOutput = ""
    @State private var isDiagnosing = false
    @State private var showingLogsSheet = false
    @State private var logsContent = ""
    @State private var isLoadingLogs = false

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
        .sheet(isPresented: $showingDiagnoseSheet) {
            DiagnosticsTextSheet(
                title: "Diagnostic Results",
                text: diagnoseOutput,
                isLoading: isDiagnosing,
                loadingTitle: "Running diagnostics..."
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

        case .diagnose, .validateQueue:
            Button(action: performDiagnosis) {
                Label(action == .diagnose ? "Diagnose" : "Validate Queue", systemImage: action == .diagnose ? "stethoscope" : "checkmark.shield")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isDiagnosing)

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
        }
    }

    private func copyErrorDetails() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(error.fullErrorDetails, forType: .string)
    }

    private func performDiagnosis() {
        showingDiagnoseSheet = true
        isDiagnosing = true

        Task { @MainActor in
            diagnoseOutput = await diagnosticsOutput()
            isDiagnosing = false
        }
    }

    private func diagnosticsOutput() async -> String {
        guard let workspace else {
            return "No workspace is available for diagnostics."
        }
        return await WorkspaceDiagnosticsService.queueValidationOutput(for: workspace)
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
        .frame(minWidth: 450, minHeight: 400)
    }
}

#Preview("Error Recovery") {
    ErrorRecoveryView(
        error: RecoveryError(
            category: .cliUnavailable,
            message: "Failed to load tasks",
            underlyingError: "Executable not found at expected path",
            operation: "loadTasks",
            suggestions: ["Check installation", "Verify permissions"]
        ),
        workspace: nil,
        onRetry: {},
        onDismiss: {}
    )
}
