/**
 QuickActionsSection

 Responsibilities:
 - Provide Quick Actions content column with working directory header and console output.
 - Provide Quick Actions detail column with commands, status, and controls.
 - Handle quick commands like version check and init.

 Does not handle:
 - Task queue management (see QueueContent).
 - Advanced command configuration (see AdvancedRunnerSection).
 - Direct CLI execution (delegated to Workspace).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - View is used within NavigationSplitView context.
 */

import SwiftUI
import RalphCore

@MainActor
struct QuickActionsContentColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            WorkingDirectoryHeader(workspace: workspace)
                .padding(16)

            Divider()

            ConsoleView(workspace: workspace)
                .padding(16)
        }
        .contentBackground(cornerRadius: 12)
        .navigationTitle(navTitle("Quick Actions"))
    }
}

@MainActor
struct QuickActionsDetailColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                workingDirectorySection()

                Divider()

                quickCommandsSection()

                Divider()

                statusSection()

                errorSection()
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(navTitle("Quick Actions"))
    }

    @ViewBuilder
    private func workingDirectorySection() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Working Directory")
                .font(.headline)

            VStack(alignment: .leading, spacing: 4) {
                Text(workspace.identityState.name)
                    .font(.subheadline)
                Text(workspace.identityState.workingDirectoryURL.path)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            HStack {
                if !workspace.identityState.recentWorkingDirectories.isEmpty {
                    Menu("Recents") {
                        ForEach(workspace.identityState.recentWorkingDirectories, id: \.path) { url in
                            Button(url.path) {
                                workspace.selectRecentWorkingDirectory(url)
                            }
                        }
                    }
                }

                Button("Choose…") {
                    workspace.chooseWorkingDirectory()
                }
                .buttonStyle(GlassButtonStyle())
            }
        }
    }

    @ViewBuilder
    private func quickCommandsSection() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Quick Commands")
                .font(.headline)

            HStack(spacing: 12) {
                actionButton("Version", icon: "info.circle.fill", action: { workspace.runVersion() })
                actionButton("Init", icon: "folder.badge.plus", action: { workspace.runInit() })

                Spacer()

                if workspace.runState.isRunning {
                    Button(action: { workspace.cancel() }) {
                        Label("Stop", systemImage: "stop.circle.fill")
                            .foregroundStyle(.red)
                    }
                    .buttonStyle(.borderless)
                }
            }
        }
    }

    @ViewBuilder
    private func statusSection() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Status")
                .font(.headline)

            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 16) {
                    if let status = workspace.runState.lastExitStatus {
                        HStack(spacing: 6) {
                            Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                                .foregroundStyle(status.code == 0 ? .green : .red)
                            Text("Exit: \(status.code) [\(status.reason.rawValue)]")
                                .font(.system(.body, design: .monospaced))
                        }
                    } else {
                        Text("No commands run yet")
                            .foregroundStyle(.secondary)
                    }

                    Spacer()
                }

                queueStatusSummary()
            }
        }
    }

    @ViewBuilder
    private func queueStatusSummary() -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label(queueStatusHeadline, systemImage: queueStatusIcon)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(queueStatusColor)

                Spacer()

                Button("Reload") {
                    Task { @MainActor in
                        await workspace.refreshRepositoryState(retryConfiguration: .minimal)
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(workspace.taskState.tasksLoading || workspace.insightsState.graphDataLoading || workspace.insightsState.analytics.isLoading)
            }

            Text(queueStatusDetail)
                .font(.caption)
                .foregroundStyle(.secondary)

            if !workspace.diagnosticsState.operationalSummary.isHealthy {
                Text(workspace.diagnosticsState.operationalSummary.title)
                    .font(.caption)
                    .foregroundStyle(.orange)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.08))
        .clipShape(.rect(cornerRadius: 10))
    }

    private var queueStatusHeadline: String {
        if workspace.taskState.tasksLoading {
            return "Refreshing queue state..."
        }

        if workspace.taskState.tasks.isEmpty {
            return "Queue is empty"
        }

        return "Queue has \(workspace.taskState.tasks.count) task\(workspace.taskState.tasks.count == 1 ? "" : "s")"
    }

    private var queueStatusIcon: String {
        if workspace.taskState.tasksLoading {
            return "arrow.triangle.2.circlepath"
        }

        return workspace.taskState.tasks.isEmpty ? "tray" : "list.bullet.rectangle"
    }

    private var queueStatusColor: Color {
        if workspace.taskState.tasksLoading {
            return .accentColor
        }

        return workspace.taskState.tasks.isEmpty ? .secondary : .primary
    }

    private var queueStatusDetail: String {
        let todoCount = workspace.taskState.tasks.filter { $0.status == .todo }.count
        let doingCount = workspace.taskState.tasks.filter { $0.status == .doing }.count
        let doneCount = workspace.taskState.tasks.filter { $0.status == .done }.count
        let watcherStatus = watcherStatusText

        if workspace.taskState.tasksLoading {
            return "Reloading tasks, graph, and analytics from \(workspace.identityState.workingDirectoryURL.path). Watcher: \(watcherStatus)."
        }

        return "Todo \(todoCount) • Doing \(doingCount) • Done \(doneCount) • Watcher: \(watcherStatus)"
    }

    private var watcherStatusText: String {
        switch workspace.diagnosticsState.watcherHealth.state {
        case .idle:
            return "idle"
        case .starting:
            return "starting"
        case .watching:
            return "watching"
        case .degraded:
            return "degraded"
        case .failed:
            return "failed"
        case .stopped:
            return "stopped"
        }
    }

    @ViewBuilder
    private func errorSection() -> some View {
        if let error = workspace.runState.errorMessage {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .accessibilityLabel("Error")
                    Text("Error")
                        .font(.headline)
                        .foregroundStyle(.red)

                    Spacer()

                    if workspace.diagnosticsState.lastRecoveryError != nil {
                        Button("Show Recovery Options") {
                            workspace.diagnosticsState.showErrorRecovery = true
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                    }
                }

                Text(error)
                    .foregroundStyle(.red)
                    .font(.body)
            }
        }
    }

    private func actionButton(_ title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
        }
        .buttonStyle(GlassButtonStyle())
        .accessibilityLabel("\(title)")
    }
}
