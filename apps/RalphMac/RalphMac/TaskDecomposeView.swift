/**
 TaskDecomposeView

 Responsibilities:
 - Present a dedicated UI for previewing and writing task decompositions.
 - Support freeform decomposition and existing-task decomposition from a selected task context.
 - Render CLI preview results, warnings, blockers, and write summaries without reimplementing planner logic.

 Does not handle:
 - Direct queue mutation outside the CLI.
 - Long-running stream output or cancellation plumbing.
 - Generic task creation workflows.

 Invariants/assumptions callers must respect:
 - Preview must be run before write.
 - Existing-task mode decomposes the selected task in place; freeform mode may optionally attach under a parent.
 - Any form change invalidates the active preview and requires a fresh preview before write.
 */

import SwiftUI
import RalphCore

@MainActor
struct TaskDecomposeView: View {
    struct PresentationContext: Equatable {
        var selectedTaskID: String?
    }

    private enum SourceMode: String, CaseIterable, Identifiable {
        case freeform
        case existingTask

        var id: String { rawValue }

        var displayName: String {
            switch self {
            case .freeform: return "Freeform Goal"
            case .existingTask: return "Existing Task"
            }
        }
    }

    private enum AccessibilityID {
        static let requestField = "task-decompose-request-field"
        static let taskPicker = "task-decompose-task-picker"
        static let previewButton = "task-decompose-preview-button"
        static let writeButton = "task-decompose-write-button"
        static let summary = "task-decompose-summary"
    }

    @ObservedObject var workspace: Workspace
    let context: PresentationContext
    @Environment(\.dismiss) private var dismiss

    @State private var sourceMode: SourceMode
    @State private var freeformRequest: String = ""
    @State private var selectedTaskID: String?
    @State private var attachToTaskID: String?
    @State private var maxDepth: Int = 3
    @State private var maxChildren: Int = 5
    @State private var maxNodes: Int = 50
    @State private var status: RalphTaskStatus = .draft
    @State private var childPolicy: DecompositionChildPolicy = .fail
    @State private var withDependencies: Bool = false

    @State private var preview: DecompositionPreview?
    @State private var writeResult: TaskDecomposeWriteResult?
    @State private var isPreviewing = false
    @State private var isWriting = false
    @State private var recoveryError: RecoveryError?
    @State private var showingRecoverySheet = false

    init(workspace: Workspace, context: PresentationContext = PresentationContext()) {
        self.workspace = workspace
        self.context = context
        let defaultMode: SourceMode = context.selectedTaskID == nil ? .freeform : .existingTask
        _sourceMode = State(initialValue: defaultMode)
        _selectedTaskID = State(initialValue: context.selectedTaskID)
        _attachToTaskID = State(initialValue: context.selectedTaskID)
    }

    private var isBusy: Bool {
        isPreviewing || isWriting
    }

    private var canPreview: Bool {
        sourceInput != nil && !isBusy
    }

    private var canWrite: Bool {
        preview != nil && preview?.writeBlockers.isEmpty == true && !isBusy
    }

    private var sourceInput: TaskDecomposeSourceInput? {
        switch sourceMode {
        case .freeform:
            let trimmed = freeformRequest.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed.isEmpty ? nil : .freeform(trimmed)
        case .existingTask:
            guard let selectedTaskID, !selectedTaskID.isEmpty else { return nil }
            return .existingTaskID(selectedTaskID)
        }
    }

    private var options: TaskDecomposeOptions {
        TaskDecomposeOptions(
            attachToTaskID: sourceMode == .freeform ? attachToTaskID : nil,
            maxDepth: maxDepth,
            maxChildren: maxChildren,
            maxNodes: maxNodes,
            status: status,
            childPolicy: childPolicy,
            withDependencies: withDependencies
        )
    }

    private var availableTasks: [RalphTask] {
        workspace.tasks.sorted { lhs, rhs in
            if lhs.status != rhs.status {
                return lhs.status.displayName < rhs.status.displayName
            }
            return lhs.id < rhs.id
        }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Form {
                    sourceSection()
                    optionsSection()
                    previewSection()
                }
                .formStyle(.grouped)

                Divider()

                footerBar()
                    .padding(.horizontal, 20)
                    .padding(.vertical, 16)
            }
            .navigationTitle("Decompose Task")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(isBusy)
                }
            }
        }
        .frame(minWidth: 760, minHeight: 680)
        .sheet(isPresented: $showingRecoverySheet) {
            if let recoveryError {
                ErrorRecoverySheet(
                    error: recoveryError,
                    workspace: workspace,
                    onRetry: {
                        showingRecoverySheet = false
                        if preview == nil || writeResult != nil {
                            previewDecomposition()
                        } else {
                            writeDecomposition()
                        }
                    },
                    onDismiss: {
                        showingRecoverySheet = false
                        self.recoveryError = nil
                    }
                )
            }
        }
        .onChange(of: sourceMode) { _, _ in invalidatePreview() }
        .onChange(of: freeformRequest) { _, _ in invalidatePreview() }
        .onChange(of: selectedTaskID) { _, _ in invalidatePreview() }
        .onChange(of: attachToTaskID) { _, _ in invalidatePreview() }
        .onChange(of: maxDepth) { _, _ in invalidatePreview() }
        .onChange(of: maxChildren) { _, _ in invalidatePreview() }
        .onChange(of: maxNodes) { _, _ in invalidatePreview() }
        .onChange(of: status) { _, _ in invalidatePreview() }
        .onChange(of: childPolicy) { _, _ in invalidatePreview() }
        .onChange(of: withDependencies) { _, _ in invalidatePreview() }
    }

    @ViewBuilder
    private func sourceSection() -> some View {
        Section("Source") {
            Picker("Decompose", selection: $sourceMode) {
                ForEach(SourceMode.allCases) { mode in
                    Text(mode.displayName).tag(mode)
                }
            }
            .pickerStyle(.segmented)

            switch sourceMode {
            case .freeform:
                TextField("Describe the goal to decompose", text: $freeformRequest, axis: .vertical)
                    .lineLimit(3...6)
                    .accessibilityIdentifier(AccessibilityID.requestField)

                Picker("Attach under", selection: Binding(
                    get: { attachToTaskID ?? "" },
                    set: { attachToTaskID = $0.isEmpty ? nil : $0 }
                )) {
                    Text("None").tag("")
                    ForEach(availableTasks) { task in
                        Text("\(task.id) · \(task.title)").tag(task.id)
                    }
                }
                .pickerStyle(.menu)
            case .existingTask:
                Picker("Task", selection: Binding(
                    get: { selectedTaskID ?? "" },
                    set: { selectedTaskID = $0.isEmpty ? nil : $0 }
                )) {
                    Text("Select a task").tag("")
                    ForEach(availableTasks) { task in
                        Text("\(task.id) · \(task.title)").tag(task.id)
                    }
                }
                .pickerStyle(.menu)
                .accessibilityIdentifier(AccessibilityID.taskPicker)

                if let selectedTask = availableTasks.first(where: { $0.id == selectedTaskID }) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(selectedTask.title)
                            .font(.headline)
                        Text("This keeps the selected task as the parent and creates children beneath it.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    @ViewBuilder
    private func optionsSection() -> some View {
        Section("Options") {
            Stepper("Max depth: \(maxDepth)", value: $maxDepth, in: 1...10)
            Stepper("Max children: \(maxChildren)", value: $maxChildren, in: 1...25)
            Stepper("Max nodes: \(maxNodes)", value: $maxNodes, in: 1...200, step: 5)

            Picker("Child policy", selection: $childPolicy) {
                ForEach(DecompositionChildPolicy.allCases) { policy in
                    Text(policy.displayName).tag(policy)
                }
            }
            .pickerStyle(.menu)

            Text(childPolicy.helpText)
                .font(.caption)
                .foregroundStyle(.secondary)

            Picker("New task status", selection: $status) {
                ForEach([RalphTaskStatus.draft, .todo], id: \.self) { status in
                    Text(status.displayName).tag(status)
                }
            }
            .pickerStyle(.menu)

            Toggle("Infer sibling dependencies", isOn: $withDependencies)
        }
    }

    @ViewBuilder
    private func previewSection() -> some View {
        Section("Preview") {
            if isBusy {
                HStack(spacing: 12) {
                    ProgressView()
                    Text(isWriting ? "Writing decomposition..." : "Generating preview...")
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 8)
            }

            if let preview {
                VStack(alignment: .leading, spacing: 12) {
                    summaryCard(preview)
                    if !preview.plan.warnings.isEmpty {
                        messageBlock(title: "Warnings", messages: preview.plan.warnings, color: .yellow)
                    }
                    if !preview.writeBlockers.isEmpty {
                        messageBlock(title: "Write Blockers", messages: preview.writeBlockers, color: .red)
                    }
                    if !preview.plan.dependencyEdges.isEmpty {
                        messageBlock(
                            title: "Dependencies",
                            messages: preview.plan.dependencyEdges.map { "\($0.taskTitle) depends on \($0.dependsOnTitle)" },
                            color: .blue
                        )
                    }
                    previewTree(node: preview.plan.root, depth: 0)
                }
            } else {
                Text("Run a preview to inspect the proposed hierarchy, warnings, and write blockers before mutating the queue.")
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 8)
            }
        }
    }

    @ViewBuilder
    private func summaryCard(_ preview: DecompositionPreview) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(summaryTitle(for: preview))
                .font(.headline)
            Text("\(preview.plan.totalNodes) nodes · \(preview.plan.leafNodes) leaves · child policy \(preview.childPolicy.displayName.lowercased())")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let attachTarget = preview.attachTarget {
                Text("Attach target: \(attachTarget.task.id) · \(attachTarget.task.title)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(Color.accentColor.opacity(0.08))
        .clipShape(.rect(cornerRadius: 10))
        .accessibilityIdentifier(AccessibilityID.summary)
    }

    @ViewBuilder
    private func messageBlock(title: String, messages: [String], color: Color) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(color)
            ForEach(messages, id: \.self) { message in
                Text("• \(message)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(color.opacity(0.08))
        .clipShape(.rect(cornerRadius: 10))
    }

    private func previewTree(node: PlannedNode, depth: Int) -> AnyView {
        AnyView(
            VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Text(String(repeating: "  ", count: depth) + "•")
                    .foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 4) {
                    Text(node.title)
                        .font(.subheadline.weight(.medium))
                    Text(node.plannerKey)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    if let description = node.description, !description.isEmpty {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            ForEach(node.children) { child in
                previewTree(node: child, depth: depth + 1)
            }
            }
        )
    }

    @ViewBuilder
    private func footerBar() -> some View {
        HStack {
            if let writeResult {
                Text(writeSummary(writeResult))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text("Preview first. Write is only enabled when the current preview has no blockers.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button("Preview") { previewDecomposition() }
                .disabled(!canPreview)
                .accessibilityIdentifier(AccessibilityID.previewButton)

            Button("Write") { writeDecomposition() }
                .buttonStyle(.borderedProminent)
                .disabled(!canWrite)
                .accessibilityIdentifier(AccessibilityID.writeButton)
        }
    }

    private func summaryTitle(for preview: DecompositionPreview) -> String {
        switch preview.source {
        case .freeform(let request):
            return request
        case .existingTask(let task):
            return "\(task.id) · \(task.title)"
        }
    }

    private func writeSummary(_ result: TaskDecomposeWriteResult) -> String {
        let created = result.createdIDs.count
        let replaced = result.replacedIDs.count
        if replaced > 0 {
            return "Created \(created) tasks and replaced \(replaced) previous children."
        }
        return "Created \(created) tasks."
    }

    private func invalidatePreview() {
        preview = nil
        writeResult = nil
    }

    private func previewDecomposition() {
        guard let sourceInput else { return }
        isPreviewing = true
        writeResult = nil

        Task { @MainActor in
            do {
                preview = try await workspace.previewTaskDecomposition(source: sourceInput, options: options)
                isPreviewing = false
            } catch {
                isPreviewing = false
                present(error: error, operation: "previewTaskDecompose")
            }
        }
    }

    private func writeDecomposition() {
        guard let sourceInput else { return }
        isWriting = true

        Task { @MainActor in
            do {
                writeResult = try await workspace.writeTaskDecomposition(source: sourceInput, options: options)
                isWriting = false
                dismiss()
            } catch {
                isWriting = false
                present(error: error, operation: "writeTaskDecompose")
            }
        }
    }

    private func present(error: any Error, operation: String) {
        recoveryError = RecoveryError.classify(
            error: error,
            operation: operation,
            workspaceURL: workspace.workingDirectoryURL
        )
        showingRecoverySheet = true
    }
}

#Preview {
    TaskDecomposeView(workspace: Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp")))
}
