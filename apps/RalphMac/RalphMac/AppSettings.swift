/**
 SettingsView

 Responsibilities:
 - Provide Settings window UI with TabView for Runner, Notifications, Appearance.
 - Bind to SettingsViewModel for state management.
 - Support standard macOS Cmd+, shortcut.

 Does not handle:
 - Config persistence (see SettingsViewModel).
 - CLI operations (see RalphCLIClient).

 Invariants/assumptions callers must respect:
 - Must receive a valid Workspace for the settings context.
 - Uses observable pattern for automatic UI updates.
 */

import SwiftUI
import RalphCore

struct SettingsView: View {
    @State private var viewModel: SettingsViewModel
    @Environment(\.dismiss) private var dismiss

    init(workspace: Workspace) {
        self._viewModel = State(initialValue: SettingsViewModel(workspace: workspace))
    }

    var body: some View {
        TabView {
            RunnerSettingsTab(viewModel: viewModel)
                .tabItem {
                    Label("Runner", systemImage: "gearshape.2")
                }

            NotificationsSettingsTab(viewModel: viewModel)
                .tabItem {
                    Label("Notifications", systemImage: "bell")
                }

            AppearanceSettingsTab()
                .tabItem {
                    Label("Appearance", systemImage: "paintbrush")
                }
        }
        .frame(minWidth: 500, minHeight: 400)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Done") {
                    Task {
                        await viewModel.saveConfig()
                    }
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }

            ToolbarItem(placement: .cancellationAction) {
                Button("Reset") {
                    viewModel.resetToDefaults()
                }
            }
        }
        .overlay {
            if viewModel.isLoading {
                ProgressView("Loading...")
                    .padding()
                    .background(.regularMaterial)
                    .cornerRadius(8)
            }
        }
        .alert("Error", isPresented: .init(
            get: { viewModel.errorMessage != nil },
            set: { if !$0 { viewModel.errorMessage = nil } }
        )) {
            Button("OK") { viewModel.errorMessage = nil }
        } message: {
            if let error = viewModel.errorMessage {
                Text(error)
            }
        }
    }
}

// MARK: - Runner Settings Tab

struct RunnerSettingsTab: View {
    @Bindable var viewModel: SettingsViewModel

    var body: some View {
        Form {
            Section("Runner Selection") {
                Picker("Runner:", selection: $viewModel.runner) {
                    ForEach(viewModel.availableRunners) { runner in
                        Text(runner.displayName).tag(runner.rawValue)
                    }
                }
                .onChange(of: viewModel.runner) { _, newValue in
                    // Auto-select first suggested model for new runner
                    if let firstModel = viewModel.commonModels[newValue]?.first {
                        viewModel.model = firstModel
                    }
                    viewModel.scheduleSave()
                }

                HStack {
                    Text("Model:")
                    TextField("Model name", text: $viewModel.model)
                        .onChange(of: viewModel.model) { _, _ in viewModel.scheduleSave() }

                    // Quick model picker for common options
                    Menu {
                        ForEach(viewModel.suggestedModels, id: \.self) { model in
                            Button(model) {
                                viewModel.model = model
                                viewModel.scheduleSave()
                            }
                        }
                    } label: {
                        Image(systemName: "chevron.down.circle")
                    }
                    .frame(width: 30)
                }
            }

            Section("Execution Settings") {
                Picker("Phases:", selection: $viewModel.phases) {
                    ForEach(viewModel.availablePhases) { phase in
                        Text(phase.displayName).tag(phase.rawValue)
                    }
                }
                .onChange(of: viewModel.phases) { _, _ in viewModel.scheduleSave() }

                Stepper("Iterations: \(viewModel.iterations)", value: $viewModel.iterations, in: 1...10)
                    .onChange(of: viewModel.iterations) { _, _ in viewModel.scheduleSave() }

                Picker("Reasoning Effort:", selection: $viewModel.reasoningEffort) {
                    ForEach(viewModel.availableEfforts) { effort in
                        Text(effort.displayName).tag(effort.rawValue)
                    }
                }
                .onChange(of: viewModel.reasoningEffort) { _, _ in viewModel.scheduleSave() }
            }

            Section {
                HStack {
                    Spacer()
                    if viewModel.hasUnsavedChanges {
                        Text("Unsaved changes")
                            .foregroundStyle(.secondary)
                            .font(.caption)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}

// MARK: - Notifications Settings Tab

struct NotificationsSettingsTab: View {
    @Bindable var viewModel: SettingsViewModel

    var body: some View {
        Form {
            Section("Desktop Notifications") {
                Toggle("Enable notifications", isOn: $viewModel.notificationsEnabled)
                    .onChange(of: viewModel.notificationsEnabled) { _, _ in viewModel.scheduleSave() }

                Toggle("Notify on task completion", isOn: $viewModel.notifyOnComplete)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.notifyOnComplete) { _, _ in viewModel.scheduleSave() }

                Toggle("Notify on task failure", isOn: $viewModel.notifyOnFail)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.notifyOnFail) { _, _ in viewModel.scheduleSave() }

                Toggle("Notify when loop completes", isOn: $viewModel.notifyOnLoopComplete)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.notifyOnLoopComplete) { _, _ in viewModel.scheduleSave() }
            }

            Section("Sound & Behavior") {
                Toggle("Play sound with notification", isOn: $viewModel.soundEnabled)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.soundEnabled) { _, _ in viewModel.scheduleSave() }

                Toggle("Suppress when app is active", isOn: $viewModel.suppressWhenActive)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.suppressWhenActive) { _, _ in viewModel.scheduleSave() }
            }

            Section {
                Text("Notification settings affect both CLI runs and in-app task execution.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}

// MARK: - Appearance Settings Tab

struct AppearanceSettingsTab: View {
    @AppStorage("colorScheme") private var colorScheme: AppColorScheme = .system

    enum AppColorScheme: String, CaseIterable {
        case system = "system"
        case light = "light"
        case dark = "dark"

        var displayName: String {
            switch self {
            case .system: return "System"
            case .light: return "Light"
            case .dark: return "Dark"
            }
        }

        var colorScheme: ColorScheme? {
            switch self {
            case .system: return nil
            case .light: return .light
            case .dark: return .dark
            }
        }
    }

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance:", selection: $colorScheme) {
                    ForEach(AppColorScheme.allCases, id: \.self) { scheme in
                        Text(scheme.displayName).tag(scheme)
                    }
                }
                .pickerStyle(.radioGroup)
            }

            Section {
                Text("Additional appearance settings will be added in future updates.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}

#Preview {
    SettingsView(workspace: Workspace(
        workingDirectoryURL: URL(fileURLWithPath: "/Users/example/project")
    ))
}

// MARK: - Settings Window Content

/// Container view referenced from RalphMacApp.swift using AnyView to work around compilation order.
/// AnyView defers type checking to runtime/link time, allowing SettingsView to be defined in this file.
@MainActor
struct SettingsContentContainer: View {
    @ObservedObject private var manager = WorkspaceManager.shared
    
    init() {}
    
    var body: some View {
        Group {
            if let workspace = manager.focusedWorkspace ?? manager.workspaces.first {
                SettingsView(workspace: workspace)
            } else {
                NoWorkspaceSettingsView()
            }
        }
    }
}

@MainActor
struct NoWorkspaceSettingsView: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "gearshape.2")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            
            Text("No Workspace Available")
                .font(.headline)
            
            Text("Open a workspace to configure settings.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
