/**
 TaskListView

 Responsibilities:
 - Display a rich, sortable, filterable list of tasks from the Ralph queue.
 - Show task metadata with visual indicators (status badges, priority dots, tag chips).
 - Provide search, filtering, and sorting controls.
 - Display "What's Next" section highlighting the next todo task.
 - Support two-way binding for task selection to integrate with NavigationSplitView.

 Does not handle:
 - Task editing (see TaskDetailView - now shown in detail column).
 - Task creation (see TaskCreationView).
 - Direct CLI execution (delegates to Workspace).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - selectedTaskID is bound to external navigation state.
 - Tasks are loaded via workspace.loadTasks() before display.
 */

import SwiftUI
import RalphCore

@MainActor
struct TaskListView: View {
    @ObservedObject var workspace: Workspace
    @Binding var selectedTaskID: String?
    @Binding var selectedTaskIDs: Set<String>
    let showTaskCreation: () -> Void
    let showTaskDecompose: (String?) -> Void
    let showTaskDetail: (String) -> Void
    @State private var showingBulkActions = false
    @State private var recentlyChangedTaskIDs: Set<String> = []
    @State private var showExternalUpdateBanner = false
    
    // MARK: - Keyboard Navigation State
    @FocusState private var focusedTaskID: String?

    var body: some View {
        VStack(spacing: 0) {
            // What's Next section
            if let nextTask = workspace.nextTask() {
                whatsNextSection(task: nextTask)
                    .padding(.horizontal, 16)
                    .padding(.top, 16)
                    .padding(.bottom, 12)
            }

            // Filter and sort controls
            filterControls()
                .padding(.horizontal, 16)
                .padding(.bottom, 12)

            // External update banner
            externalUpdateBanner()
                .padding(.horizontal, 16)
                .padding(.top, showExternalUpdateBanner ? 8 : 0)
            
            // Task list
            taskList()
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
        }
        .background(.clear)
        .task { @MainActor in
            await workspace.loadTasks()
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button(action: showTaskCreation) {
                    Label("New Task", systemImage: "plus")
                }
                .accessibilityIdentifier("new-task-toolbar-button")

                Button(action: {
                    showTaskDecompose(selectedTaskID)
                }) {
                    Label("Decompose", systemImage: "square.split.2x2")
                }
                .disabled(selectedTaskID == nil && workspace.tasks.isEmpty)
                .accessibilityIdentifier("task-decompose-toolbar-button")
            }
            
            // Add bulk actions button when multi-select is active
            if selectedTaskIDs.count > 1 {
                ToolbarItem(placement: .cancellationAction) {
                    Button(action: { showingBulkActions = true }) {
                        Label("Bulk Actions", systemImage: "rectangle.stack")
                    }
                    .help("Perform bulk actions on \(selectedTaskIDs.count) selected tasks")
                }
            }
        }
        .sheet(isPresented: $showingBulkActions) {
            BulkActionsView(
                workspace: workspace,
                selectedTaskIDs: selectedTaskIDs,
                onCompletion: {
                    // Clear selection after successful bulk operation
                    selectedTaskIDs.removeAll()
                    selectedTaskID = nil
                }
            )
        }
        .task(id: workspace.lastQueueRefreshEvent?.id) {
            await handleQueueRefreshEvent()
        }
        .onChange(of: selectedTaskIDs) { _, newSelection in
            syncPrimarySelection(with: newSelection)
        }
    }

    // MARK: - What's Next Section

    @ViewBuilder
    private func whatsNextSection(task: RalphTask) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label("What's Next", systemImage: "sparkles")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Spacer()

                priorityDot(priority: task.priority)
            }

            Text(task.title)
                .font(.system(.body, design: .default))
                .lineLimit(2)

            HStack {
                statusBadge(status: task.status)

                if !task.tags.isEmpty {
                    tagChips(tags: Array(task.tags.prefix(3)))
                }

                Spacer()

                Text(task.id)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospaced()
            }
        }
        .padding(12)
        .background(Color.accentColor.opacity(0.1))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(Color.accentColor.opacity(0.3), lineWidth: 1)
        )
        .clipShape(.rect(cornerRadius: 10))
        .contentShape(Rectangle())
        .onTapGesture {
            handleTaskSelection(taskID: task.id, modifierFlags: NSEvent.modifierFlags)
        }
        .accessibilityLabel("What's Next: \(task.title)")
        .accessibilityHint("Double click to open task details")
        .accessibilityValue("Priority \(task.priority.displayName), Status \(task.status.displayName)")
    }

    // MARK: - Filter Controls

    @ViewBuilder
    private func filterControls() -> some View {
        VStack(spacing: 8) {
            // Search bar
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)

                TextField("Search tasks...", text: $workspace.taskFilterText)
                    .textFieldStyle(.plain)
                    .accessibilityIdentifier("task-search-field")

                if !workspace.taskFilterText.isEmpty {
                    Button(action: { workspace.taskFilterText = "" }) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear search")
                }
            }
            .padding(8)
            .background(.tertiary.opacity(0.1))
            .clipShape(.rect(cornerRadius: 8))

            // Filter and sort row
            HStack(spacing: 12) {
                // Status filter
                Picker("Status", selection: $workspace.taskStatusFilter) {
                    Text("All Status")
                        .tag(nil as RalphTaskStatus?)
                    ForEach(RalphTaskStatus.allCases, id: \.self) { status in
                        Text(status.displayName)
                            .tag(status as RalphTaskStatus?)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Filter by Status")

                // Priority filter
                Picker("Priority", selection: $workspace.taskPriorityFilter) {
                    Text("All Priorities")
                        .tag(nil as RalphTaskPriority?)
                    ForEach(RalphTaskPriority.allCases, id: \.self) { priority in
                        Text(priority.displayName)
                            .tag(priority as RalphTaskPriority?)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Filter by Priority")

                Spacer()

                // Sort picker
                Picker("Sort", selection: $workspace.taskSortBy) {
                    ForEach(Workspace.TaskSortOption.allCases, id: \.self) { option in
                        Text(option.rawValue)
                            .tag(option)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Sort tasks")

                // Sort direction toggle
                Button(action: { workspace.taskSortAscending.toggle() }) {
                    Image(systemName: workspace.taskSortAscending ? "arrow.up" : "arrow.down")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Toggle sort direction")
                .accessibilityHint("Currently sorted \(workspace.taskSortAscending ? "ascending" : "descending")")

                Divider()
                    .frame(height: 20)

                // Refresh button
                Button(action: {
                    Task { @MainActor in
                        await workspace.loadTasks()
                    }
                }) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(workspace.tasksLoading)
                .accessibilityLabel("Refresh task list")
            }
        }
    }

    // MARK: - Task List

    @ViewBuilder
    private func taskList() -> some View {
        let presentation = workspace.taskPresentation()
        let filteredTasks = presentation.tasks

        VStack(spacing: 0) {
            if workspace.tasksLoading {
                ProgressView("Loading tasks...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let recoveryError = workspace.lastRecoveryError,
                      workspace.showErrorRecovery {
                ErrorRecoveryView(
                    error: recoveryError,
                    workspace: workspace,
                    onRetry: {
                        Task { @MainActor in
                            workspace.clearErrorRecovery()
                            await workspace.loadTasks()
                        }
                    },
                    onDismiss: {
                        workspace.clearErrorRecovery()
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = workspace.tasksErrorMessage {
                VStack(spacing: 12) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.largeTitle)
                        .foregroundStyle(.red)
                        .accessibilityLabel("Error")
                    Text("Failed to load tasks")
                        .font(.headline)
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if filteredTasks.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "checkmark.circle")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                    Text("No tasks found")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(filteredTasks, selection: $selectedTaskIDs) { task in
                    TaskRow(
                        task: task,
                        isHighlighted: recentlyChangedTaskIDs.contains(task.id),
                        isSelected: selectedTaskIDs.contains(task.id),
                        isFocused: focusedTaskID == task.id
                    )
                        .tag(task.id)
                        .focused($focusedTaskID, equals: task.id)
                        .contextMenu {
                            Button("Decompose Task...") {
                                handleTaskSelection(taskID: task.id, modifierFlags: [])
                                focusedTaskID = task.id
                                showTaskDecompose(task.id)
                            }
                        }
                        .onTapGesture {
                            handleTaskSelection(taskID: task.id, modifierFlags: NSEvent.modifierFlags)
                            focusedTaskID = task.id
                        }
                        // Add slide-in animation for new tasks
                        .transition(.asymmetric(
                            insertion: .slide.combined(with: .opacity),
                            removal: .opacity
                        ))
                        .listRowSeparator(.visible)
                        .listRowInsets(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
                        .contentShape(Rectangle())
                }
                .listStyle(.plain)
                .accessibilityIdentifier("task-list-container")
                #if swift(>=5.9)
                .alternatingRowBackgrounds(.automatic)
                #endif
                // MARK: - Keyboard Navigation Handlers
                .onKeyPress(.upArrow) {
                    navigateTask(direction: -1, tasks: filteredTasks)
                    return .handled
                }
                .onKeyPress(.downArrow) {
                    navigateTask(direction: 1, tasks: filteredTasks)
                    return .handled
                }
                .onKeyPress(.return) {
                    if let taskID = focusedTaskID ?? selectedTaskID {
                        selectedTaskID = taskID
                        selectedTaskIDs = [taskID]
                        showTaskDetail(taskID)
                    }
                    return .handled
                }
                .onKeyPress(.space) {
                    // Space key opens task detail (same as Enter for quick access)
                    if let taskID = focusedTaskID ?? selectedTaskID {
                        selectedTaskID = taskID
                        selectedTaskIDs = [taskID]
                        showTaskDetail(taskID)
                    }
                    return .handled
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(.separator.opacity(0.3), lineWidth: 0.5)
        )
        .clipShape(.rect(cornerRadius: 10))
    }

    // MARK: - Helper Views

    @ViewBuilder
    private func statusBadge(status: RalphTaskStatus) -> some View {
        Text(status.displayName)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(statusColor(status).opacity(0.2))
            .foregroundStyle(statusColor(status))
            .clipShape(.rect(cornerRadius: 4))
    }

    @ViewBuilder
    private func priorityDot(priority: RalphTaskPriority) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(priorityColor(priority))
                .frame(width: 8, height: 8)
            Text(priority.displayName)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityLabel("Priority: \(priority.displayName)")
    }

    @ViewBuilder
    private func tagChips(tags: [String]) -> some View {
        HStack(spacing: 4) {
            ForEach(tags, id: \.self) { tag in
                Text(tag)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(.secondary.opacity(0.15))
                    .foregroundStyle(.secondary)
                    .cornerRadius(4)
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
    
    // MARK: - Task Selection
    
    /// Handle task selection with support for Cmd+click multi-select
    /// - Parameters:
    ///   - taskID: The task ID being selected
    ///   - modifierFlags: The modifier flags from the click event
    private func handleTaskSelection(taskID: String, modifierFlags: NSEvent.ModifierFlags) {
        if modifierFlags.contains(.command) {
            // Cmd+click: toggle selection in multi-select set
            if selectedTaskIDs.contains(taskID) {
                selectedTaskIDs.remove(taskID)
                // Only update selectedTaskID if we deselected the current primary
                if selectedTaskID == taskID {
                    selectedTaskID = selectedTaskIDs.first
                }
                // If we removed the last selected task, clear selectedTaskID too
                if selectedTaskIDs.isEmpty {
                    selectedTaskID = nil
                }
            } else {
                selectedTaskIDs.insert(taskID)
                selectedTaskID = taskID
            }
        } else {
            // Normal click: single selection
            selectedTaskID = taskID
            selectedTaskIDs = [taskID]
        }
    }
    
    // MARK: - Keyboard Navigation
    
    private func navigateTask(direction: Int, tasks: [RalphTask]) {
        let currentID = focusedTaskID ?? selectedTaskID
        
        guard let currentIndex = tasks.firstIndex(where: { $0.id == currentID }) else {
            // No selection, select first task
            if let first = tasks.first {
                withAnimation(.easeInOut(duration: 0.15)) {
                    focusedTaskID = first.id
                    selectedTaskID = first.id
                    selectedTaskIDs = [first.id]
                }
            }
            return
        }
        
        let newIndex = currentIndex + direction
        guard newIndex >= 0 && newIndex < tasks.count else { return }
        
        let newTask = tasks[newIndex]
        withAnimation(.easeInOut(duration: 0.15)) {
            focusedTaskID = newTask.id
            selectedTaskID = newTask.id
            selectedTaskIDs = [newTask.id]
        }
    }
    
    // MARK: - External Change Handling
    
    private func handleQueueRefreshEvent() async {
        guard let refreshEvent = workspace.lastQueueRefreshEvent,
              refreshEvent.source == .externalFileChange else {
            return
        }

        // Show banner notification
        withAnimation(.easeInOut(duration: 0.3)) {
            showExternalUpdateBanner = true
        }

        withAnimation(.easeInOut(duration: 0.2)) {
            recentlyChangedTaskIDs = refreshEvent.highlightedTaskIDs
        }

        do {
            try await Task.sleep(for: .milliseconds(2000))
            withAnimation(.easeInOut(duration: 0.5)) {
                recentlyChangedTaskIDs.removeAll()
            }

            try await Task.sleep(for: .milliseconds(1000))
            withAnimation(.easeInOut(duration: 0.3)) {
                showExternalUpdateBanner = false
            }
        } catch {
            return
        }
    }
    
    // MARK: - External Update Banner
    
    @ViewBuilder
    private func externalUpdateBanner() -> some View {
        if showExternalUpdateBanner {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .foregroundStyle(.blue)
                Text("Queue updated externally")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.primary)
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.blue.opacity(0.1))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(.blue.opacity(0.3), lineWidth: 1)
            )
            .cornerRadius(6)
            .transition(.move(edge: .top).combined(with: .opacity))
            .accessibilityLabel("Queue updated externally")
        }
    }

    private func syncPrimarySelection(with selection: Set<String>) {
        if selection.isEmpty {
            selectedTaskID = nil
            focusedTaskID = nil
            return
        }

        if let currentID = selectedTaskID, selection.contains(currentID) {
            focusedTaskID = currentID
            return
        }

        let nextPrimaryTaskID = workspace.taskPresentation().orderedTaskIDs
            .first(where: selection.contains)
            ?? selection.sorted().first

        selectedTaskID = nextPrimaryTaskID
        focusedTaskID = nextPrimaryTaskID
    }
}

// MARK: - Task Row

struct TaskRow: View {
    let task: RalphTask
    var isHighlighted: Bool = false
    var isSelected: Bool = false
    var isFocused: Bool = false

    var body: some View {
        HStack(spacing: 12) {
            // Priority indicator
            Circle()
                .fill(priorityColor(task.priority))
                .frame(width: 8, height: 8)
                .accessibilityLabel("Priority: \(task.priority.displayName)")

            VStack(alignment: .leading, spacing: 4) {
                // Title
                Text(task.title)
                    .font(.system(.body, design: .default))
                    .lineLimit(1)

                HStack(spacing: 8) {
                    // Status badge
                    Text(task.status.displayName)
                        .font(.caption2.weight(.medium))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(statusColor(task.status).opacity(0.2))
                        .foregroundStyle(statusColor(task.status))
                        .cornerRadius(4)
                        .accessibilityLabel("Status: \(task.status.displayName)")

                    // Tags
                    if !task.tags.isEmpty {
                        HStack(spacing: 4) {
                            ForEach(task.tags.prefix(3), id: \.self) { tag in
                                Text(tag)
                                    .font(.caption2)
                                    .padding(.horizontal, 4)
                                    .padding(.vertical, 1)
                                    .background(.secondary.opacity(0.12))
                                    .foregroundStyle(.secondary)
                                    .cornerRadius(3)
                            }
                        }
                        .accessibilityLabel("Tags: \(task.tags.joined(separator: ", "))")
                    }
                }
            }

            Spacer()

            // Task ID
            Text(task.id)
                .font(.caption)
                .foregroundStyle(.secondary)
                .monospaced()
                .accessibilityLabel("Task ID: \(task.id)")
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .background(backgroundColor)
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(isFocused ? Color.accentColor : Color.clear, lineWidth: 2)
        )
        .cornerRadius(6)
        .animation(.easeInOut(duration: 0.2), value: isHighlighted)
        .animation(.easeInOut(duration: 0.15), value: isFocused)
        .animation(.easeInOut(duration: 0.15), value: isSelected)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(task.id): \(task.title)")
        .accessibilityValue("Priority \(task.priority.displayName), Status \(task.status.displayName), Tags: \(task.tags.joined(separator: ", "))")
        .accessibilityHint("Select to view task details. Use arrow keys to navigate.")
        .accessibilityAddTraits(.isButton)
    }
    
    private var backgroundColor: Color {
        if isHighlighted { return Color.accentColor.opacity(0.15) }
        if isSelected { return Color.accentColor.opacity(0.1) }
        return Color.clear
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
