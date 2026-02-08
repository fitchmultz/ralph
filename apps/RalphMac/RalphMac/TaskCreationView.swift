/**
 TaskCreationView

 Responsibilities:
 - Provide a modal interface for creating new tasks with or without templates.
 - Support template selection from 10 built-in templates with preview cards.
 - Handle template variable substitution for templates requiring {{target}}.
 - Offer Quick Create mode (title + priority only) and Advanced mode (all fields).
 - Integrate with Workspace to create tasks via CLI commands.

 Does not handle:
 - Direct queue file manipulation (delegates to CLI via Workspace).
 - Template editing or custom template creation.
 - Task editing (see TaskDetailView).

 Invariants/assumptions callers must respect:
 - Workspace must have a valid CLI client injected.
 - Template selection triggers pre-fill of task fields.
 - Templates with {{target}} variables require target input before creation.
 - Task creation runs asynchronously and reports success/failure.
 */

import SwiftUI
import RalphCore

// MARK: - Creation Mode
enum TaskCreationMode {
    case quick     // Title + Priority only
    case advanced  // All fields
}

// MARK: - Template Info
struct TemplateInfo: Identifiable, Equatable {
    let id = UUID()
    let name: String
    let description: String
    let icon: String
    let requiresTarget: Bool
    let defaultPriority: RalphTaskPriority
    let defaultTags: [String]
}

// MARK: - View
struct TaskCreationView: View {
    @ObservedObject var workspace: Workspace
    @Environment(\.dismiss) private var dismiss

    // MARK: Mode & State
    @State private var mode: TaskCreationMode = .quick
    @State private var selectedTemplate: TemplateInfo? = nil
    @State private var showingTemplatePicker = true // Start with template picker

    // MARK: Task Fields
    @State private var title = ""
    @State private var description = ""
    @State private var priority: RalphTaskPriority = .medium
    @State private var tags: [String] = []
    @State private var scope: [String] = []
    @State private var target: String = "" // For template variables

    // MARK: UI State
    @State private var isCreating = false
    @State private var errorMessage: String?
    @State private var showingError = false

    // MARK: Available Templates
    private let templates: [TemplateInfo] = [
        TemplateInfo(name: "bug", description: "Bug fix with reproduction steps", icon: "ladybug.fill", requiresTarget: false, defaultPriority: .high, defaultTags: ["bug", "fix"]),
        TemplateInfo(name: "feature", description: "New feature with design and docs", icon: "star.fill", requiresTarget: false, defaultPriority: .medium, defaultTags: ["feature", "enhancement"]),
        TemplateInfo(name: "refactor", description: "Code refactoring", icon: "arrow.2.squarepath", requiresTarget: false, defaultPriority: .medium, defaultTags: ["refactor", "cleanup"]),
        TemplateInfo(name: "test", description: "Test addition or improvement", icon: "checkmark.seal.fill", requiresTarget: false, defaultPriority: .high, defaultTags: ["test", "coverage"]),
        TemplateInfo(name: "docs", description: "Documentation update", icon: "doc.text.fill", requiresTarget: false, defaultPriority: .low, defaultTags: ["docs", "documentation"]),
        TemplateInfo(name: "add-tests", description: "Add tests for existing code", icon: "plus.viewfinder", requiresTarget: true, defaultPriority: .high, defaultTags: ["test", "coverage", "quality"]),
        TemplateInfo(name: "refactor-performance", description: "Optimize performance", icon: "gauge.high", requiresTarget: true, defaultPriority: .medium, defaultTags: ["refactor", "performance", "optimization"]),
        TemplateInfo(name: "fix-error-handling", description: "Fix error handling", icon: "exclamationmark.triangle.fill", requiresTarget: true, defaultPriority: .high, defaultTags: ["bug", "error-handling", "reliability"]),
        TemplateInfo(name: "add-docs", description: "Add documentation for file/module", icon: "text.badge.plus", requiresTarget: true, defaultPriority: .low, defaultTags: ["docs", "documentation"]),
        TemplateInfo(name: "security-audit", description: "Security audit", icon: "lock.shield.fill", requiresTarget: true, defaultPriority: .critical, defaultTags: ["security", "audit", "compliance"]),
    ]

    var body: some View {
        NavigationStack {
            Group {
                if showingTemplatePicker {
                    templatePickerView()
                } else {
                    taskFormView()
                }
            }
            .navigationTitle(showingTemplatePicker ? "New Task" : "Create Task")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    if showingTemplatePicker {
                        Button("Cancel") { dismiss() }
                    } else {
                        Button("Back") { showingTemplatePicker = true }
                    }
                }

                if !showingTemplatePicker {
                    ToolbarItem(placement: .primaryAction) {
                        Button("Create") { createTask() }
                            .disabled(!canCreate() || isCreating)
                    }
                }
            }
        }
        .frame(minWidth: 500, minHeight: showingTemplatePicker ? 400 : 600)
        .alert("Error", isPresented: $showingError) {
            Button("OK") {}
        } message: {
            Text(errorMessage ?? "Unknown error")
        }
    }

    // MARK: - Template Picker View
    @ViewBuilder
    private func templatePickerView() -> some View {
        VStack(spacing: 0) {
            // Mode Toggle
            Picker("Mode", selection: $mode) {
                Text("Quick Create").tag(TaskCreationMode.quick)
                Text("Advanced").tag(TaskCreationMode.advanced)
            }
            .pickerStyle(.segmented)
            .padding()

            if mode == .quick {
                quickCreateForm()
            } else {
                templateGalleryView()
            }
        }
    }

    // MARK: - Quick Create Form
    @ViewBuilder
    private func quickCreateForm() -> some View {
        Form {
            Section {
                TextField("Task title", text: $title)
                    .font(.title3)

                Picker("Priority", selection: $priority) {
                    ForEach(RalphTaskPriority.allCases, id: \.self) { p in
                        HStack {
                            Circle()
                                .fill(priorityColor(p))
                                .frame(width: 8, height: 8)
                            Text(p.displayName)
                        }
                        .tag(p)
                    }
                }
            }

            Section {
                Button("Create Task") { createTask() }
                    .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty || isCreating)
            }
        }
        .formStyle(.grouped)
        .padding()
    }

    // MARK: - Template Gallery View
    @ViewBuilder
    private func templateGalleryView() -> some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 200))], spacing: 16) {
                // "No Template" option
                templateCard(
                    name: "Blank",
                    description: "Start from scratch",
                    icon: "doc",
                    requiresTarget: false,
                    isSelected: selectedTemplate == nil
                ) {
                    selectedTemplate = nil
                    showingTemplatePicker = false
                }

                // Template cards
                ForEach(templates) { template in
                    templateCard(
                        name: template.name,
                        description: template.description,
                        icon: template.icon,
                        requiresTarget: template.requiresTarget,
                        isSelected: selectedTemplate?.id == template.id
                    ) {
                        selectedTemplate = template
                        applyTemplate(template)
                        showingTemplatePicker = false
                    }
                }
            }
            .padding()
        }
    }

    // MARK: - Template Card
    @ViewBuilder
    private func templateCard(
        name: String,
        description: String,
        icon: String,
        requiresTarget: Bool,
        isSelected: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Image(systemName: icon)
                        .font(.title2)
                        .foregroundStyle(.accent)

                    Spacer()

                    if requiresTarget {
                        Image(systemName: "text.cursor")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .help("Requires target path")
                    }
                }

                VStack(alignment: .leading, spacing: 4) {
                    Text(name)
                        .font(.headline)

                    Text(description)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            .padding()
            .frame(height: 100)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(isSelected ? Color.accentColor.opacity(0.1) : Color.secondary.opacity(0.05))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(isSelected ? Color.accentColor : Color.clear, lineWidth: 2)
            )
            .cornerRadius(10)
        }
        .buttonStyle(.plain)
    }

    // MARK: - Task Form View
    @ViewBuilder
    private func taskFormView() -> some View {
        Form {
            // Template indicator
            if let template = selectedTemplate {
                Section {
                    HStack {
                        Image(systemName: template.icon)
                        Text("Using template: \(template.name)")
                            .font(.caption)
                        Spacer()
                    }
                    .foregroundStyle(.secondary)
                }
            }

            // Target input for templates requiring it
            if selectedTemplate?.requiresTarget == true {
                Section("Target") {
                    TextField("File or module path (e.g., src/main.rs)", text: $target)
                    Text("Required for template variable substitution ({{target}}, {{module}}, {{file}})")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            // Basic Fields
            Section("Basic Information") {
                TextField("Title", text: $title)

                Picker("Priority", selection: $priority) {
                    ForEach(RalphTaskPriority.allCases, id: \.self) { p in
                        HStack(spacing: 6) {
                            Circle()
                                .fill(priorityColor(p))
                                .frame(width: 8, height: 8)
                            Text(p.displayName)
                        }
                        .tag(p)
                    }
                }
            }

            // Description
            Section("Description") {
                TextEditor(text: $description)
                    .frame(minHeight: 60)
            }

            // Tags
            Section("Tags") {
                TagEditorView(tags: $tags)
            }

            // Scope
            Section("Scope") {
                StringArrayEditor(items: $scope, placeholder: "Add file path...")
            }
        }
        .formStyle(.grouped)
    }

    // MARK: - Helper Methods

    private func applyTemplate(_ template: TemplateInfo) {
        priority = template.defaultPriority
        tags = template.defaultTags

        // Set a placeholder title based on template
        if template.requiresTarget {
            title = "\(template.name) for {{target}}"
        } else {
            title = ""
        }
    }

    private func canCreate() -> Bool {
        let hasTitle = !title.trimmingCharacters(in: .whitespaces).isEmpty
        let hasTargetIfRequired = !(selectedTemplate?.requiresTarget == true && target.trimmingCharacters(in: .whitespaces).isEmpty)
        return hasTitle && hasTargetIfRequired
    }

    private func createTask() {
        isCreating = true

        Task {
            do {
                try await workspace.createTask(
                    title: title,
                    description: description.isEmpty ? nil : description,
                    priority: priority,
                    tags: tags,
                    scope: scope.isEmpty ? nil : scope,
                    template: selectedTemplate?.name,
                    target: target.isEmpty ? nil : target
                )

                await MainActor.run {
                    isCreating = false
                    dismiss()
                }
            } catch {
                await MainActor.run {
                    isCreating = false
                    errorMessage = error.localizedDescription
                    showingError = true
                }
            }
        }
    }

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical: return .red
        case .high: return .orange
        case .medium: return .yellow
        case .low: return .gray
        }
    }
}

// MARK: - Preview
#Preview {
    TaskCreationView(workspace: Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp")))
}
