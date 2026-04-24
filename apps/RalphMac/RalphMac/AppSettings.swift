/**
 SettingsView

 Purpose:
 - Provide the macOS Settings window UI for Runner, Notifications, and Appearance.

 Responsibilities:
 - Provide the macOS Settings window UI for Runner, Notifications, and Appearance.
 - Bind to SettingsViewModel for state management.
 - Support standard macOS Cmd+, shortcut.

 Does not handle:
 - Config persistence (see SettingsViewModel).
 - CLI operations (see RalphCLIClient).
 - Tab-specific control implementation details in this file.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Must receive a valid Workspace for the settings context.
 - Uses observable pattern for automatic UI updates.
 - Tab content and AppKit bridge helpers live in adjacent `AppSettings+...` files.
 */

import AppKit
import RalphCore
import SwiftUI

enum SettingsAccessibilityID {
    static let root = "settings-root"
    static let modelField = "settings-model-field"
}

private enum SettingsPane: String, CaseIterable, Identifiable {
    case runner
    case notifications
    case appearance

    var id: String { rawValue }

    var title: String {
        switch self {
        case .runner:
            return "Runner"
        case .notifications:
            return "Notifications"
        case .appearance:
            return "Appearance"
        }
    }

    var subtitle: String {
        switch self {
        case .runner:
            return "Default runner, model, and execution settings."
        case .notifications:
            return "Desktop alerts, sounds, and active-app behavior."
        case .appearance:
            return "Choose how Ralph renders in light or dark mode."
        }
    }

    var systemImage: String {
        switch self {
        case .runner:
            return "gearshape.2"
        case .notifications:
            return "bell"
        case .appearance:
            return "paintbrush"
        }
    }
}

struct SettingsView: View {
    @ObservedObject private var workspace: Workspace
    private let presentationToken: String
    @State private var viewModel: SettingsViewModel
    @State private var selectedPane: SettingsPane = .runner

    init(workspace: Workspace, presentationToken: String) {
        self.workspace = workspace
        self.presentationToken = presentationToken
        self._viewModel = State(initialValue: SettingsViewModel(workspace: workspace))
    }

    var body: some View {
        HStack(spacing: 0) {
            settingsSidebar

            Divider()

            VStack(alignment: .leading, spacing: 0) {
                settingsHeader
                if let errorMessage = viewModel.errorMessage {
                    Divider()
                    settingsErrorBanner(errorMessage)
                }
                if viewModel.isLoading {
                    Divider()
                    settingsLoadingBanner
                }
                Divider()
                settingsContent
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .task(id: settingsReloadToken) {
            let refreshedViewModel = SettingsViewModel(workspace: workspace)
            viewModel = refreshedViewModel
            await refreshedViewModel.loadConfig()
        }
        .task(id: diagnosticsSnapshotToken) {
            SettingsPresentationCoordinator.shared.captureContent(
                workspacePath: workspace.identityState.workingDirectoryURL.path,
                runner: viewModel.runner,
                model: viewModel.model,
                isLoading: viewModel.isLoading
            )
        }
        .frame(minWidth: 760, minHeight: 520)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Reset") {
                    viewModel.resetToDefaults()
                }
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .accessibilityIdentifier(SettingsAccessibilityID.root)
    }

    private var settingsReloadToken: String {
        [
            presentationToken,
            workspace.id.uuidString,
            String(workspace.identityState.retargetRevision),
            workspace.identityState.workingDirectoryURL.path,
        ].joined(separator: "|")
    }

    private var diagnosticsSnapshotToken: String {
        [
            settingsReloadToken,
            viewModel.runner,
            viewModel.model,
            viewModel.isLoading ? "loading" : "loaded"
        ].joined(separator: "|")
    }

    private var settingsSidebar: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(SettingsPane.allCases) { pane in
                Button {
                    selectedPane = pane
                } label: {
                    HStack(spacing: 12) {
                        Image(systemName: pane.systemImage)
                            .frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(pane.title)
                                .font(.headline)
                            Text(pane.subtitle)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background {
                        RoundedRectangle(cornerRadius: 12)
                            .fill(
                                selectedPane == pane
                                    ? Color.accentColor.opacity(0.16)
                                    : Color.clear
                            )
                    }
                }
                .buttonStyle(.plain)
            }

            Spacer()
        }
        .padding(16)
        .frame(width: 250)
        .frame(maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var settingsHeader: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(selectedPane.title)
                .font(.title2.weight(.semibold))
            Text(selectedPane.subtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private var settingsLoadingBanner: some View {
        HStack(spacing: 12) {
            ProgressView()
                .controlSize(.small)

            Text("Loading settings…")
                .font(.subheadline)
                .foregroundStyle(.secondary)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 14)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    @ViewBuilder
    private func settingsErrorBanner(_ message: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.yellow)

            VStack(alignment: .leading, spacing: 6) {
                Text("Settings Error")
                    .font(.headline)
                Text(message)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 0)

            Button("Dismiss") {
                viewModel.errorMessage = nil
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 16)
        .background(Color.yellow.opacity(0.08))
    }

    @ViewBuilder
    private var settingsContent: some View {
        switch selectedPane {
        case .runner:
            RunnerSettingsTab(viewModel: viewModel)
        case .notifications:
            NotificationsSettingsTab(viewModel: viewModel)
        case .appearance:
            AppearanceSettingsTab()
        }
    }
}
