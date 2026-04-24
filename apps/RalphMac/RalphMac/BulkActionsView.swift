/**
 BulkActionsView

 Purpose:
 - Display bulk action options (status change, priority change, tag modification)

 Responsibilities:
 - Display bulk action options (status change, priority change, tag modification)
 - Show operation count confirmation
 - Execute bulk operations via Workspace

 Does not handle:
 - Direct CLI execution (delegates to Workspace)
 - Selection management (managed by parent view)

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - selectedTaskIDs must be non-empty before presenting
 - Workspace must be available for CLI operations
*/

import SwiftUI
import RalphCore

@MainActor
struct BulkActionsView: View {
    let workspace: Workspace
    let selectedTaskIDs: Set<String>
    var onCompletion: (() -> Void)? = nil
    @Environment(\.dismiss) private var dismiss

    @State private var selectedStatus: RalphTaskStatus?
    @State private var selectedPriority: RalphTaskPriority?
    @State private var tagsToAdd: String = ""
    @State private var tagsToRemove: String = ""
    @State private var isExecuting: Bool = false
    @State private var errorMessage: String?
    @State private var showConfirmation: Bool = true

    var body: some View {
        VStack(spacing: 0) {
            // Header with count
            headerSection()
                .padding(.horizontal, 20)
                .padding(.top, 20)
                .padding(.bottom, 12)

            Divider()
                .padding(.horizontal, 12)

            ScrollView {
                VStack(spacing: 20) {
                    // Confirmation section
                    if showConfirmation {
                        confirmationSection()
                    }

                    // Status change section
                    statusSection()

                    Divider()
                        .padding(.horizontal, 4)

                    // Priority change section
                    prioritySection()

                    Divider()
                        .padding(.horizontal, 4)

                    // Tag modification section
                    tagsSection()

                    // Error message
                    if let error = errorMessage {
                        errorSection(message: error)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.vertical, 16)
            }

            // Action buttons
            buttonBar()
                .padding(.horizontal, 20)
                .padding(.vertical, 16)
        }
        .frame(width: 420, height: 520)
    }

    // MARK: - View Sections

    @ViewBuilder
    private func headerSection() -> some View {
        VStack(spacing: 4) {
            HStack {
                Image(systemName: "rectangle.stack")
                    .font(.title2)
                    .foregroundStyle(Color.accentColor)

                Text("Bulk Actions")
                    .font(.headline)

                Spacer()

                Button(action: { dismiss() }) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title3)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .keyboardShortcut(.escape)
            }

            HStack {
                Text("\(selectedTaskIDs.count) tasks selected")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)

                Spacer()
            }
        }
    }

    @ViewBuilder
    private func confirmationSection() -> some View {
        HStack(spacing: 12) {
            Image(systemName: "info.circle.fill")
                .foregroundStyle(.blue)
                .font(.title3)

            VStack(alignment: .leading, spacing: 4) {
                Text("Confirm Operation")
                    .font(.subheadline.weight(.medium))

                Text("You are about to modify \(selectedTaskIDs.count) tasks. This action cannot be undone.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button(action: { showConfirmation = false }) {
                Image(systemName: "xmark")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding(12)
        .background(.blue.opacity(0.1))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(.blue.opacity(0.2), lineWidth: 1)
        )
        .cornerRadius(8)
    }

    @ViewBuilder
    private func statusSection() -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Change Status", systemImage: "checkmark.circle")
                .font(.subheadline.weight(.medium))

            Picker("New Status", selection: $selectedStatus) {
                Text("No change")
                    .tag(nil as RalphTaskStatus?)

                Divider()

                ForEach(RalphTaskStatus.allCases, id: \.self) { status in
                    HStack(spacing: 6) {
                        Circle()
                            .fill(statusColor(status))
                            .frame(width: 8, height: 8)
                        Text(status.displayName)
                    }
                    .tag(status as RalphTaskStatus?)
                }
            }
            .pickerStyle(.menu)
            .fixedSize()
        }
    }

    @ViewBuilder
    private func prioritySection() -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Change Priority", systemImage: "exclamationmark.circle")
                .font(.subheadline.weight(.medium))

            Picker("New Priority", selection: $selectedPriority) {
                Text("No change")
                    .tag(nil as RalphTaskPriority?)

                Divider()

                ForEach(RalphTaskPriority.allCases, id: \.self) { priority in
                    HStack(spacing: 6) {
                        Circle()
                            .fill(priorityColor(priority))
                            .frame(width: 8, height: 8)
                        Text(priority.displayName)
                    }
                    .tag(priority as RalphTaskPriority?)
                }
            }
            .pickerStyle(.menu)
            .fixedSize()
        }
    }

    @ViewBuilder
    private func tagsSection() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("Modify Tags", systemImage: "tag")
                .font(.subheadline.weight(.medium))

            VStack(spacing: 8) {
                TextField("Tags to add (comma-separated)", text: $tagsToAdd)
                    .textFieldStyle(.roundedBorder)

                TextField("Tags to remove (comma-separated)", text: $tagsToRemove)
                    .textFieldStyle(.roundedBorder)
            }

            Text("Example: bug, urgent, frontend")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func errorSection(message: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)

            Text(message)
                .font(.caption)
                .foregroundStyle(.red)
                .lineLimit(2)

            Spacer()
        }
        .padding(10)
        .background(.red.opacity(0.1))
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(.red.opacity(0.2), lineWidth: 1)
        )
        .cornerRadius(6)
    }

    @ViewBuilder
    private func buttonBar() -> some View {
        HStack(spacing: 12) {
            Button("Cancel") {
                dismiss()
            }
            .keyboardShortcut(.escape, modifiers: [.command])

            Spacer()

            if isExecuting {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Updating...")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Button("Apply Changes") {
                Task {
                    await executeBulkActions()
                }
            }
            .keyboardShortcut(.return)
            .disabled(isExecuting || !hasAnyAction)
            .buttonStyle(.borderedProminent)
        }
    }

    // MARK: - Helpers

    private var hasAnyAction: Bool {
        selectedStatus != nil ||
        selectedPriority != nil ||
        !tagsToAdd.trimmingCharacters(in: .whitespaces).isEmpty ||
        !tagsToRemove.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func executeBulkActions() async {
        isExecuting = true
        errorMessage = nil
        defer { isExecuting = false }

        do {
            // Track which operations are pending to determine the last one
            let hasStatusChange = selectedStatus != nil
            let hasPriorityChange = selectedPriority != nil
            let addTags = parseTags(tagsToAdd)
            let removeTags = parseTags(tagsToRemove)
            let hasTagChange = !addTags.isEmpty || !removeTags.isEmpty
            
            // Determine the last operation to avoid multiple reloads
            let totalOperations = (hasStatusChange ? 1 : 0) + (hasPriorityChange ? 1 : 0) + (hasTagChange ? 1 : 0)
            var completedOperations = 0

            // Execute status change if selected (skip reload if not the last operation)
            if let status = selectedStatus {
                completedOperations += 1
                try await workspace.bulkUpdateStatus(
                    taskIDs: Array(selectedTaskIDs),
                    to: status,
                    skipReload: completedOperations < totalOperations
                )
            }

            // Execute priority change if selected (skip reload if not the last operation)
            if let priority = selectedPriority {
                completedOperations += 1
                try await workspace.bulkUpdatePriority(
                    taskIDs: Array(selectedTaskIDs),
                    to: priority,
                    skipReload: completedOperations < totalOperations
                )
            }

            // Execute tag modifications (always reload after this if executed)
            if hasTagChange {
                try await workspace.bulkUpdateTags(
                    taskIDs: Array(selectedTaskIDs),
                    addTags: addTags,
                    removeTags: removeTags,
                    skipReload: false
                )
            }

            // Notify parent of successful completion
            onCompletion?()
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func parseTags(_ text: String) -> [String] {
        text.split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
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
}

// MARK: - Preview

#Preview("Bulk Actions") {
    BulkActionsView(
        workspace: PreviewWorkspaceSupport.makeWorkspace(label: "bulk-actions"),
        selectedTaskIDs: ["RQ-0001", "RQ-0002", "RQ-0003"],
        onCompletion: nil
    )
}
