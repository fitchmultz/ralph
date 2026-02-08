/**
 TaskDetailView

 Responsibilities:
 - Display a comprehensive form for viewing and editing all task fields.
 - Support inline editing with proper form controls (pickers, text editors, tag editors).
 - Integrate with Workspace to persist changes via CLI.

 Does not handle:
 - Task creation (see task builder workflow).
 - Batch operations on multiple tasks.

 Invariants/assumptions callers must respect:
 - Task is passed in and copied to @State for editing.
 - Changes are only persisted when user explicitly saves.
 - Sheet is dismissed on cancel or successful save.
 */

import SwiftUI
import RalphCore

struct TaskDetailView: View {
    @ObservedObject var workspace: Workspace
    let task: RalphTask
    @Binding var isPresented: Bool

    // State for mutable copy of task being edited
    @State private var draftTask: RalphTask
    @State private var isSaving = false
    @State private var saveError: String?
    @State private var showingUnsavedChangesAlert = false

    init(workspace: Workspace, task: RalphTask, isPresented: Binding<Bool>) {
        self.workspace = workspace
        self.task = task
        self._isPresented = isPresented
        self._draftTask = State(initialValue: task)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    basicInfoSection()
                    statusSection()
                    tagsSection()
                    contentSections()
                    relationshipsSection()
                    metadataSection()
                }
                .padding(20)
            }
            .background(.clear)
            .navigationTitle("Edit Task")
            .navigationSubtitle(task.id)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        if hasChanges() {
                            showingUnsavedChangesAlert = true
                        } else {
                            isPresented = false
                        }
                    }
                }

                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        saveChanges()
                    }
                    .disabled(!hasChanges() || isSaving)
                }
            }
            .alert("Unsaved Changes", isPresented: $showingUnsavedChangesAlert) {
                Button("Discard Changes", role: .destructive) {
                    isPresented = false
                }
                Button("Keep Editing", role: .cancel) {}
            } message: {
                Text("You have unsaved changes. Are you sure you want to discard them?")
            }
            .alert("Save Error", isPresented: .constant(saveError != nil)) {
                Button("OK") {
                    saveError = nil
                }
            } message: {
                Text(saveError ?? "")
            }
        }
        .frame(minWidth: 700, minHeight: 600)
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
                    .cornerRadius(6)
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
                                Text(status.displayName)
                            }
                            .tag(status)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 140)
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
                                Text(priority.displayName)
                            }
                            .tag(priority)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 140)
                }

                Spacer()
            }
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
                        currentTaskID: task.id
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
                        currentTaskID: task.id
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
                        currentTaskID: task.id
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
    }

    private func hasChanges() -> Bool {
        draftTask != task
    }

    private func saveChanges() {
        isSaving = true
        saveError = nil

        Task {
            do {
                try await workspace.updateTask(from: task, to: draftTask)
                await MainActor.run {
                    isSaving = false
                    isPresented = false
                }
            } catch {
                await MainActor.run {
                    isSaving = false
                    saveError = error.localizedDescription
                }
            }
        }
    }

    private func statusColor(_ status: RalphTaskStatus) -> Color {
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

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
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

    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    private func isEditingNewArrayField(_ field: String) -> Bool {
        // Used to check if we're currently editing a field that was just added
        // This helps with conditional display of optional array fields
        false
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
        ),
        isPresented: .constant(true)
    )
}
