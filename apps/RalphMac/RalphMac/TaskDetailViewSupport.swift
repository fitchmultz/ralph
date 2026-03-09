/**
 TaskDetailViewSupport

 Responsibilities:
 - Provide shared display helpers used by `TaskDetailView` for status/priority colors and metadata formatting.
 - Host reusable action-bar/alert view modifiers to keep `TaskDetailView` focused on core form layout and editing logic.

 Does not handle:
 - Task field editing logic or persistence (handled in `TaskDetailView` + `Workspace`).
 - Conflict resolution UI internals (delegated to `TaskConflictResolverView`).

 Invariants/assumptions callers must respect:
 - Intended for use only with `TaskDetailView` in the RalphMac app target.
 - Color/formatting helpers should remain presentation-only and side-effect free.
 */

import SwiftUI
import RalphCore

extension TaskDetailView {
    func statusColor(_ status: RalphTaskStatus) -> Color {
        switch status {
        case .draft:
            return .gray
        case .todo:
            return .blue
        case .doing:
            return .orange
        case .done:
            return .green
        case .rejected:
            return .red
        }
    }

    func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical:
            return .red
        case .high:
            return .orange
        case .medium:
            return .yellow
        case .low:
            return .gray
        }
    }

    func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    func formatDateForAccessibility(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .long
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    func isEditingNewArrayField(_ field: String) -> Bool {
        false
    }

    /// Builds the complete set of edges from all tasks in the workspace.
    /// Used for cycle detection in TaskRelationshipPicker.
    func buildExistingEdges() -> [GraphEdge] {
        var edges: [GraphEdge] = []

        for task in workspace.taskState.tasks {
            for depId in task.dependsOn ?? [] {
                edges.append(GraphEdge(from: task.id, to: depId, type: .dependency))
            }
            for blockedId in task.blocks ?? [] {
                edges.append(GraphEdge(from: task.id, to: blockedId, type: .blocks))
            }
            for relatedId in task.relatesTo ?? [] where task.id < relatedId {
                edges.append(GraphEdge(from: task.id, to: relatedId, type: .relatesTo))
            }
        }

        return edges
    }
}

private struct TaskDetailAlertsModifier: ViewModifier {
    @Binding var showingUnsavedChangesAlert: Bool
    @Binding var showingConflictAlert: Bool
    @Binding var showingConflictResolver: Bool
    @Binding var saveError: String?
    let draftTask: RalphTask
    let conflictedExternalTask: RalphTask?
    let onDiscard: () -> Void
    let onForceSave: () -> Void
    let onDiscardExternal: () -> Void
    let onMerge: (RalphTask) -> Void

    func body(content: Content) -> some View {
        content
            .alert("Discard Changes?", isPresented: $showingUnsavedChangesAlert) {
                Button("Discard", role: .destructive, action: onDiscard)
                Button("Keep Editing", role: .cancel) {}
            } message: {
                Text("You have unsaved changes. Are you sure you want to discard them and reset to the saved version?")
            }
            .alert("Save Error", isPresented: .constant(saveError != nil)) {
                Button("OK") { saveError = nil }
            } message: {
                Text(saveError ?? "")
            }
            .alert("External Changes Detected", isPresented: $showingConflictAlert) {
                Button("Overwrite External Changes", role: .destructive, action: onForceSave)
                Button("Discard My Changes", action: onDiscardExternal)
                Button("Resolve Conflicts...") { showingConflictResolver = true }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This task has been modified externally (via CLI or another window). Your changes conflict with the external changes.\n\nWhat would you like to do?")
            }
            .sheet(isPresented: $showingConflictResolver) {
                if let externalTask = conflictedExternalTask {
                    TaskConflictResolverView(
                        localTask: draftTask,
                        externalTask: externalTask,
                        onMerge: onMerge,
                        onCancel: { showingConflictResolver = false }
                    )
                }
            }
    }
}

private struct TaskDetailActionBarModifier: ViewModifier {
    let hasConflict: Bool
    let isSaving: Bool
    let saveSuccess: Bool
    let hasChanges: Bool
    let onReset: () -> Void
    let onSave: () -> Void

    func body(content: Content) -> some View {
        content
            .safeAreaInset(edge: .bottom, spacing: 0) {
                HStack(spacing: 10) {
                    if hasConflict {
                        Label("External changes detected", systemImage: "exclamationmark.triangle.fill")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    } else if saveSuccess {
                        Label("Saved", systemImage: "checkmark.circle.fill")
                            .font(.caption)
                            .foregroundStyle(.green)
                            .accessibilityIdentifier("task-detail-save-success")
                    } else if hasChanges {
                        Label("Unsaved changes", systemImage: "circle.fill")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        Text("No changes")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }

                    Spacer(minLength: 16)

                    if isSaving {
                        ProgressView()
                            .controlSize(.small)
                            .padding(.trailing, 4)
                    }

                    Button("Reset", action: onReset)
                        .buttonStyle(.bordered)
                        .disabled(!hasChanges || isSaving)
                        .accessibilityLabel("Reset changes")
                        .accessibilityHint("Discard all changes and revert to the saved version")

                    Button("Save", action: onSave)
                        .buttonStyle(.borderedProminent)
                        .disabled(!hasChanges || isSaving)
                        .keyboardShortcut("s", modifiers: .command)
                        .accessibilityLabel("Save changes")
                        .accessibilityHint("Save all changes to this task")
                        .accessibilityIdentifier("task-detail-save-button")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(.ultraThinMaterial)
                .overlay(alignment: .top) {
                    Divider()
                }
            }
    }
}

extension View {
    func withTaskDetailActionBar(
        hasConflict: Bool,
        isSaving: Bool,
        saveSuccess: Bool,
        hasChanges: Bool,
        onReset: @escaping () -> Void,
        onSave: @escaping () -> Void
    ) -> some View {
        modifier(TaskDetailActionBarModifier(
            hasConflict: hasConflict,
            isSaving: isSaving,
            saveSuccess: saveSuccess,
            hasChanges: hasChanges,
            onReset: onReset,
            onSave: onSave
        ))
    }

    func withTaskDetailAlerts(
        showingUnsavedChangesAlert: Binding<Bool>,
        showingConflictAlert: Binding<Bool>,
        showingConflictResolver: Binding<Bool>,
        saveError: Binding<String?>,
        draftTask: RalphTask,
        conflictedExternalTask: RalphTask?,
        onDiscard: @escaping () -> Void,
        onForceSave: @escaping () -> Void,
        onDiscardExternal: @escaping () -> Void,
        onMerge: @escaping (RalphTask) -> Void
    ) -> some View {
        return modifier(TaskDetailAlertsModifier(
            showingUnsavedChangesAlert: showingUnsavedChangesAlert,
            showingConflictAlert: showingConflictAlert,
            showingConflictResolver: showingConflictResolver,
            saveError: saveError,
            draftTask: draftTask,
            conflictedExternalTask: conflictedExternalTask,
            onDiscard: onDiscard,
            onForceSave: onForceSave,
            onDiscardExternal: onDiscardExternal,
            onMerge: onMerge
        ))
    }
}
