/**
 TaskDetailView

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
    private enum AccessibilityID {
        static let titleField = "task-detail-title-field"
    }

    @ObservedObject var workspace: Workspace
    let task: RalphTask
    var onTaskUpdated: ((RalphTask) -> Void)? = nil

    // State for mutable copy of task being edited
    @State private var draftTask: RalphTask
    @State private var baselineTask: RalphTask
    @State private var isSaving = false
    @State private var saveError: String?
    @State private var showingUnsavedChangesAlert = false
    @State private var saveSuccess = false

    // State for conflict detection (optimistic locking)
    @State private var originalUpdatedAt: Date?
    @State private var hasConflict = false
    @State private var conflictedExternalTask: RalphTask?
    @State private var showingConflictAlert = false
    @State private var showingConflictResolver = false

    init(workspace: Workspace, task: RalphTask, onTaskUpdated: ((RalphTask) -> Void)? = nil) {
        self.workspace = workspace
        self.task = task
        self.onTaskUpdated = onTaskUpdated
        self._draftTask = State(initialValue: task)
        self._baselineTask = State(initialValue: task)
        self._originalUpdatedAt = State(initialValue: task.updatedAt)
    }

    var body: some View {
        contentView
            .withTaskDetailAlerts(
                showingUnsavedChangesAlert: $showingUnsavedChangesAlert,
                showingConflictAlert: $showingConflictAlert,
                showingConflictResolver: $showingConflictResolver,
                saveError: $saveError,
                draftTask: draftTask,
                conflictedExternalTask: conflictedExternalTask,
                onDiscard: { draftTask = baselineTask },
                onForceSave: { saveChanges(force: true) },
                onDiscardExternal: { discardLocalChanges() },
                onMerge: { mergedTask in
                    self.draftTask = mergedTask
                    self.baselineTask = mergedTask
                    self.originalUpdatedAt = mergedTask.updatedAt
                    self.hasConflict = false
                    self.conflictedExternalTask = nil
                    self.showingConflictResolver = false
                }
            )
            .withTaskDetailActionBar(
                hasConflict: hasConflict,
                isSaving: isSaving,
                saveSuccess: saveSuccess,
                hasChanges: hasChanges(),
                onReset: { showingUnsavedChangesAlert = true },
                onSave: { saveChanges() }
            )
            .onChange(of: task.id) { _, _ in
                // Task changed, reset draft and conflict state
                draftTask = task
                baselineTask = task
                originalUpdatedAt = task.updatedAt
                hasConflict = false
                conflictedExternalTask = nil
                saveSuccess = false
            }
            .onChange(of: task.updatedAt) { _, _ in
                // Keep baseline in sync when parent task refreshes and no local edits are pending.
                guard !hasChanges() else { return }
                draftTask = task
                baselineTask = task
                originalUpdatedAt = task.updatedAt
                hasConflict = false
                conflictedExternalTask = nil
            }
            .onReceive(NotificationCenter.default.publisher(for: .queueFilesExternallyChanged)) { _ in
                checkForExternalChanges()
            }
    }

    private var contentView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                basicInfoSection()
                statusSection()
                timeTrackingSection()
                TaskExecutionOverridesSection(
                    draftTask: $draftTask,
                    workspace: workspace,
                    mutateTaskAgent: mutateTaskAgent
                )
                tagsSection()
                contentSections()
                relationshipsSection()
                metadataSection()
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(draftTask.title)
        .navigationSubtitle(task.id)
    }

    // MARK: - Sections

    @ViewBuilder
    private func basicInfoSection() -> some View {
        glassGroupBox("Basic Information") {
            VStack(alignment: .leading, spacing: 16) {
                // Title
                VStack(alignment: .leading, spacing: 4) {
                    Text("Title")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Task title", text: $draftTask.title)
                        .textFieldStyle(.roundedBorder)
                        .accessibilityLabel("Task title")
                        .accessibilityHint("Enter the task title")
                        .accessibilityIdentifier(AccessibilityID.titleField)
                }

                // Description
                VStack(alignment: .leading, spacing: 4) {
                    Text("Description")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextEditor(text: Binding(
                        get: { draftTask.description ?? "" },
                        set: { draftTask.description = $0.isEmpty ? nil : $0 }
                    ))
                    .font(.body)
                    .frame(minHeight: 80, maxHeight: 120)
                    .padding(4)
                    .background(Color(NSColor.textBackgroundColor))
                    .clipShape(.rect(cornerRadius: 6))
                    .accessibilityLabel("Task description")
                    .accessibilityHint("Enter a detailed description of the task")
                }
            }
        }
    }

    @ViewBuilder
    private func statusSection() -> some View {
        glassGroupBox("Status & Priority") {
            HStack(spacing: 20) {
                // Status Picker
                VStack(alignment: .leading, spacing: 4) {
                    Text("Status")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Picker("Status", selection: $draftTask.status) {
                        ForEach(RalphTaskStatus.allCases, id: \.self) { status in
                            HStack(spacing: 6) {
                                Circle()
                                    .fill(statusColor(status))
                                    .frame(width: 8, height: 8)
                                    .accessibilityLabel("Status: \(status.displayName)")
                                Text(status.displayName)
                            }
                            .tag(status)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 140)
                    .accessibilityLabel("Task status")
                }

                // Priority Picker
                VStack(alignment: .leading, spacing: 4) {
                    Text("Priority")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Picker("Priority", selection: $draftTask.priority) {
                        ForEach(RalphTaskPriority.allCases, id: \.self) { priority in
                            HStack(spacing: 6) {
                                Circle()
                                    .fill(priorityColor(priority))
                                    .frame(width: 8, height: 8)
                                    .accessibilityLabel("Priority: \(priority.displayName)")
                                Text(priority.displayName)
                            }
                            .tag(priority)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 140)
                    .accessibilityLabel("Task priority")
                }

                Spacer()
            }
        }
    }

    @ViewBuilder
    private func timeTrackingSection() -> some View {
        glassGroupBox("Time Tracking") {
            HStack(spacing: 20) {
                // Estimated Minutes
                VStack(alignment: .leading, spacing: 4) {
                    Text("Estimated (min)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Minutes", value: $draftTask.estimatedMinutes, format: .number)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 100)
                        .accessibilityLabel("Estimated minutes")
                }

                // Actual Minutes
                VStack(alignment: .leading, spacing: 4) {
                    Text("Actual (min)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Minutes", value: $draftTask.actualMinutes, format: .number)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 100)
                        .accessibilityLabel("Actual minutes")
                }

                // Accuracy indicator (if both are set)
                if let estimated = draftTask.estimatedMinutes,
                   let actual = draftTask.actualMinutes,
                   estimated > 0 {
                    let ratio = Double(actual) / Double(estimated)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Accuracy")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        HStack(spacing: 4) {
                            Circle()
                                .fill(accuracyColor(ratio: ratio))
                                .frame(width: 8, height: 8)
                            Text(accuracyLabel(ratio: ratio))
                                .font(.caption)
                        }
                    }
                }

                Spacer()
            }
        }
    }

    private func accuracyColor(ratio: Double) -> Color {
        if ratio >= 0.75 && ratio <= 1.25 {
            return .green
        } else if ratio >= 0.5 && ratio <= 1.5 {
            return .yellow
        } else {
            return .red
        }
    }

    private func accuracyLabel(ratio: Double) -> String {
        if ratio >= 0.75 && ratio <= 1.25 {
            return "On target"
        } else if ratio >= 0.5 && ratio < 0.75 {
            return "Overestimated"
        } else if ratio > 1.25 && ratio <= 1.5 {
            return "Underestimated"
        } else if ratio < 0.5 {
            return "Way over"
        } else {
            return "Way under"
        }
    }

    @ViewBuilder
    private func tagsSection() -> some View {
        glassGroupBox("Tags") {
            TagEditorView(tags: $draftTask.tags)
        }
    }

    @ViewBuilder
    private func contentSections() -> some View {
        // Scope
        if draftTask.scope != nil || isEditingNewArrayField("scope") {
            glassGroupBox("Scope") {
                StringArrayEditor(
                    items: Binding(
                        get: { draftTask.scope ?? [] },
                        set: { draftTask.scope = $0.isEmpty ? nil : $0 }
                    ),
                    placeholder: "Add file path..."
                )
            }
        }

        // Evidence
        if draftTask.evidence != nil || isEditingNewArrayField("evidence") {
            glassGroupBox("Evidence") {
                StringArrayEditor(
                    items: Binding(
                        get: { draftTask.evidence ?? [] },
                        set: { draftTask.evidence = $0.isEmpty ? nil : $0 }
                    ),
                    placeholder: "Add evidence item..."
                )
            }
        }

        // Plan
        if draftTask.plan != nil || isEditingNewArrayField("plan") {
            glassGroupBox("Plan") {
                StringArrayEditor(
                    items: Binding(
                        get: { draftTask.plan ?? [] },
                        set: { draftTask.plan = $0.isEmpty ? nil : $0 }
                    ),
                    placeholder: "Add plan step..."
                )
            }
        }

        // Notes
        if draftTask.notes != nil || isEditingNewArrayField("notes") {
            glassGroupBox("Notes") {
                StringArrayEditor(
                    items: Binding(
                        get: { draftTask.notes ?? [] },
                        set: { draftTask.notes = $0.isEmpty ? nil : $0 }
                    ),
                    placeholder: "Add note..."
                )
            }
        }

        // Add Field Buttons
        glassGroupBox("Add Fields") {
            FlowLayout(spacing: 8) {
                if draftTask.scope == nil {
                    addFieldButton("+ Scope", action: { draftTask.scope = [] })
                }
                if draftTask.evidence == nil {
                    addFieldButton("+ Evidence", action: { draftTask.evidence = [] })
                }
                if draftTask.plan == nil {
                    addFieldButton("+ Plan", action: { draftTask.plan = [] })
                }
                if draftTask.notes == nil {
                    addFieldButton("+ Notes", action: { draftTask.notes = [] })
                }
            }
        }
    }

    @ViewBuilder
    private func relationshipsSection() -> some View {
        let allTaskIDs = workspace.tasks.map { $0.id }.filter { $0 != task.id }
        let existingEdges = buildExistingEdges()

        glassGroupBox("Relationships") {
            VStack(alignment: .leading, spacing: 16) {
                // Depends On
                if draftTask.dependsOn != nil || isEditingNewArrayField("dependsOn") {
                    TaskRelationshipPicker(
                        label: "Depends On",
                        relatedTaskIDs: Binding(
                            get: { draftTask.dependsOn ?? [] },
                            set: { draftTask.dependsOn = $0.isEmpty ? nil : $0 }
                        ),
                        allTaskIDs: allTaskIDs,
                        currentTaskID: task.id,
                        edgeType: .dependency,
                        existingEdges: existingEdges
                    )
                }

                // Blocks
                if draftTask.blocks != nil || isEditingNewArrayField("blocks") {
                    TaskRelationshipPicker(
                        label: "Blocks",
                        relatedTaskIDs: Binding(
                            get: { draftTask.blocks ?? [] },
                            set: { draftTask.blocks = $0.isEmpty ? nil : $0 }
                        ),
                        allTaskIDs: allTaskIDs,
                        currentTaskID: task.id,
                        edgeType: .blocks,
                        existingEdges: existingEdges
                    )
                }

                // Relates To
                if draftTask.relatesTo != nil || isEditingNewArrayField("relatesTo") {
                    TaskRelationshipPicker(
                        label: "Relates To",
                        relatedTaskIDs: Binding(
                            get: { draftTask.relatesTo ?? [] },
                            set: { draftTask.relatesTo = $0.isEmpty ? nil : $0 }
                        ),
                        allTaskIDs: allTaskIDs,
                        currentTaskID: task.id,
                        edgeType: .relatesTo,
                        existingEdges: existingEdges
                    )
                }

                // Add Relationship Buttons
                if draftTask.dependsOn == nil || draftTask.blocks == nil || draftTask.relatesTo == nil {
                    FlowLayout(spacing: 8) {
                        if draftTask.dependsOn == nil {
                            addFieldButton("+ Depends On", action: { draftTask.dependsOn = [] })
                        }
                        if draftTask.blocks == nil {
                            addFieldButton("+ Blocks", action: { draftTask.blocks = [] })
                        }
                        if draftTask.relatesTo == nil {
                            addFieldButton("+ Relates To", action: { draftTask.relatesTo = [] })
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func metadataSection() -> some View {
        glassGroupBox("Metadata") {
            VStack(alignment: .leading, spacing: 8) {
                metadataRow(label: "Created", date: draftTask.createdAt)
                metadataRow(label: "Updated", date: draftTask.updatedAt)
                metadataRow(label: "Started", date: draftTask.startedAt)
                metadataRow(label: "Completed", date: draftTask.completedAt)
            }
        }
    }

    @ViewBuilder
    private func metadataRow(label: String, date: Date?) -> some View {
        HStack {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 70, alignment: .leading)

            if let date = date {
                Text(formatDate(date))
                    .font(.caption)
                    .foregroundStyle(.primary)
            } else {
                Text("—")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .accessibilityLabel("\(label): \(date.map(formatDateForAccessibility) ?? "Not set")")
    }

    @ViewBuilder
    private func addFieldButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.caption)
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
        }
        .buttonStyle(GlassButtonStyle())
        .accessibilityLabel("Add \(title) field")
    }

    // MARK: - Helper Methods

    private func glassGroupBox<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(.caption, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)

            content()
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .underPageBackground(cornerRadius: 10, isEmphasized: false)
        }
        .accessibilityLabel("\(title) section")
    }

    private func hasChanges() -> Bool {
        draftTask != baselineTask
    }

    private func saveChanges(force: Bool = false) {
        // Check for conflict before saving (unless force)
        if !force && hasConflict {
            showingConflictAlert = true
            return
        }

        isSaving = true
        saveError = nil
        saveSuccess = false

        Task { @MainActor in
            do {
                // Pass originalUpdatedAt for optimistic locking check
                try await workspace.updateTask(
                    from: baselineTask,
                    to: draftTask,
                    originalUpdatedAt: force ? nil : originalUpdatedAt
                )
                let persistedTask = workspace.tasks.first(where: { $0.id == draftTask.id }) ?? draftTask
                isSaving = false
                saveSuccess = true
                hasConflict = false
                conflictedExternalTask = nil

                // Update baseline after successful save so future optimistic-lock checks are accurate.
                draftTask = persistedTask
                baselineTask = persistedTask
                originalUpdatedAt = persistedTask.updatedAt
                onTaskUpdated?(persistedTask)

                // Clear success indicator after 2 seconds
                DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                    withAnimation(.easeInOut) {
                        saveSuccess = false
                    }
                }
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

    // MARK: - Conflict Detection

    private func checkForExternalChanges() {
        guard !isSaving else { return }

        // If no local changes, silently update the draft to match external changes
        guard hasChanges() else {
            if let currentTask = workspace.tasks.first(where: { $0.id == task.id }) {
                draftTask = currentTask
                baselineTask = currentTask
                originalUpdatedAt = currentTask.updatedAt
                hasConflict = false
                conflictedExternalTask = nil
            }
            return
        }

        // Check for conflict using optimistic locking
        if let externalTask = workspace.checkForConflict(
            taskID: task.id,
            originalUpdatedAt: originalUpdatedAt
        ) {
            hasConflict = true
            conflictedExternalTask = externalTask
            showingConflictAlert = true
        }
    }

    private func discardLocalChanges() {
        if let externalTask = conflictedExternalTask {
            draftTask = externalTask
            baselineTask = externalTask
            originalUpdatedAt = externalTask.updatedAt
            hasConflict = false
            conflictedExternalTask = nil
        }
    }

    private func mutateTaskAgent(_ mutate: (inout RalphTaskAgent) -> Void) {
        var agent = draftTask.agent ?? RalphTaskAgent()
        mutate(&agent)
        draftTask.agent = RalphTaskAgent.normalizedOverride(agent)
    }
}

// Preview
#Preview {
    TaskDetailView(
        workspace: Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp")),
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
