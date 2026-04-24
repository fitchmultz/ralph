/**
 AppSettings+Runner

 Purpose:
 - Render runner, model, phase, iteration, and reasoning controls for Settings.

 Responsibilities:
 - Render runner, model, phase, iteration, and reasoning controls for Settings.
 - Keep runner-tab selection UI and button-grid helpers out of the root settings shell.

 Does not handle:
 - Notifications or appearance settings.
 - Underlying settings persistence logic.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI

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
                    .accessibilityIdentifier(SettingsAccessibilityID.modelField)
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
