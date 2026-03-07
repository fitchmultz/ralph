/**
 MenuBarContentView

 Responsibilities:
 - Render the menu bar dropdown content.
 - Display current/next task information.
 - Provide quick action buttons.
 - Show task counts by status.
 - List recent tasks for quick access.

 Does not handle:
 - Menu bar icon rendering (see MenuBarIconView).
 - Direct business logic (delegates to Workspace/MenuBarManager).

 Invariants/assumptions:
 - Observes WorkspaceManager.shared and MenuBarManager.shared.
 - Uses NotificationCenter to communicate with main app.
 - Must run on MainActor.
 */

import SwiftUI
import RalphCore

/// Main content view for the menu bar extra dropdown menu.
struct MenuBarContentView: View {
    @ObservedObject private var manager = WorkspaceManager.shared
    @ObservedObject private var menuBarManager = MenuBarManager.shared
    
    var body: some View {
        if let workspace = manager.workspaces.first {
            menuContent(for: workspace)
        } else {
            emptyContent
        }
    }
    
    // MARK: - Main Content
    
    private func menuContent(for workspace: Workspace) -> some View {
        Group {
            // Current/Next Task Section
            nextTaskSection(for: workspace)
            
            Divider()
            
            // Task Counts Section
            taskCountsSection(for: workspace)
            
            Divider()
            
            // Quick Actions Section
            quickActionsSection(for: workspace)
            
            // Recent Tasks Section (if enabled)
            if menuBarManager.showRecentTasks {
                recentTasksSection(for: workspace)
            }
            
            Divider()
            
            // App Actions
            appActionsSection()
        }
    }
    
    // MARK: - Sections
    
    /// Shows the next task to work on, or a "no tasks" message
    @ViewBuilder
    private func nextTaskSection(for workspace: Workspace) -> some View {
        if let nextTask = workspace.nextTask() {
            VStack(alignment: .leading, spacing: 6) {
                Label("Next Task", systemImage: "arrow.right.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                
                Text(nextTask.title)
                    .font(.system(.body, design: .rounded))
                    .lineLimit(2)
                
                HStack(spacing: 8) {
                    StatusBadge(status: nextTask.status)
                    PriorityBadge(priority: nextTask.priority)
                }
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
            .onTapGesture {
                showTaskDetail(nextTask.id, workspaceID: workspace.id)
            }
        } else {
            VStack(alignment: .leading, spacing: 6) {
                Label("No Tasks", systemImage: "checkmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
                
                if workspace.tasks.isEmpty {
                    Text("No tasks in queue")
                        .font(.body)
                        .foregroundStyle(.secondary)
                } else {
                    Text("All tasks completed!")
                        .font(.body)
                        .foregroundStyle(.green)
                }
            }
            .padding(.vertical, 4)
        }
    }
    
    /// Shows task counts by status (Todo/Doing/Done)
    @ViewBuilder
    private func taskCountsSection(for workspace: Workspace) -> some View {
        let counts = taskCounts(from: workspace.tasks)
        
        HStack(spacing: 12) {
            CountBadge(count: counts.todo, label: "Todo", color: .blue)
            CountBadge(count: counts.doing, label: "Doing", color: .orange)
            CountBadge(count: counts.done, label: "Done", color: .green)
        }
        .padding(.vertical, 4)
    }
    
    /// Quick action buttons (Run Next, Quick Add)
    @ViewBuilder
    private func quickActionsSection(for workspace: Workspace) -> some View {
        Group {
            Button("Run Next Task") {
                workspace.runNextTask()
            }
            .disabled(workspace.nextTask() == nil || workspace.isRunning)
            
            Button("Quick Add Task...") {
                NotificationCenter.default.post(
                    name: .quickAddTaskFromMenuBar,
                    object: nil
                )
                activateMainApp()
            }

            Button("Decompose Task...") {
                NotificationCenter.default.post(
                    name: .showTaskDecompose,
                    object: nil
                )
                activateMainApp()
            }
        }
    }
    
    /// Recent tasks list for quick access
    @ViewBuilder
    private func recentTasksSection(for workspace: Workspace) -> some View {
        let recentTasks = getRecentTasks(from: workspace.tasks, limit: menuBarManager.maxRecentTasks)
        
        if !recentTasks.isEmpty {
            Divider()
            
            Text("Recent Tasks")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.top, 4)
            
            ForEach(recentTasks) { task in
                Button(task.title) {
                    showTaskDetail(task.id, workspaceID: workspace.id)
                }
            }
        }
    }
    
    /// App-level actions (Open Ralph, Settings, Quit)
    @ViewBuilder
    private func appActionsSection() -> some View {
        Group {
            Button("Open Ralph") {
                activateMainApp()
            }
            
            Button("Settings...") {
                activateMainApp()
                NotificationCenter.default.post(name: .showRalphSettings, object: nil)
            }
            .keyboardShortcut(",", modifiers: .command)
            
            Toggle("Show in Menu Bar", isOn: $menuBarManager.isMenuBarExtraVisible)
            
            Divider()
            
            Button("Quit") {
                NSApp.terminate(nil)
            }
        }
    }
    
    /// Empty state when no workspace is available
    @ViewBuilder
    private var emptyContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("No workspace available")
                .font(.body)
            
            Button("Open Ralph") {
                activateMainApp()
            }
            
            Divider()
            
            Button("Quit") {
                NSApp.terminate(nil)
            }
        }
        .padding(.vertical, 4)
    }
    
    // MARK: - Helpers
    
    /// Calculate task counts by status
    private func taskCounts(from tasks: [RalphTask]) -> (todo: Int, doing: Int, done: Int) {
        (
            todo: tasks.filter { $0.status == .todo }.count,
            doing: tasks.filter { $0.status == .doing }.count,
            done: tasks.filter { $0.status == .done }.count
        )
    }
    
    /// Get recent tasks sorted by updatedAt
    private func getRecentTasks(from tasks: [RalphTask], limit: Int) -> [RalphTask] {
        tasks
            .filter { $0.updatedAt != nil }
            .sorted { ($0.updatedAt ?? .distantPast) > ($1.updatedAt ?? .distantPast) }
            .prefix(limit)
            .map { $0 }
    }
    
    /// Activate the main app and show task detail
    private func showTaskDetail(_ taskID: String, workspaceID: UUID) {
        // Post notification to show task detail
        NotificationCenter.default.post(
            name: .showTaskDetailFromMenuBar,
            object: taskID
        )
        
        // Activate the workspace
        NotificationCenter.default.post(
            name: .activateWorkspace,
            object: workspaceID
        )
        
        // Bring app to front
        activateMainApp()
    }
    
    /// Activate the main app window
    private func activateMainApp() {
        NSApp.activate(ignoringOtherApps: true)
        
        // Post notification to show main app
        NotificationCenter.default.post(
            name: .showMainAppFromMenuBar,
            object: nil
        )
    }
}

// MARK: - Supporting Views

/// Badge showing task status
struct StatusBadge: View {
    let status: RalphTaskStatus
    
    var body: some View {
        Text(status.displayName)
            .font(.caption2)
            .font(.body.weight(.medium))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(backgroundColor)
            .foregroundStyle(.white)
            .clipShape(Capsule())
    }
    
    private var backgroundColor: Color {
        switch status {
        case .draft: return .gray
        case .todo: return .blue
        case .doing: return .orange
        case .done: return .green
        case .rejected: return .red
        }
    }
}

/// Badge showing task priority
struct PriorityBadge: View {
    let priority: RalphTaskPriority
    
    var body: some View {
        HStack(spacing: 2) {
            Circle()
                .fill(priorityColor)
                .frame(width: 6, height: 6)
            Text(priority.displayName)
                .font(.caption2)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(.quaternary.opacity(0.3))
        .clipShape(Capsule())
    }
    
    private var priorityColor: Color {
        switch priority {
        case .critical: return .red
        case .high: return .orange
        case .medium: return .yellow
        case .low: return .green
        }
    }
}

/// Badge showing a count with a label and color
struct CountBadge: View {
    let count: Int
    let label: String
    let color: Color
    
    var body: some View {
        HStack(spacing: 4) {
            Text("\(count)")
                .font(.system(.body, design: .rounded))
                .font(.body.weight(.semibold))
                .foregroundStyle(color)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(color.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

#Preview {
    MenuBarContentView()
}
