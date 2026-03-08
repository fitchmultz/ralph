/**
 KanbanBoardView

 Responsibilities:
 - Display a horizontal Kanban board with all status columns.
 - Handle drag-and-drop between columns to change task status.
 - Provide "Start Work" button for quick status transitions.
 - Coordinate with Workspace for status updates and task reloads.

 Does not handle:
 - Task editing (delegates to TaskDetailView via navigation).
 - Task creation (handled by parent view).
 - Direct CLI execution (delegates to Workspace).

 Invariants/assumptions callers must respect:
 - Workspace is injected and provides task data.
 - selectedTaskID binding is used for navigation.
 - Status changes are persisted via CLI calls.
 */

import SwiftUI
import RalphCore

@MainActor
struct KanbanBoardView: View {
    @ObservedObject var workspace: Workspace
    @Binding var selectedTaskID: String?
    let showTaskDetail: (String) -> Void

    @State private var isUpdating = false
    @State private var updateError: String?
    @State private var recentlyChangedTaskIDs: Set<String> = []
    
    // MARK: - Keyboard Navigation State
    @State private var focusedColumnStatus: RalphTaskStatus = .todo
    @State private var focusedTaskID: String?

    var body: some View {
        let presentation = workspace.taskPresentation()

        ScrollView(.horizontal) {
            HStack(spacing: 16) {
                ForEach(RalphTaskStatus.allCases, id: \.self) { status in
                    let statusTasks = presentation.tasksByStatus[status, default: []]

                    KanbanColumnView(
                        status: status,
                        tasks: statusTasks,
                        isTaskBlocked: { task in workspace.isTaskBlocked(task) },
                        isTaskOverdue: { task in workspace.isTaskOverdue(task) },
                        onTaskDrop: { taskID in
                            handleTaskDrop(taskID: taskID, to: status)
                        },
                        onTaskSelect: { taskID in
                            selectedTaskID = taskID
                            focusedTaskID = taskID
                            focusedColumnStatus = status
                        },
                        highlightedTaskIDs: recentlyChangedTaskIDs,
                        focusedTaskID: focusedTaskID,
                        isFocusedColumn: focusedColumnStatus == status
                    )
                    // MARK: - Accessibility
                    // Column accessibility label with status and task count
                    .accessibilityLabel("\(status.displayName) column, \(statusTasks.count) tasks")
                }
            }
            .padding(20)
        }
        // MARK: - Keyboard Navigation Handlers
        .focusable()
        .onKeyPress(.leftArrow) {
            navigateColumn(direction: -1)
            return .handled
        }
        .onKeyPress(.rightArrow) {
            navigateColumn(direction: 1)
            return .handled
        }
        .onKeyPress(.upArrow) {
            navigateKanbanTask(direction: -1)
            return .handled
        }
        .onKeyPress(.downArrow) {
            navigateKanbanTask(direction: 1)
            return .handled
        }
        .onKeyPress(.return) {
            if let taskID = focusedTaskID {
                selectedTaskID = taskID
                showTaskDetail(taskID)
            }
            return .handled
        }
        .onKeyPress(.space) {
            // Space key opens task detail (same as Enter for quick access)
            if let taskID = focusedTaskID {
                selectedTaskID = taskID
                showTaskDetail(taskID)
            }
            return .handled
        }
        // MARK: - Accessibility
        // Board-level accessibility for VoiceOver users
        // Provides context about the Kanban board layout and navigation
        .accessibilityLabel("Kanban board")
        .accessibilityHint("Horizontal scroll to view columns. Shows tasks grouped by status.")
        .background(.clear)
        .overlay {
            if isUpdating {
                ProgressView()
                    .scaleEffect(1.2)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(.ultraThinMaterial)
            }
        }
        .scrollIndicators(.automatic)
        .alert("Update Error", isPresented: .constant(updateError != nil)) {
            Button("OK") { updateError = nil }
        } message: {
            Text(updateError ?? "")
        }
        .task { @MainActor in
            await workspace.loadTasks()
        }
        .task(id: workspace.lastQueueRefreshEvent?.id) {
            await handleQueueRefreshEvent()
        }
    }

    private func handleTaskDrop(taskID: String, to status: RalphTaskStatus) {
        // Find the task
        guard let task = workspace.tasks.first(where: { $0.id == taskID }) else { return }

        // Skip if status hasn't changed
        guard task.status != status else { return }

        isUpdating = true

        Task { @MainActor in
            do {
                try await workspace.updateTaskStatus(taskID: taskID, to: status)
                isUpdating = false
            } catch {
                isUpdating = false
                updateError = error.localizedDescription
            }
        }
    }

    private func handleQueueRefreshEvent() async {
        guard let refreshEvent = workspace.lastQueueRefreshEvent,
              refreshEvent.source == .externalFileChange else {
            return
        }

        withAnimation(.easeInOut(duration: 0.3)) {
            recentlyChangedTaskIDs = refreshEvent.highlightedTaskIDs
        }

        do {
            try await Task.sleep(for: .milliseconds(2000))
            withAnimation(.easeInOut(duration: 0.5)) {
                recentlyChangedTaskIDs.removeAll()
            }
        } catch {
            return
        }
    }
    
    // MARK: - Keyboard Navigation
    
    private func navigateColumn(direction: Int) {
        let statuses = RalphTaskStatus.allCases
        guard let currentIndex = statuses.firstIndex(of: focusedColumnStatus) else { return }
        
        let newIndex = currentIndex + direction
        guard newIndex >= 0 && newIndex < statuses.count else { return }
        
        let newStatus = statuses[newIndex]
        withAnimation(.easeInOut(duration: 0.2)) {
            focusedColumnStatus = newStatus
            // Select first task in new column if exists
            let columnTasks = workspace.taskPresentation().tasksByStatus[newStatus, default: []]
            focusedTaskID = columnTasks.first?.id
            selectedTaskID = focusedTaskID
        }
    }
    
    private func navigateKanbanTask(direction: Int) {
        let columnTasks = workspace.taskPresentation().tasksByStatus[focusedColumnStatus, default: []]
        
        guard let currentID = focusedTaskID,
              let currentIndex = columnTasks.firstIndex(where: { $0.id == currentID }) else {
            // No current focus, select first in column
            if let first = columnTasks.first {
                withAnimation(.easeInOut(duration: 0.15)) {
                    focusedTaskID = first.id
                    selectedTaskID = first.id
                }
            }
            return
        }
        
        let newIndex = currentIndex + direction
        guard newIndex >= 0 && newIndex < columnTasks.count else { return }
        
        let newTask = columnTasks[newIndex]
        withAnimation(.easeInOut(duration: 0.15)) {
            focusedTaskID = newTask.id
            selectedTaskID = newTask.id
        }
    }
}

#Preview {
    struct PreviewWrapper: View {
        @State private var selectedTaskID: String?

        var body: some View {
            KanbanBoardView(
                workspace: previewWorkspace(),
                selectedTaskID: $selectedTaskID,
                showTaskDetail: { _ in }
            )
        }

        func previewWorkspace() -> Workspace {
            let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
            // Note: In real usage, tasks would be loaded from the CLI
            return workspace
        }
    }

    return PreviewWrapper()
}
