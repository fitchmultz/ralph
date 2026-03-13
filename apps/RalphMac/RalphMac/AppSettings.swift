/**
 SettingsView

 Responsibilities:
 - Provide the macOS Settings window UI for Runner, Notifications, and Appearance.
 - Bind to SettingsViewModel for state management.
 - Support standard macOS Cmd+, shortcut.

 Does not handle:
 - Config persistence (see SettingsViewModel).
 - CLI operations (see RalphCLIClient).

 Invariants/assumptions callers must respect:
 - Must receive a valid Workspace for the settings context.
 - Uses observable pattern for automatic UI updates.
 */

import AppKit
import SwiftUI
import RalphCore

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
    @State private var viewModel: SettingsViewModel
    @State private var selectedPane: SettingsPane = .runner

    init(workspace: Workspace) {
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
        .task {
            await viewModel.loadConfigIfNeeded()
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

@MainActor
private struct SettingsModelTextField: NSViewRepresentable {
    @Binding var text: String

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField(string: text)
        textField.placeholderString = "Model name"
        textField.delegate = context.coordinator
        configure(textField)
        return textField
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        configure(nsView)
        context.coordinator.configureFieldEditorIfNeeded(for: nsView)
    }

    private func configure(_ textField: NSTextField) {
        if #available(macOS 15.2, *) {
            textField.allowsWritingTools = false
        }
        textField.isAutomaticTextCompletionEnabled = false
        if #available(macOS 15.4, *) {
            textField.allowsWritingToolsAffordance = false
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        @Binding private var text: String

        init(text: Binding<String>) {
            self._text = text
        }

        func controlTextDidBeginEditing(_ notification: Notification) {
            guard let textField = notification.object as? NSTextField else { return }
            configureFieldEditorIfNeeded(for: textField)
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let textField = notification.object as? NSTextField else { return }
            configureFieldEditorIfNeeded(for: textField)
            text = textField.stringValue
        }

        func configureFieldEditorIfNeeded(for textField: NSTextField) {
            if #available(macOS 15.2, *) {
                textField.allowsWritingTools = false
            }
            textField.isAutomaticTextCompletionEnabled = false
            if #available(macOS 15.4, *) {
                textField.allowsWritingToolsAffordance = false
            }

            guard let editor = textField.currentEditor() as? NSTextView else { return }
            if #available(macOS 15.0, *) {
                editor.writingToolsBehavior = .none
            }
            editor.isContinuousSpellCheckingEnabled = false
            editor.isGrammarCheckingEnabled = false
            editor.isAutomaticQuoteSubstitutionEnabled = false
            editor.isAutomaticDashSubstitutionEnabled = false
            editor.isAutomaticTextReplacementEnabled = false
            editor.isAutomaticSpellingCorrectionEnabled = false
            editor.isAutomaticTextCompletionEnabled = false
            editor.smartInsertDeleteEnabled = false
        }
    }
}

// MARK: - Runner Settings Tab

struct RunnerSettingsTab: View {
    @Bindable var viewModel: SettingsViewModel

    var body: some View {
        Form {
            Section("Runner Selection") {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Runner")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    FlexibleChoiceRow(
                        options: viewModel.availableRunners.map { ($0.rawValue, $0.displayName) },
                        selection: $viewModel.runner
                    ) { newValue in
                        viewModel.handleRunnerChanged(to: newValue)
                    }
                }

                SettingsModelTextField(text: $viewModel.model)
                    .onChange(of: viewModel.model) { _, _ in viewModel.scheduleSave() }

                if !viewModel.suggestedModels.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Suggested Models")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        LazyVGrid(
                            columns: [GridItem(.adaptive(minimum: 150), alignment: .leading)],
                            alignment: .leading,
                            spacing: 8
                        ) {
                            ForEach(viewModel.suggestedModels, id: \.self) { model in
                                Button(model) {
                                    viewModel.selectSuggestedModel(model)
                                }
                                .buttonStyle(.bordered)
                                .controlSize(.small)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            }
                        }
                    }
                    .padding(.top, 4)
                }
            }

            Section("Execution Settings") {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Phases")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    FlexibleChoiceRow(
                        options: viewModel.availablePhases.map { ("\($0.rawValue)", $0.displayName) },
                        selection: Binding(
                            get: { String(viewModel.phases) },
                            set: { newValue in
                                guard let newPhase = Int(newValue) else { return }
                                viewModel.phases = newPhase
                                viewModel.scheduleSave()
                            }
                        )
                    )
                }

                Stepper("Iterations: \(viewModel.iterations)", value: $viewModel.iterations, in: 1...10)
                    .onChange(of: viewModel.iterations) { _, _ in viewModel.scheduleSave() }

                VStack(alignment: .leading, spacing: 10) {
                    Text("Reasoning Effort")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    FlexibleChoiceRow(
                        options: viewModel.availableEfforts.map { ($0.rawValue, $0.displayName) },
                        selection: $viewModel.reasoningEffort
                    ) { _ in
                        viewModel.scheduleSave()
                    }
                }
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

private struct FlexibleChoiceRow: View {
    let options: [(value: String, label: String)]
    @Binding var selection: String
    var onSelect: ((String) -> Void)?

    var body: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 140), alignment: .leading)],
            alignment: .leading,
            spacing: 8
        ) {
            ForEach(options, id: \.value) { option in
                Button {
                    guard selection != option.value else { return }
                    selection = option.value
                    onSelect?(option.value)
                } label: {
                    Text(option.label)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(selection == option.value ? Color.accentColor : Color.secondary.opacity(0.2))
                .foregroundStyle(selection == option.value ? Color.white : Color.primary)
            }
        }
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
    @ObservedObject private var appearance = AppAppearanceController.shared

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance:", selection: $appearance.selection) {
                    ForEach(AppColorScheme.allCases) { scheme in
                        Text(scheme.displayName).tag(scheme)
                    }
                }
                .pickerStyle(.radioGroup)

                Divider()

                VStack(alignment: .leading, spacing: 10) {
                    Text("Preview")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    RoundedRectangle(cornerRadius: 14)
                        .fill(previewBackground)
                        .frame(height: 140)
                        .overlay(alignment: .leading) {
                            VStack(alignment: .leading, spacing: 10) {
                                Text(previewTitle)
                                    .font(.headline)
                                    .foregroundStyle(previewForeground)

                                Text(appearance.selection.helperText)
                                    .foregroundStyle(previewForeground.opacity(0.8))

                                HStack(spacing: 8) {
                                    Capsule()
                                        .fill(Color.accentColor)
                                        .frame(width: 68, height: 24)
                                    Capsule()
                                        .fill(previewForeground.opacity(0.18))
                                        .frame(width: 96, height: 24)
                                }
                            }
                            .padding(18)
                        }
                }
            }

            Section {
                Text("Appearance changes apply immediately and persist across relaunch.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
    }

    private var previewTitle: String {
        switch appearance.selection {
        case .system:
            return "Match macOS"
        case .light:
            return "Light Appearance"
        case .dark:
            return "Dark Appearance"
        }
    }

    private var previewBackground: some ShapeStyle {
        switch appearance.selection {
        case .system:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(nsColor: .windowBackgroundColor), Color.accentColor.opacity(0.18)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case .light:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color.white, Color(red: 0.9, green: 0.95, blue: 1.0)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case .dark:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.12, green: 0.13, blue: 0.16), Color(red: 0.2, green: 0.24, blue: 0.31)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        }
    }

    private var previewForeground: Color {
        switch appearance.selection {
        case .system:
            return .primary
        case .light:
            return Color(red: 0.14, green: 0.16, blue: 0.2)
        case .dark:
            return Color.white
        }
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
    let workspace: Workspace?

    var body: some View {
        Group {
            if let workspace {
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
