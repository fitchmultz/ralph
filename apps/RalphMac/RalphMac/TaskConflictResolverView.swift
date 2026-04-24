/**
 TaskConflictResolverView

 Purpose:
 - Display a structured comparison of local vs external task changes.

 Responsibilities:
 - Display a structured comparison of local vs external task changes.
 - Let users choose a winner for each conflicted field and preview that decision in-place.
 - Produce a merged task using the shared conflict-resolution model.

 Does not handle:
 - Optimistic-lock detection timing.
 - Error-recovery flows or diagnostics tooling.
 - Persisting the merged task.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Both tasks refer to the same task ID.
 - Conflict field semantics come from `TaskConflictResolutionModel`.
 - `onMerge` receives the fully merged task chosen by the user.
 */

import SwiftUI
import RalphCore

@MainActor
struct TaskConflictResolverView: View {
    let localTask: RalphTask
    let externalTask: RalphTask
    let onMerge: (RalphTask) -> Void
    let onCancel: () -> Void

    @State private var selections: [TaskConflictField: TaskConflictMergeChoice]

    private let model: TaskConflictResolutionModel

    init(
        localTask: RalphTask,
        externalTask: RalphTask,
        onMerge: @escaping (RalphTask) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.localTask = localTask
        self.externalTask = externalTask
        self.onMerge = onMerge
        self.onCancel = onCancel

        let model = TaskConflictResolutionModel(localTask: localTask, externalTask: externalTask)
        self.model = model
        _selections = State(initialValue: model.initialSelections)
    }

    private var mergedTask: RalphTask {
        model.applySelections(selections)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            summary

            Divider()

            List {
                ForEach(model.sections) { section in
                    Section(section.section.rawValue) {
                        ForEach(section.fields) { field in
                            mergeRow(field)
                        }
                    }
                }
            }
        }
        .frame(minWidth: 760, minHeight: 560)
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Resolve Conflicting Changes")
                    .font(.headline)
                Text("This task changed outside the editor. Choose which value should win for each conflicting field.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button("Cancel", action: onCancel)
            Button("Apply Selection") {
                onMerge(mergedTask)
            }
            .buttonStyle(.borderedProminent)
            .disabled(model.fieldPresentations.isEmpty)
        }
        .padding()
        .background(.ultraThinMaterial)
    }

    private var summary: some View {
        HStack(spacing: 20) {
            conflictSummaryCard(
                title: "My Changes",
                task: localTask,
                accent: .blue,
                selectedCount: selections.values.filter { $0 == .local }.count
            )

            Image(systemName: "arrow.left.arrow.right")
                .font(.title2)
                .foregroundStyle(.secondary)

            conflictSummaryCard(
                title: "External Changes",
                task: externalTask,
                accent: .orange,
                selectedCount: selections.values.filter { $0 == .external }.count
            )
        }
        .padding()
    }

    private func conflictSummaryCard(
        title: String,
        task: RalphTask,
        accent: Color,
        selectedCount: Int
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Circle()
                    .fill(accent)
                    .frame(width: 8, height: 8)
                Text(title)
                    .font(.body.weight(.semibold))
                Spacer()
                Text("\(selectedCount) selected")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(task.title)
                    .font(.caption)
                    .lineLimit(1)
                Text("Status: \(task.status.displayName)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                if let updatedAt = task.updatedAt {
                    Text("Updated: \(Self.relativeDateFormatter.localizedString(for: updatedAt, relativeTo: Date()))")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(accent.opacity(0.1))
        .clipShape(.rect(cornerRadius: 8))
    }

    private func mergeRow(_ field: TaskConflictFieldPresentation) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(field.label)
                    .font(.body.weight(.medium))
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .help("Conflict detected")
                Spacer()
                Picker(
                    field.label,
                    selection: Binding(
                        get: { selections[field.field] ?? .external },
                        set: { selections[field.field] = $0 }
                    )
                ) {
                    Text(TaskConflictMergeChoice.external.rawValue).tag(TaskConflictMergeChoice.external)
                    Text("My Changes").tag(TaskConflictMergeChoice.local)
                }
                .pickerStyle(.segmented)
                .frame(width: 180)
            }

            HStack(alignment: .top, spacing: 12) {
                valueColumn(
                    title: "External",
                    value: field.externalValue,
                    isSelected: selections[field.field] == .external,
                    accent: .orange
                )

                valueColumn(
                    title: "My Changes",
                    value: field.localValue,
                    isSelected: selections[field.field] == .local,
                    accent: .blue
                )
            }
        }
        .padding(.vertical, 6)
    }

    private func valueColumn(
        title: String,
        value: String,
        isSelected: Bool,
        accent: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.caption)
                .foregroundStyle(isSelected ? .primary : .secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .textSelection(.enabled)
                .lineLimit(4)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(isSelected ? accent.opacity(0.12) : Color.clear)
        .clipShape(.rect(cornerRadius: 6))
    }

    private static let relativeDateFormatter: RelativeDateTimeFormatter = {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter
    }()
}

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
        onCancel: {}
    )
}
