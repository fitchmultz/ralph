/**
 AppSettings+Notifications

 Responsibilities:
 - Render notification toggles and explanatory copy for Settings.
 - Keep notification-tab presentation out of the root settings shell.

 Does not handle:
 - Runner or appearance controls.
 - Notification delivery runtime.
 */

import SwiftUI

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

                Toggle("Notify when watch mode adds tasks from comments", isOn: $viewModel.notifyOnWatchNewTasks)
                    .disabled(!viewModel.notificationsEnabled)
                    .onChange(of: viewModel.notifyOnWatchNewTasks) { _, _ in viewModel.scheduleSave() }
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
