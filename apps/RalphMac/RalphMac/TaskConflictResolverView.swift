/**
 TaskConflictResolverView

 Responsibilities:
 - Display a side-by-side comparison of local vs external task changes.
 - Allow users to select which version (local or external) to keep for each conflicted field.
 - Provide an option to merge selections and produce a final merged task.

 Does not handle:
 - Direct saving of the merged task (parent view handles this via callback).
 - Automatic resolution of conflicts (user must explicitly choose).

 Invariants/assumptions callers must respect:
 - Both localTask and externalTask must have the same task ID.
 - onMerge callback receives the merged task for the parent to save.
 - onCancel callback is called when user cancels the merge.
 */

import SwiftUI
import RalphCore

struct TaskConflictResolverView: View {
    let localTask: RalphTask
    let externalTask: RalphTask
    let onMerge: (RalphTask) -> Void
    let onCancel: () -> Void
    
    @State private var mergedTask: RalphTask
    @State private var fieldSelections: [String: MergeChoice] = [:]
    
    enum MergeChoice: String, CaseIterable {
        case local = "Local"
        case external = "External"
    }
    
    init(localTask: RalphTask, externalTask: RalphTask, onMerge: @escaping (RalphTask) -> Void, onCancel: @escaping () -> Void) {
        self.localTask = localTask
        self.externalTask = externalTask
        self.onMerge = onMerge
        self.onCancel = onCancel
        // Start with external as base (most recent)
        _mergedTask = State(initialValue: externalTask)
        
        // Initialize field selections based on which fields differ
        var initialSelections: [String: MergeChoice] = [:]
        if localTask.title != externalTask.title { initialSelections["title"] = .external }
        if localTask.description != externalTask.description { initialSelections["description"] = .external }
        if localTask.status != externalTask.status { initialSelections["status"] = .external }
        if localTask.priority != externalTask.priority { initialSelections["priority"] = .external }
        if localTask.tags != externalTask.tags { initialSelections["tags"] = .external }
        if localTask.scope != externalTask.scope { initialSelections["scope"] = .external }
        if localTask.evidence != externalTask.evidence { initialSelections["evidence"] = .external }
        if localTask.plan != externalTask.plan { initialSelections["plan"] = .external }
        if localTask.notes != externalTask.notes { initialSelections["notes"] = .external }
        if localTask.dependsOn != externalTask.dependsOn { initialSelections["dependsOn"] = .external }
        if localTask.blocks != externalTask.blocks { initialSelections["blocks"] = .external }
        if localTask.relatesTo != externalTask.relatesTo { initialSelections["relatesTo"] = .external }
        _fieldSelections = State(initialValue: initialSelections)
    }
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Resolve Conflicting Changes")
                        .font(.headline)
                    Text("This task was modified externally while you were editing. Choose which changes to keep.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel", action: onCancel)
                Button("Apply Selection") {
                    applySelections()
                    onMerge(mergedTask)
                }
                .buttonStyle(.borderedProminent)
            }
            .padding()
            .background(.ultraThinMaterial)
            
            // Conflict summary
            HStack(spacing: 20) {
                conflictSummaryCard(
                    title: "My Changes",
                    task: localTask,
                    color: .blue
                )
                
                Image(systemName: "arrow.left.arrow.right")
                    .font(.title2)
                    .foregroundStyle(.secondary)
                
                conflictSummaryCard(
                    title: "External Changes",
                    task: externalTask,
                    color: .orange
                )
            }
            .padding()
            
            Divider()
            
            // Field comparison list
            List {
                Section("Basic Information") {
                    mergeRow(
                        field: "title",
                        label: "Title",
                        localValue: localTask.title,
                        externalValue: externalTask.title
                    )
                    
                    mergeRow(
                        field: "description",
                        label: "Description",
                        localValue: localTask.description ?? "(none)",
                        externalValue: externalTask.description ?? "(none)"
                    )
                    
                    mergeRow(
                        field: "status",
                        label: "Status",
                        localValue: localTask.status.displayName,
                        externalValue: externalTask.status.displayName
                    )
                    
                    mergeRow(
                        field: "priority",
                        label: "Priority",
                        localValue: localTask.priority.displayName,
                        externalValue: externalTask.priority.displayName
                    )
                }
                
                if localTask.tags != externalTask.tags {
                    Section("Tags") {
                        mergeRow(
                            field: "tags",
                            label: "Tags",
                            localValue: localTask.tags.joined(separator: ", ").isEmpty ? "(none)" : localTask.tags.joined(separator: ", "),
                            externalValue: externalTask.tags.joined(separator: ", ").isEmpty ? "(none)" : externalTask.tags.joined(separator: ", ")
                        )
                    }
                }
                
                if hasArrayFieldConflicts() {
                    Section("Arrays") {
                        if localTask.scope != externalTask.scope {
                            mergeRow(
                                field: "scope",
                                label: "Scope",
                                localValue: formatArray(localTask.scope),
                                externalValue: formatArray(externalTask.scope)
                            )
                        }
                        
                        if localTask.evidence != externalTask.evidence {
                            mergeRow(
                                field: "evidence",
                                label: "Evidence",
                                localValue: formatArray(localTask.evidence),
                                externalValue: formatArray(externalTask.evidence)
                            )
                        }
                        
                        if localTask.plan != externalTask.plan {
                            mergeRow(
                                field: "plan",
                                label: "Plan",
                                localValue: formatArray(localTask.plan),
                                externalValue: formatArray(externalTask.plan)
                            )
                        }
                        
                        if localTask.notes != externalTask.notes {
                            mergeRow(
                                field: "notes",
                                label: "Notes",
                                localValue: formatArray(localTask.notes),
                                externalValue: formatArray(externalTask.notes)
                            )
                        }
                    }
                }
                
                if hasRelationshipConflicts() {
                    Section("Relationships") {
                        if localTask.dependsOn != externalTask.dependsOn {
                            mergeRow(
                                field: "dependsOn",
                                label: "Depends On",
                                localValue: formatArray(localTask.dependsOn),
                                externalValue: formatArray(externalTask.dependsOn)
                            )
                        }
                        
                        if localTask.blocks != externalTask.blocks {
                            mergeRow(
                                field: "blocks",
                                label: "Blocks",
                                localValue: formatArray(localTask.blocks),
                                externalValue: formatArray(externalTask.blocks)
                            )
                        }
                        
                        if localTask.relatesTo != externalTask.relatesTo {
                            mergeRow(
                                field: "relatesTo",
                                label: "Relates To",
                                localValue: formatArray(localTask.relatesTo),
                                externalValue: formatArray(externalTask.relatesTo)
                            )
                        }
                    }
                }
            }
        }
        .frame(minWidth: 650, minHeight: 500)
    }
    
    @ViewBuilder
    private func conflictSummaryCard(title: String, task: RalphTask, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Circle()
                    .fill(color)
                    .frame(width: 8, height: 8)
                Text(title)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                Spacer()
            }
            
            VStack(alignment: .leading, spacing: 4) {
                Text(task.title)
                    .font(.caption)
                    .lineLimit(1)
                Text("Status: \(task.status.displayName)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                if let updatedAt = task.updatedAt {
                    Text("Updated: \(formatDate(updatedAt))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(color.opacity(0.1))
        .cornerRadius(8)
    }
    
    @ViewBuilder
    private func mergeRow(field: String, label: String, localValue: String, externalValue: String) -> some View {
        let hasConflict = localValue != externalValue
        
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(label)
                    .font(.subheadline)
                    .fontWeight(.medium)
                if hasConflict {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .help("Conflict detected")
                }
                Spacer()
            }
            
            if hasConflict {
                VStack(spacing: 8) {
                    // External version option
                    HStack {
                        Picker("", selection: Binding(
                            get: { fieldSelections[field] ?? .external },
                            set: { fieldSelections[field] = $0 }
                        )) {
                            Text("External").tag(MergeChoice.external)
                            Text("My Changes").tag(MergeChoice.local)
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 160)
                        
                        Spacer()
                    }
                    
                    // Show both values
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("External:")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            Text(externalValue)
                                .font(.caption)
                                .foregroundStyle(fieldSelections[field] == .external ? .primary : .secondary)
                                .lineLimit(3)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(6)
                        .background(fieldSelections[field] == .external ? Color.orange.opacity(0.1) : Color.clear)
                        .cornerRadius(4)
                        
                        VStack(alignment: .leading, spacing: 2) {
                            Text("My Changes:")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            Text(localValue)
                                .font(.caption)
                                .foregroundStyle(fieldSelections[field] == .local ? .primary : .secondary)
                                .lineLimit(3)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(6)
                        .background(fieldSelections[field] == .local ? Color.blue.opacity(0.1) : Color.clear)
                        .cornerRadius(4)
                    }
                }
            } else {
                HStack {
                    Text(externalValue)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    Spacer()
                }
            }
        }
        .padding(.vertical, 4)
    }
    
    private func applySelections() {
        var newTask = externalTask
        
        for (field, choice) in fieldSelections {
            if choice == .local {
                switch field {
                case "title":
                    newTask.title = localTask.title
                case "description":
                    newTask.description = localTask.description
                case "status":
                    newTask.status = localTask.status
                case "priority":
                    newTask.priority = localTask.priority
                case "tags":
                    newTask.tags = localTask.tags
                case "scope":
                    newTask.scope = localTask.scope
                case "evidence":
                    newTask.evidence = localTask.evidence
                case "plan":
                    newTask.plan = localTask.plan
                case "notes":
                    newTask.notes = localTask.notes
                case "dependsOn":
                    newTask.dependsOn = localTask.dependsOn
                case "blocks":
                    newTask.blocks = localTask.blocks
                case "relatesTo":
                    newTask.relatesTo = localTask.relatesTo
                default:
                    break
                }
            }
        }
        
        mergedTask = newTask
    }
    
    private func hasArrayFieldConflicts() -> Bool {
        localTask.scope != externalTask.scope ||
        localTask.evidence != externalTask.evidence ||
        localTask.plan != externalTask.plan ||
        localTask.notes != externalTask.notes
    }
    
    private func hasRelationshipConflicts() -> Bool {
        localTask.dependsOn != externalTask.dependsOn ||
        localTask.blocks != externalTask.blocks ||
        localTask.relatesTo != externalTask.relatesTo
    }
    
    private func formatArray(_ array: [String]?) -> String {
        guard let array = array, !array.isEmpty else { return "(none)" }
        return array.joined(separator: ", ")
    }
    
    private func formatDate(_ date: Date) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

// Preview
#Preview {
    TaskConflictResolverView(
        localTask: RalphTask(
            id: "RQ-0001",
            status: .doing,
            title: "My Local Title",
            description: "Local description",
            priority: .high,
            tags: ["swift", "ui"],
            updatedAt: Date()
        ),
        externalTask: RalphTask(
            id: "RQ-0001",
            status: .todo,
            title: "External Title",
            description: "External description",
            priority: .medium,
            tags: ["swift", "backend"],
            updatedAt: Date().addingTimeInterval(300)
        ),
        onMerge: { _ in },
        onCancel: { }
    )
}
