/**
 WorkspaceView

 Responsibilities:
 - Display the Ralph UI using a modern three-column NavigationSplitView layout.
 - Left sidebar: Navigation sections (Queue, Quick Actions, Run Control, Advanced Runner, Analytics)
 - Middle column: Content list (delegated to section-specific content views)
 - Right column: Detail/inspector view (delegated to section-specific detail views)
 - Bind to a specific Workspace instance for isolated state management.

 Does not handle:
 - Window-level tab management (see WindowView).
 - Cross-workspace operations.
 - Direct navigation state persistence (see NavigationViewModel).
 - Section-specific UI (delegated to *Section views).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - NavigationViewModel manages sidebar state.
 - View updates when workspace state changes.
 - Scene-scoped route actions are registered while the workspace view is visible.
 */

import SwiftUI
import RalphCore

@MainActor
struct WorkspaceView: View {
    @ObservedObject var workspace: Workspace
    @StateObject private var navigation: NavigationViewModel
    @State private var showingCommandPalette: Bool = false
    @State private var showingTaskCreation: Bool = false
    @State private var showingTaskDecompose: Bool = false
    @State private var showingOperationalHealth = false
    @State private var taskDecomposeContext = TaskDecomposeView.PresentationContext()
    @FocusedValue(\.workspaceWindowActions) private var workspaceWindowActions
    private let manager = WorkspaceManager.shared

    private var showErrorRecoveryBinding: Binding<Bool> {
        Binding(
            get: { workspace.diagnosticsState.showErrorRecovery },
            set: { workspace.diagnosticsState.showErrorRecovery = $0 }
        )
    }

    init(workspace: Workspace) {
        self._workspace = ObservedObject(wrappedValue: workspace)
        self._navigation = StateObject(
            wrappedValue: NavigationViewModel(
                workspaceID: workspace.id,
                issueSink: { issue in
                    workspace.updateNavigationPersistenceIssue(issue)
                }
            )
        )
    }

    private func navTitle(_ context: String) -> String {
        "\(workspace.projectDisplayName) · \(context)"
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $navigation.sidebarVisibility) {
            sidebarColumn()
                .navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 250)
        } content: {
            contentColumn()
                .navigationSplitViewColumnWidth(min: 320, ideal: 400, max: 600)
        } detail: {
            detailColumn()
                .navigationSplitViewColumnWidth(min: 450, ideal: 550, max: .infinity)
        }
        .frame(minWidth: 1200, minHeight: 640)
        .background(.clear)
        .focusedSceneValue(\.workspaceUIActions, focusedWorkspaceUIActions)
        .sheet(isPresented: showErrorRecoveryBinding) { errorRecoverySheet() }
        .sheet(isPresented: $showingCommandPalette) { commandPaletteSheet() }
        .sheet(isPresented: $showingTaskCreation) {
            TaskCreationView(workspace: workspace)
        }
        .sheet(isPresented: $showingTaskDecompose) {
            TaskDecomposeView(workspace: workspace, context: taskDecomposeContext)
        }
        .sheet(isPresented: $showingOperationalHealth) { operationalHealthSheet() }
        .onAppear {
            registerWorkspaceRouteActions()
        }
        .onDisappear {
            manager.unregisterWorkspaceRouteActions(for: workspace.id)
        }
    }

    // MARK: - Focused Actions

    private var focusedWorkspaceUIActions: WorkspaceUIActions {
        WorkspaceUIActions(
            showCommandPalette: { showingCommandPalette = true },
            navigateToSection: { section in
                navigation.navigate(to: section)
            },
            toggleSidebar: {
                navigation.toggleSidebar()
            },
            toggleTaskViewMode: {
                navigation.toggleTaskViewMode()
            },
            setTaskViewMode: { mode in
                navigation.setTaskViewMode(mode)
            },
            showTaskCreation: {
                showTaskCreation()
            },
            showTaskDecompose: { taskID in
                showTaskDecompose(selectedTaskID: taskID)
            },
            showTaskDetail: { taskID in
                showTaskDetail(taskID)
            },
            startWorkOnSelectedTask: {
                handleStartWork()
            }
        )
    }

    private func registerWorkspaceRouteActions() {
        manager.registerWorkspaceRouteActions(for: workspace.id) { route in
            switch route {
            case .showTaskCreation:
                showTaskCreation()
            case .showTaskDecompose(let taskID):
                showTaskDecompose(selectedTaskID: taskID)
            case .showTaskDetail(let taskID):
                showTaskDetail(taskID)
            }
        }
    }

    // MARK: - Columns

    @ViewBuilder
    private func sidebarColumn() -> some View {
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
    private func contentColumn() -> some View {
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
    private func detailColumn() -> some View {
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

    // MARK: - Section Content

    @ViewBuilder
    private func sectionContent() -> some View {
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
    private func queueContentColumn() -> some View {
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
    private func viewModeToolbar() -> some View {
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
    private func queueDetailColumn() -> some View {
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

    // MARK: - Sidebar Footer

    @ViewBuilder
    private func connectionStatusFooter() -> some View {
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

    // MARK: - Sheets

    @ViewBuilder
    private func errorRecoverySheet() -> some View {
        if let error = workspace.diagnosticsState.lastRecoveryError {
            ErrorRecoverySheet(
                error: error,
                workspace: workspace,
                onRetry: { handleRetry(for: error.operation) },
                onDismiss: { workspace.clearErrorRecovery() }
            )
        }
    }

    @ViewBuilder
    private func commandPaletteSheet() -> some View {
        CommandPaletteView(
            windowActions: workspaceWindowActions,
            workspaceUIActions: focusedWorkspaceUIActions
        )
            .frame(minWidth: 640, minHeight: 300)
    }

    @ViewBuilder
    private func operationalHealthSheet() -> some View {
        OperationalHealthSheet(
            workspaceName: workspace.projectDisplayName,
            summary: workspace.diagnosticsState.operationalSummary,
            issues: workspace.diagnosticsState.operationalIssues,
            watcherHealth: workspace.diagnosticsState.watcherHealth,
            cliHealthStatus: workspace.diagnosticsState.cliHealthStatus,
            onRepair: { handleRepairOperationalHealth() }
        )
    }

    // MARK: - Actions

    private func handleRetryConnection() {
        Task { @MainActor in
            _ = await workspace.checkHealth()
            if let newStatus = workspace.diagnosticsState.cliHealthStatus, newStatus.isAvailable {
                await workspace.loadTasks()
            }
        }
    }

    private func handleRepairOperationalHealth() {
        Task { @MainActor in
            await workspace.repairOperationalHealth()
        }
    }

    private func handleRetry(for operation: String) {
        workspace.clearErrorRecovery()

        switch operation {
        case "loadTasks":
            Task { @MainActor in await workspace.loadTasks() }
        case "loadGraphData":
            Task { @MainActor in await workspace.loadGraphData() }
        case "loadCLISpec":
            Task { @MainActor in await workspace.loadCLISpec() }
        case "run", "runVersion", "runInit":
            if workspace.runState.isRunning { workspace.cancel() }
            if navigation.selectedSection == .quickActions {
                workspace.runVersion()
            }
        default:
            Task { @MainActor in await workspace.loadTasks() }
        }
    }

    private func handleStartWork() {
        guard let taskID = navigation.selectedTaskID else { return }

        Task { @MainActor in
            do {
                try await workspace.updateTaskStatus(taskID: taskID, to: .doing)
            } catch {
                RalphLogger.shared.error("Failed to start work on task: \(error)", category: .workspace)
            }
        }
    }

    private func showTaskCreation() {
        navigation.selectedSection = .queue
        showingTaskCreation = true
    }

    private func showTaskDecompose(selectedTaskID: String?) {
        navigation.selectedSection = .queue
        taskDecomposeContext = TaskDecomposeView.PresentationContext(
            selectedTaskID: selectedTaskID ?? navigation.selectedTaskID
        )
        showingTaskDecompose = true
    }

    private func showTaskDetail(_ taskID: String) {
        navigation.selectedSection = .queue
        navigation.selectedTaskID = taskID
        navigation.selectedTaskIDs = [taskID]
    }
}

// MARK: - Empty Detail View

@MainActor
struct EmptyDetailView: View {
    let icon: String
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: icon)
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            Text(title)
                .font(.headline)

            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title). \(message)")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
    }
}
