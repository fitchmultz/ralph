/**
 TaskDetailEditorState

 Purpose:
 - Own mutable editing state for `TaskDetailView`, including draft/baseline tracking.

 Responsibilities:
 - Own mutable editing state for `TaskDetailView`, including draft/baseline tracking.
 - Coordinate save, conflict, and refresh handling against `Workspace`.
 - Centralize transient save-success feedback with cancellable state transitions.

 Does not handle:
 - Rendering the task detail form.
 - Defining workspace persistence behavior beyond invoking existing APIs.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - The state object is main-actor confined because it mutates SwiftUI-observed properties.
 - The owning view must forward task identity and queue-refresh changes into this state owner.
 */

import Foundation
import RalphCore
import SwiftUI

@MainActor
final class TaskDetailEditorState: ObservableObject {
    @Published var draftTask: RalphTask
    @Published private(set) var baselineTask: RalphTask
    @Published private(set) var originalUpdatedAt: Date?
    @Published var isSaving = false
    @Published var saveError: String?
    @Published var showingUnsavedChangesAlert = false
    @Published var saveSuccess = false
    @Published var hasConflict = false
    @Published var conflictedExternalTask: RalphTask?
    @Published var showingConflictAlert = false
    @Published var showingConflictResolver = false

    private var saveSuccessResetTask: Task<Void, Never>?

    init(task: RalphTask) {
        self.draftTask = task
        self.baselineTask = task
        self.originalUpdatedAt = task.updatedAt
    }

    deinit {
        saveSuccessResetTask?.cancel()
    }

    var hasChanges: Bool {
        draftTask != baselineTask
    }

    func resetForLoadedTask(_ task: RalphTask) {
        draftTask = task
        baselineTask = task
        originalUpdatedAt = task.updatedAt
        hasConflict = false
        conflictedExternalTask = nil
        saveSuccess = false
    }

    func synchronizeIfNoLocalChanges(with task: RalphTask) {
        guard !hasChanges else { return }
        resetForLoadedTask(task)
    }

    func discardChanges() {
        draftTask = baselineTask
    }

    func applyMergedTask(_ task: RalphTask) {
        resetForLoadedTask(task)
        showingConflictResolver = false
    }

    func discardLocalChangesAfterConflict() {
        guard let conflictedExternalTask else { return }
        resetForLoadedTask(conflictedExternalTask)
    }

    func saveChanges(
        in workspace: Workspace,
        onTaskUpdated: ((RalphTask) -> Void)?,
        force: Bool = false
    ) {
        if !force && hasConflict {
            showingConflictAlert = true
            return
        }

        isSaving = true
        saveError = nil
        saveSuccess = false

        Task { @MainActor in
            do {
                try await workspace.updateTask(
                    from: baselineTask,
                    to: draftTask,
                    originalUpdatedAt: force ? nil : originalUpdatedAt
                )
                let persistedTask = workspace.taskState.tasks.first(where: { $0.id == draftTask.id }) ?? draftTask
                isSaving = false
                hasConflict = false
                conflictedExternalTask = nil
                draftTask = persistedTask
                baselineTask = persistedTask
                originalUpdatedAt = persistedTask.updatedAt
                onTaskUpdated?(persistedTask)
                presentSaveSuccess()
            } catch let error as Workspace.WorkspaceError {
                isSaving = false
                if case .taskConflict(let currentTask) = error {
                    hasConflict = true
                    conflictedExternalTask = currentTask
                    showingConflictAlert = true
                } else {
                    saveError = error.localizedDescription
                }
            } catch {
                isSaving = false
                saveError = error.localizedDescription
            }
        }
    }

    func checkForExternalChanges(in workspace: Workspace, taskID: String) {
        guard !isSaving else { return }

        guard hasChanges else {
            if let currentTask = workspace.taskState.tasks.first(where: { $0.id == taskID }) {
                resetForLoadedTask(currentTask)
            }
            return
        }

        if let externalTask = workspace.checkForConflict(
            taskID: taskID,
            originalUpdatedAt: originalUpdatedAt
        ) {
            hasConflict = true
            conflictedExternalTask = externalTask
            showingConflictAlert = true
        }
    }

    private func presentSaveSuccess() {
        saveSuccessResetTask?.cancel()
        saveSuccess = true
        saveSuccessResetTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await Task.sleep(for: .seconds(2))
                withAnimation(.easeInOut) {
                    self.saveSuccess = false
                }
            } catch {
                return
            }
        }
    }
}
