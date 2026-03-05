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
public import RalphCore

@MainActor
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
        if localTask.agent != externalTask.agent { initialSelections["agent"] = .external }
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

                if localTask.agent != externalTask.agent {
                    Section("Execution Overrides") {
                        mergeRow(
                            field: "agent",
                            label: "Agent Overrides",
                            localValue: formatAgent(localTask.agent),
                            externalValue: formatAgent(externalTask.agent)
                        )
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
                    .font(.body.weight(.semibold))
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
        .clipShape(.rect(cornerRadius: 8))
    }
    
    @ViewBuilder
    private func mergeRow(field: String, label: String, localValue: String, externalValue: String) -> some View {
        let hasConflict = localValue != externalValue
        
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(label)
                    .font(.subheadline)
                    .font(.body.weight(.medium))
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
                        .clipShape(.rect(cornerRadius: 4))
                        
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
                        .clipShape(.rect(cornerRadius: 4))
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
                case "agent":
                    newTask.agent = localTask.agent
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

    private func formatAgent(_ agent: RalphTaskAgent?) -> String {
        guard let agent else { return "(none)" }

        var parts: [String] = []
        if let runner = agent.runner, !runner.isEmpty { parts.append("runner=\(runner)") }
        if let model = agent.model, !model.isEmpty { parts.append("model=\(model)") }
        if let effort = agent.modelEffort, !effort.isEmpty { parts.append("effort=\(effort)") }
        if let phases = agent.phases { parts.append("phases=\(phases)") }
        if let iterations = agent.iterations { parts.append("iterations=\(iterations)") }
        if let phaseOverrides = agent.phaseOverrides, !phaseOverrides.isEmpty {
            parts.append("phase_overrides=yes")
        }
        if parts.isEmpty { return "(none)" }
        return parts.joined(separator: ", ")
    }
    
    private func formatDate(_ date: Date) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

// MARK: - Error Recovery Views

/// Extended guidance for offline scenarios
extension ErrorCategory {
    /// Extended guidance message for offline scenarios with specific troubleshooting steps
    public var offlineGuidance: String? {
        switch self {
        case .cliUnavailable:
            return """
            The Ralph CLI is not available. This can happen when:
            • The app bundle is damaged or incomplete
            • The ralph binary was moved or deleted
            • Antivirus software quarantined the binary
            
            Try reinstalling Ralph or checking your security software.
            """
        case .permissionDenied:
            return """
            Ralph cannot access the workspace directory. This can happen when:
            • The directory was moved or deleted
            • File permissions changed
            • The workspace is on a disconnected drive
            
            Check that the workspace path is still valid and accessible.
            """
        case .networkError:
            return """
            A network-related operation timed out. This can happen when:
            • The CLI took too long to respond
            • The system is under heavy load
            • There's a resource deadlock
            
            Try again in a moment.
            """
        default:
            return guidanceMessage
        }
    }
}

/// SwiftUI color extension for ErrorCategory
extension ErrorCategory {
    var swiftUIColor: Color {
        switch self {
        case .cliUnavailable: return .orange
        case .permissionDenied: return .red
        case .parseError: return .yellow
        case .networkError: return .blue
        case .queueCorrupted: return .red
        case .resourceBusy: return .orange
        case .versionMismatch: return .purple
        case .unknown: return .gray
        }
    }
}

/**
 ErrorRecoveryView

 Responsibilities:
 - Display rich error information with category-specific styling
 - Provide contextual recovery actions based on error category
 - Support retry, diagnose, copy error details, and dismiss actions
 - Show guidance messages based on error type

 Does not handle:
 - Direct error recovery execution (delegates to callbacks)
 - Error classification (receives pre-classified RecoveryError)
 - Queue validation directly (delegates to workspace)

 Invariants/assumptions callers must respect:
 - error is a properly classified RecoveryError
 - All action callbacks are provided
 - View is displayed within a valid SwiftUI view hierarchy
 - Workspace is available for diagnostic operations
 */
@MainActor
struct ErrorRecoveryView: View {
    let error: RecoveryError
    let workspace: Workspace?
    let onRetry: () -> Void
    let onDismiss: () -> Void

    @State private var showingDiagnoseSheet = false
    @State private var diagnoseOutput: String = ""
    @State private var isDiagnosing = false
    @State private var showingLogsSheet = false
    @State private var logsContent: String = ""
    @State private var isLoadingLogs = false

    var body: some View {
        VStack(spacing: 20) {
            // Error icon and category
            VStack(spacing: 12) {
                Image(systemName: error.category.icon)
                    .font(.system(size: 48))
                    .foregroundStyle(error.category.swiftUIColor)
                    .accessibilityLabel("Error: \(error.category.displayName)")

                Text(error.category.displayName)
                    .font(.headline)
                    .foregroundStyle(error.category.swiftUIColor)

                Text(error.message)
                    .font(.body)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.primary)
            }

            // Guidance message if available (use offline guidance if applicable)
            if let guidance = error.category.offlineGuidance ?? error.category.guidanceMessage {
                Text(guidance)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)
            }

            // Recovery actions
            VStack(spacing: 8) {
                ForEach(error.category.suggestedActions, id: \.self) { action in
                    recoveryButton(for: action)
                }
            }
            .padding(.top, 8)
        }
        .padding(24)
        .frame(maxWidth: 400)
        .background(.ultraThinMaterial)
        .clipShape(.rect(cornerRadius: 12))
        .sheet(isPresented: $showingDiagnoseSheet) {
            DiagnoseResultView(output: diagnoseOutput, isLoading: isDiagnosing)
        }
        .sheet(isPresented: $showingLogsSheet) {
            LogsView(logs: logsContent, isLoading: isLoadingLogs)
        }
    }

    @ViewBuilder
    private func recoveryButton(for action: RecoveryAction) -> some View {
        switch action {
        case .retry:
            Button(action: onRetry) {
                Label("Retry", systemImage: "arrow.clockwise")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityLabel("Retry the operation")

        case .diagnose:
            Button(action: performDiagnosis) {
                Label("Diagnose", systemImage: "stethoscope")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isDiagnosing)
            .accessibilityLabel("Run diagnostic commands")

        case .copyErrorDetails:
            Button(action: copyErrorDetails) {
                Label("Copy Error Details", systemImage: "doc.on.doc")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Copy error details to clipboard")

        case .openLogs:
            Button(action: openLogs) {
                Label("View Logs", systemImage: "doc.text.magnifyingglass")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Open Ralph logs")

        case .dismiss:
            Button(action: onDismiss) {
                Label("Dismiss", systemImage: "xmark")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Dismiss error")

        case .checkPermissions:
            Button(action: checkPermissions) {
                Label("Check Permissions", systemImage: "folder.badge.gearshape")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Open workspace to check permissions")

        case .reinstallCLI:
            Button(action: openReinstallationHelp) {
                Label("Reinstallation Help", systemImage: "arrow.down.circle")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Open reinstallation help")

        case .validateQueue:
            Button(action: performDiagnosis) {
                Label("Validate Queue", systemImage: "checkmark.shield")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .disabled(isDiagnosing)
            .accessibilityLabel("Validate queue file")
        }
    }

    private func copyErrorDetails() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(error.fullErrorDetails, forType: .string)
    }

    private func performDiagnosis() {
        isDiagnosing = true
        showingDiagnoseSheet = true

        Task { @MainActor in
            diagnoseOutput = await runQueueValidation()
            isDiagnosing = false
        }
    }

    private func openLogs() {
        showingLogsSheet = true
        isLoadingLogs = true

        if RalphLogger.shared.canExportLogs {
            RalphLogger.shared.exportLogs(hours: 2) { logs in
                DispatchQueue.main.async {
                    logsContent = logs ?? "No logs available"
                    isLoadingLogs = false
                }
            }
        } else {
            logsContent = "Log export requires macOS 12.0+"
            isLoadingLogs = false
        }
    }

    private func checkPermissions() {
        if let url = error.workspaceURL ?? workspace?.workingDirectoryURL {
            NSWorkspace.shared.open(url)
        }
    }

    private func openReinstallationHelp() {
        if let url = URL(string: "https://github.com/mitchfultz/ralph#installation") {
            NSWorkspace.shared.open(url)
        }
    }

    private func runQueueValidation() async -> String {
        guard let workspace = workspace else {
            return "Error: No workspace available for validation"
        }

        guard workspace.hasRalphQueueFile else {
            return "⚠️ Queue validation skipped\n\nNo `.ralph/queue.jsonc` found in \(workspace.workingDirectoryURL.path).\nRun `ralph init --non-interactive` in this directory first."
        }

        do {
            let client: RalphCLIClient
            if let managerClient = WorkspaceManager.shared.client {
                client = managerClient
            } else {
                client = try RalphCLIClient.bundled()
            }
            let result = try await client.runAndCollect(
                arguments: ["--no-color", "queue", "validate"],
                currentDirectoryURL: workspace.workingDirectoryURL
            )

            if result.status.code == 0 {
                return "✅ Queue validation passed\n\n\(result.stdout)"
            } else {
                return "❌ Queue validation failed\n\nExit code: \(result.status.code)\n\(result.stderr)"
            }
        } catch {
            return "❌ Failed to run validation: \(error.localizedDescription)"
        }
    }
}

// MARK: - Supporting Views

@MainActor
struct DiagnoseResultView: View {
    let output: String
    let isLoading: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack {
                if isLoading {
                    ProgressView("Running diagnostics...")
                        .padding()
                } else {
                    ScrollView {
                        Text(output)
                            .font(.system(.body, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                            .textSelection(.enabled)
                    }
                }
            }
            .navigationTitle("Diagnostic Results")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(output, forType: .string)
                    }
                    .disabled(isLoading)
                }
            }
        }
        .frame(minWidth: 500, minHeight: 300)
    }
}

@MainActor
struct LogsView: View {
    let logs: String
    let isLoading: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack {
                if isLoading {
                    ProgressView("Loading logs...")
                        .padding()
                } else {
                    ScrollView {
                        Text(logs)
                            .font(.system(.body, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                            .textSelection(.enabled)
                    }
                }
            }
            .navigationTitle("Ralph Logs")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Copy") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(logs, forType: .string)
                    }
                    .disabled(isLoading)
                }
            }
        }
        .frame(minWidth: 600, minHeight: 400)
    }
}

// MARK: - Error Recovery Sheet

@MainActor
struct ErrorRecoverySheet: View {
    let error: RecoveryError
    let workspace: Workspace?
    let onRetry: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        VStack {
            ErrorRecoveryView(
                error: error,
                workspace: workspace,
                onRetry: onRetry,
                onDismiss: onDismiss
            )
        }
        .padding()
        .frame(minWidth: 450, minHeight: 400)
    }
}

// MARK: - Preview
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

#Preview("Error Recovery") {
    ErrorRecoveryView(
        error: RecoveryError(
            category: .cliUnavailable,
            message: "Failed to load tasks",
            underlyingError: "Executable not found at expected path",
            operation: "loadTasks",
            suggestions: ["Check installation", "Verify permissions"]
        ),
        workspace: nil,
        onRetry: {},
        onDismiss: {}
    )
}
