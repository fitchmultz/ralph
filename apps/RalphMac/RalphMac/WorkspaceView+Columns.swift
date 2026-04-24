/**
 WorkspaceView+Columns

 Purpose:
 - Render workspace sidebar, content, detail, and footer column composition.

 Responsibilities:
 - Render workspace sidebar, content, detail, and footer column composition.
 - Keep section routing and queue-detail presentation out of the root workspace shell.

 Does not handle:
 - Command wiring.
 - Error-recovery or task-mutation actions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
extension WorkspaceView {
    @ViewBuilder
    func sidebarColumn() -> some View {
        VStack(spacing: 0) {
            List(SidebarSection.allCases, selection: $navigation.selectedSection) { section in
                Label(section.rawValue, systemImage: section.icon)
                    .tag(section)
                    .accessibilityHint("Navigate to \(section.rawValue)")
            }
            .accessibilityLabel("Main navigation")
            .listStyle(.sidebar)

            connectionStatusFooter()
        }
        .navigationTitle(navTitle(navigation.selectedSection.rawValue))
    }

    @ViewBuilder
    func contentColumn() -> some View {
        VStack(spacing: 0) {
            if !workspace.diagnosticsState.operationalSummary.isHealthy {
                OperationalStatusBannerView(
                    summary: workspace.diagnosticsState.operationalSummary,
                    onRetry: { handleRepairOperationalHealth() },
                    onDismiss: nil
                )
                .transition(.move(edge: .top).combined(with: .opacity))
            }

            sectionContent()
        }
    }

    @ViewBuilder
    func detailColumn() -> some View {
        switch navigation.selectedSection {
        case .queue:
            queueDetailColumn()
        case .quickActions:
            QuickActionsDetailColumn(workspace: workspace, navTitle: navTitle)
        case .runControl:
            RunControlDetailColumn(workspace: workspace, navTitle: navTitle)
        case .advancedRunner:
            AdvancedRunnerDetailColumn(workspace: workspace, navTitle: navTitle)
        case .analytics:
            AnalyticsDetailColumn(workspace: workspace, navTitle: navTitle)
        }
    }

    @ViewBuilder
    func sectionContent() -> some View {
        switch navigation.selectedSection {
        case .queue:
            queueContentColumn()
        case .quickActions:
            QuickActionsContentColumn(workspace: workspace, navTitle: navTitle)
        case .runControl:
            RunControlContentColumn(workspace: workspace, navTitle: navTitle)
        case .advancedRunner:
            AdvancedRunnerContentColumn(workspace: workspace, navTitle: navTitle)
        case .analytics:
            AnalyticsDashboardView(workspace: workspace)
        }
    }

    @ViewBuilder
    func queueContentColumn() -> some View {
        VStack(spacing: 0) {
            viewModeToolbar()
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

            Divider()

            switch navigation.taskViewMode {
            case .list:
                TaskListView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID,
                    selectedTaskIDs: $navigation.selectedTaskIDs,
                    showTaskCreation: showTaskCreation,
                    showTaskDecompose: { taskID in showTaskDecompose(selectedTaskID: taskID) },
                    showTaskDetail: showTaskDetail
                )
            case .kanban:
                KanbanBoardView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID,
                    showTaskDetail: showTaskDetail
                )
            case .graph:
                DependencyGraphView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID
                )
            }
        }
    }

    @ViewBuilder
    func viewModeToolbar() -> some View {
        HStack {
            Spacer()

            Picker("View Mode", selection: $navigation.taskViewMode) {
                ForEach(TaskViewMode.allCases, id: \.self) { mode in
                    Label(mode.rawValue, systemImage: mode.icon)
                        .tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 240)
            .help("Switch between List, Kanban, and Graph view (⌘⇧K)")
            .accessibilityLabel("Task view mode")
            .accessibilityIdentifier("task-view-mode-picker")
        }
    }

    @ViewBuilder
    func queueDetailColumn() -> some View {
        if let taskID = navigation.selectedTaskID,
           let task = workspace.taskState.tasks.first(where: { $0.id == taskID }) {
            TaskDetailView(
                workspace: workspace,
                task: task,
                onTaskUpdated: { _ in
                    Task { @MainActor in await workspace.loadTasks() }
                }
            )
        } else {
            EmptyDetailView(
                icon: "list.bullet.rectangle",
                title: "No Task Selected",
                message: "Select a task from the list to view and edit its details."
            )
        }
    }

    @ViewBuilder
    func connectionStatusFooter() -> some View {
        Divider()
        HStack {
            ConnectionStatusIndicator(
                summary: workspace.diagnosticsState.operationalSummary,
                onTap: {
                    showingOperationalHealth = true
                }
            )

            Spacer()

            if workspace.diagnosticsState.cliHealthStatus?.isAvailable == false && !workspace.diagnosticsState.cachedTasks.isEmpty {
                Label("Cached", systemImage: "archivebox")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .help("Showing cached task list")
            }

            if case .failed = workspace.diagnosticsState.watcherHealth.state {
                Label("Watcher", systemImage: "dot.scope.display")
                    .font(.caption2)
                    .foregroundStyle(.red)
                    .help("Queue watching failed and needs repair.")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial)
    }
}
