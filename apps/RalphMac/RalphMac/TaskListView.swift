/**
 TaskListView

 Purpose:
 - Display a rich, sortable, filterable list of tasks from the Ralph queue.

 Responsibilities:
 - Display a rich, sortable, filterable list of tasks from the Ralph queue.
 - Show task metadata with visual indicators (status badges, priority dots, tag chips).
 - Provide search, filtering, and sorting controls.
 - Display "What's Next" section highlighting the next todo task.
 - Support two-way binding for task selection to integrate with NavigationSplitView.

 Does not handle:
 - Task editing (see TaskDetailView - now shown in detail column).
 - Task creation (see TaskCreationView).
 - Direct CLI execution (delegates to Workspace).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - selectedTaskID is bound to external navigation state.
 - Tasks are loaded via workspace.loadTasks() before display.
 */

import SwiftUI
import RalphCore

@MainActor
struct TaskListView: View {
    @ObservedObject var workspace: Workspace
    @Binding var selectedTaskID: String?
    @Binding var selectedTaskIDs: Set<String>
    let showTaskCreation: () -> Void
    let showTaskDecompose: (String?) -> Void
    let showTaskDetail: (String) -> Void
    @StateObject private var transientState = TaskListTransientState()
    @FocusState private var focusedTaskID: String?

    var body: some View {
        VStack(spacing: 0) {
            if let nextTask = workspace.nextTask() {
                TaskListWhatsNextCard(task: nextTask) {
                    handleTaskSelection(taskID: nextTask.id, modifierFlags: NSEvent.modifierFlags)
                }
                    .padding(.horizontal, 16)
                    .padding(.top, 16)
                    .padding(.bottom, 12)
            }

            TaskListFilterControls(workspace: workspace) {
                Task { @MainActor in
                    await workspace.loadTasks()
                }
            }
                .padding(.horizontal, 16)
                .padding(.bottom, 12)

            TaskListExternalUpdateBanner(isVisible: transientState.isExternalUpdateBannerVisible)
                .padding(.horizontal, 16)
                .padding(.top, transientState.isExternalUpdateBannerVisible ? 8 : 0)

            TaskListContent(
                workspace: workspace,
                selectedTaskIDs: $selectedTaskIDs,
                selectedTaskID: selectedTaskID,
                focusedTaskID: $focusedTaskID,
                highlightedTaskIDs: transientState.highlightedTaskIDs,
                onTaskTap: { taskID in
                    handleTaskSelection(taskID: taskID, modifierFlags: NSEvent.modifierFlags)
                },
                onTaskDecompose: { taskID in
                    showTaskDecompose(taskID)
                },
                onOpenSelectedTask: openFocusedTaskIfPossible,
                onNavigate: navigateTask(direction:tasks:)
            )
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
        }
        .background(.clear)
        .task { @MainActor in
            await Task.yield()
            await workspace.loadTasks()
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button(action: showTaskCreation) {
                    Label("New Task", systemImage: "plus")
                }
                .accessibilityIdentifier("new-task-toolbar-button")

                Button(action: {
                    showTaskDecompose(selectedTaskID)
                }) {
                    Label("Decompose", systemImage: "square.split.2x2")
                }
                .disabled(selectedTaskID == nil && workspace.taskState.tasks.isEmpty)
                .accessibilityIdentifier("task-decompose-toolbar-button")
            }
            
            // Add bulk actions button when multi-select is active
            if selectedTaskIDs.count > 1 {
                ToolbarItem(placement: .cancellationAction) {
                    Button(action: { transientState.showingBulkActions = true }) {
                        Label("Bulk Actions", systemImage: "rectangle.stack")
                    }
                    .help("Perform bulk actions on \(selectedTaskIDs.count) selected tasks")
                }
            }
        }
        .sheet(isPresented: $transientState.showingBulkActions) {
            BulkActionsView(
                workspace: workspace,
                selectedTaskIDs: selectedTaskIDs,
                onCompletion: {
                    transientState.clearSelection(
                        selectedTaskIDs: $selectedTaskIDs,
                        selectedTaskID: $selectedTaskID
                    )
                }
            )
        }
        .task(id: workspace.taskState.lastQueueRefreshEvent?.id) {
            transientState.handleQueueRefreshEvent(workspace.taskState.lastQueueRefreshEvent)
        }
        .onChange(of: selectedTaskIDs) { _, newSelection in
            syncPrimarySelection(with: newSelection)
        }
        .onChange(of: workspace.identityState.retargetRevision) { _, _ in
            focusedTaskID = nil
            transientState.resetForRepositoryRetarget()
        }
    }

    private func handleTaskSelection(taskID: String, modifierFlags: NSEvent.ModifierFlags) {
        if modifierFlags.contains(.command) {
            if selectedTaskIDs.contains(taskID) {
                selectedTaskIDs.remove(taskID)
                if selectedTaskID == taskID {
                    selectedTaskID = selectedTaskIDs.first
                }
                if selectedTaskIDs.isEmpty {
                    selectedTaskID = nil
                }
            } else {
                selectedTaskIDs.insert(taskID)
                selectedTaskID = taskID
            }
        } else {
            // Normal click: single selection
            selectedTaskID = taskID
            selectedTaskIDs = [taskID]
        }
    }

    private func openFocusedTaskIfPossible() {
        if let taskID = focusedTaskID ?? selectedTaskID {
            selectedTaskID = taskID
            selectedTaskIDs = [taskID]
            showTaskDetail(taskID)
        }
    }

    private func navigateTask(direction: Int, tasks: [RalphTask]) {
        let currentID = focusedTaskID ?? selectedTaskID

        guard let currentIndex = tasks.firstIndex(where: { $0.id == currentID }) else {
            if let first = tasks.first {
                withAnimation(.easeInOut(duration: 0.15)) {
                    focusedTaskID = first.id
                    selectedTaskID = first.id
                    selectedTaskIDs = [first.id]
                }
            }
            return
        }

        let newIndex = currentIndex + direction
        guard newIndex >= 0 && newIndex < tasks.count else { return }

        let newTask = tasks[newIndex]
        withAnimation(.easeInOut(duration: 0.15)) {
            focusedTaskID = newTask.id
            selectedTaskID = newTask.id
            selectedTaskIDs = [newTask.id]
        }
    }

    private func syncPrimarySelection(with selection: Set<String>) {
        if selection.isEmpty {
            selectedTaskID = nil
            focusedTaskID = nil
            return
        }

        if let currentID = selectedTaskID, selection.contains(currentID) {
            focusedTaskID = currentID
            return
        }

        let nextPrimaryTaskID = workspace.taskPresentation().orderedTaskIDs
            .first(where: selection.contains)
            ?? selection.sorted().first

        selectedTaskID = nextPrimaryTaskID
        focusedTaskID = nextPrimaryTaskID
    }
}
