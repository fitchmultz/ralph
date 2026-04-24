/**
 TaskDetailView

 Purpose:
 - Display a comprehensive form for viewing and editing all task fields.

 Responsibilities:
 - Display a comprehensive form for viewing and editing all task fields.
 - Support inline editing with proper form controls (pickers, text editors, tag editors).
 - Integrate with Workspace to persist changes via CLI.
 - Display as inline detail view within NavigationSplitView (not as sheet).

 Does not handle:
 - Task creation (see task builder workflow).
 - Batch operations on multiple tasks.
 - Navigation or dismissal (handled by parent NavigationSplitView).
 - Execution overrides UI (delegated to TaskExecutionOverridesSection).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Task is passed in and copied to @State for editing.
 - Changes are only persisted when user explicitly saves.
 - onTaskUpdated callback is called after successful save.
 - View is displayed as detail column in NavigationSplitView.
 */

import SwiftUI
import RalphCore

@MainActor
struct TaskDetailView: View {
    @ObservedObject var workspace: Workspace
    let task: RalphTask
    var onTaskUpdated: ((RalphTask) -> Void)? = nil
    @StateObject private var editorState: TaskDetailEditorState

    init(workspace: Workspace, task: RalphTask, onTaskUpdated: ((RalphTask) -> Void)? = nil) {
        self.workspace = workspace
        self.task = task
        self.onTaskUpdated = onTaskUpdated
        self._editorState = StateObject(wrappedValue: TaskDetailEditorState(task: task))
    }

    var body: some View {
        contentView
            .withTaskDetailAlerts(
                showingUnsavedChangesAlert: $editorState.showingUnsavedChangesAlert,
                showingConflictAlert: $editorState.showingConflictAlert,
                showingConflictResolver: $editorState.showingConflictResolver,
                saveError: $editorState.saveError,
                draftTask: editorState.draftTask,
                conflictedExternalTask: editorState.conflictedExternalTask,
                onDiscard: { editorState.discardChanges() },
                onForceSave: { editorState.saveChanges(in: workspace, onTaskUpdated: onTaskUpdated, force: true) },
                onDiscardExternal: { editorState.discardLocalChangesAfterConflict() },
                onMerge: { mergedTask in
                    editorState.applyMergedTask(mergedTask)
                }
            )
            .withTaskDetailActionBar(
                hasConflict: editorState.hasConflict,
                isSaving: editorState.isSaving,
                saveSuccess: editorState.saveSuccess,
                hasChanges: editorState.hasChanges,
                onReset: { editorState.showingUnsavedChangesAlert = true },
                onSave: { editorState.saveChanges(in: workspace, onTaskUpdated: onTaskUpdated) }
            )
            .onChange(of: task.id) { _, _ in
                editorState.resetForLoadedTask(task)
            }
            .onChange(of: task.updatedAt) { _, _ in
                editorState.synchronizeIfNoLocalChanges(with: task)
            }
            .task(id: workspace.taskState.lastQueueRefreshEvent?.id) {
                guard workspace.taskState.lastQueueRefreshEvent?.source == .externalFileChange else { return }
                editorState.checkForExternalChanges(in: workspace, taskID: task.id)
            }
    }

    private var contentView: some View {
        ScrollView {
            TaskDetailFormSections(
                draftTask: $editorState.draftTask,
                workspace: workspace,
                taskID: task.id,
                mutateTaskAgent: mutateTaskAgent
            )
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(editorState.draftTask.title)
        .navigationSubtitle(task.id)
    }

    private func mutateTaskAgent(_ mutate: (inout RalphTaskAgent) -> Void) {
        var agent = editorState.draftTask.agent ?? RalphTaskAgent()
        mutate(&agent)
        editorState.draftTask.agent = RalphTaskAgent.normalizedOverride(agent)
    }
}

// Preview
#Preview {
    TaskDetailView(
        workspace: PreviewWorkspaceSupport.makeWorkspace(label: "task-detail"),
        task: RalphTask(
            id: "RQ-0001",
            status: .todo,
            title: "Sample Task",
            description: "This is a sample task description.",
            priority: .high,
            tags: ["swift", "ui"],
            scope: ["apps/RalphMac/TaskDetailView.swift"],
            createdAt: Date(),
            updatedAt: Date()
        )
    )
}
